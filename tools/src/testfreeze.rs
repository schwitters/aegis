//! Frozen tests: test code is written (and reviewed) before the implementation and then frozen.
//!
//! `aegis tests freeze` hashes every source file carrying `@verifies` annotations into `doc/testplan/tests.lock`
//! (sha256sum format). The gate stage `test_freeze` rejects modified, deleted or unfrozen test files, so the
//! implementation model cannot weaken the tests it has to pass.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::baseline::hash_file;
use crate::traceability;

pub const LOCK_FILE: &str = "testplan/tests.lock";

#[derive(Debug, Default)]
pub struct Scan {
    /// Files containing `@verifies` (relative path -> absolute path)
    pub test_files: BTreeMap<String, PathBuf>,
    /// Files containing both `@verifies` and `@implements`: tests cannot be frozen without freezing production code
    pub mixed: Vec<String>,
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn scan(root: &Path, src_dirs: &[PathBuf]) -> Result<Scan> {
    let verifies_re = Regex::new(r"@verifies\s+TEST-").unwrap();
    let implements_re = Regex::new(r"@implements\s+REQ-").unwrap();
    let mut s = Scan::default();
    for path in traceability::source_files(src_dirs) {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if verifies_re.is_match(&content) {
            let r = rel(root, &path);
            if implements_re.is_match(&content) {
                s.mixed.push(r.clone());
            }
            s.test_files.insert(r, path);
        }
    }
    Ok(s)
}

pub fn lock_path(doc_dir: &Path) -> PathBuf {
    doc_dir.join(LOCK_FILE)
}

pub fn read_lock(doc_dir: &Path) -> Result<Option<BTreeMap<String, String>>> {
    let path = lock_path(doc_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mut map = BTreeMap::new();
    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (hash, file) = line.split_once("  ").with_context(|| {
            format!("{}:{}: expected `<sha256>  <path>`", path.display(), i + 1)
        })?;
        map.insert(file.trim().to_string(), hash.trim().to_string());
    }
    Ok(Some(map))
}

/// How `freeze_checked` treats newly frozen tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreezeMode {
    /// New tests must compile and fail against the current code (tests written before the implementation).
    FailFirst,
    /// New tests must compile and run, and may pass (brownfield characterization of existing behaviour).
    Characterization,
    /// No execution check (languages without a built-in runner); recorded in the lock header.
    Unchecked,
}

/// Test function names after each `@verifies TEST-…` annotation (Rust `fn`, Python `def`) of one file.
pub fn test_fns_in(text: &str) -> Vec<String> {
    let ann = Regex::new(r"@verifies\s+TEST-").unwrap();
    let fn_re = Regex::new(r"\b(?:fn|def)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if ann.is_match(l) {
            if let Some(name) = lines
                .iter()
                .skip(i)
                .take(10)
                .find_map(|l| fn_re.captures(l).map(|c| c[1].to_string()))
            {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// True when the test function `name` carries `#[ignore = "<reason>"]` (a documented, CI-scheduled exception).
pub fn ignored_with_reason(text: &str, name: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let fn_re = Regex::new(&format!(r"\bfn\s+{}\b", regex::escape(name))).unwrap();
    let Some(pos) = lines.iter().position(|l| fn_re.is_match(l)) else {
        return false;
    };
    // only the attribute / doc-comment lines directly above this function
    lines[..pos]
        .iter()
        .rev()
        .take_while(|l| {
            let t = l.trim_start();
            t.starts_with("#[") || t.starts_with("///") || t.starts_with("//")
        })
        .any(|l| {
            let t = l.trim();
            t.starts_with("#[ignore = \"") && t.len() > "#[ignore = \"\"]".len()
        })
}

/// (file, test function) pairs recorded in the lock (`# test <file> <fn>` lines).
pub fn locked_tests(doc_dir: &Path) -> Result<BTreeSet<(String, String)>> {
    let path = lock_path(doc_dir);
    if !path.exists() {
        return Ok(BTreeSet::new());
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .filter_map(|l| l.strip_prefix("# test "))
        .filter_map(|l| {
            l.rsplit_once(' ')
                .map(|(f, n)| (f.to_string(), n.to_string()))
        })
        .collect())
}

/// Result per test name from libtest output lines `test <path> ... ok|FAILED|ignored`.
pub fn parse_libtest(output: &str) -> BTreeMap<String, String> {
    let re = Regex::new(r"(?m)^test (\S+) \.\.\. (ok|FAILED|ignored)").unwrap();
    re.captures_iter(output)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect()
}

/// Freeze with evidence: tests compile, and every newly frozen test function fails (FailFirst) or at least runs
/// (Characterization) against the current code. Only Rust workspaces (Cargo.toml) have a built-in runner.
pub fn freeze_checked(
    root: &Path,
    doc_dir: &Path,
    src_dirs: &[PathBuf],
    mode: FreezeMode,
) -> Result<(PathBuf, usize)> {
    let s = scan(root, src_dirs)?;
    let previous = locked_tests(doc_dir)?;
    let mut new_tests: Vec<(String, String)> = Vec::new();
    for (rel, abs) in &s.test_files {
        let text = std::fs::read_to_string(abs)?;
        for f in test_fns_in(&text) {
            if !previous.contains(&(rel.clone(), f.clone())) {
                new_tests.push((rel.clone(), f));
            }
        }
    }
    if mode != FreezeMode::Unchecked && !new_tests.is_empty() {
        if !root.join("Cargo.toml").exists() {
            anyhow::bail!(
                "no built-in test runner for this project (only Rust/Cargo); freeze with --unchecked after verifying by hand \
                 that the new tests compile and fail"
            );
        }
        let build = std::process::Command::new("cargo")
            .args(["test", "--workspace", "--no-run", "--quiet"])
            .current_dir(root)
            .output()
            .context("cannot run cargo")?;
        if !build.status.success() {
            anyhow::bail!(
                "tests do not compile against the current code — fix the tests or the stubs first:\n{}",
                String::from_utf8_lossy(&build.stderr)
            );
        }
        let run = std::process::Command::new("cargo")
            .args(["test", "--workspace", "--no-fail-fast"])
            .current_dir(root)
            .output()
            .context("cannot run cargo test")?;
        let results = parse_libtest(&String::from_utf8_lossy(&run.stdout));
        let mut violations = Vec::new();
        for (file, f) in &new_tests {
            let ignore_reason = ignored_with_reason(
                &std::fs::read_to_string(root.join(file)).unwrap_or_default(),
                f,
            );
            let outcome = results
                .iter()
                .find(|(name, _)| *name == f || name.ends_with(&format!("::{}", f)))
                .map(|(_, r)| r.as_str());
            match (outcome, mode) {
                (None, _) => violations.push(format!("{} ({}): not executed by cargo test", f, file)),
                (Some("ignored"), _) if ignore_reason => {} // RULE-RS-019: needs external assets, runs in a named CI job
                (Some("ignored"), _) => violations.push(format!(
                    "{} ({}): #[ignore]d without a reason — frozen tests must run (or use #[ignore = \"<why, which CI job>\"])",
                    f, file
                )),
                (Some("ok"), FreezeMode::FailFirst) => violations.push(format!(
                    "{} ({}): passes against the current code — it verifies nothing yet (use --characterization for \
                     tests that document existing behaviour)",
                    f, file
                )),
                _ => {}
            }
        }
        if !violations.is_empty() {
            anyhow::bail!("refusing to freeze:\n  {}", violations.join("\n  "));
        }
    }
    write_lock(doc_dir, &s, mode)
}

/// Freeze without execution check (used by tests and internally).
pub fn freeze(root: &Path, doc_dir: &Path, src_dirs: &[PathBuf]) -> Result<(PathBuf, usize)> {
    let s = scan(root, src_dirs)?;
    write_lock(doc_dir, &s, FreezeMode::Unchecked)
}

fn write_lock(doc_dir: &Path, s: &Scan, mode: FreezeMode) -> Result<(PathBuf, usize)> {
    if !s.mixed.is_empty() {
        anyhow::bail!(
            "Cannot freeze tests that share a file with production code (@implements): {}. \
             Move tests into dedicated test files (e.g. tests/, or `#[cfg(test)] #[path = \"x_tests.rs\"] mod tests;`).",
            s.mixed.join(", ")
        );
    }
    if s.test_files.is_empty() {
        anyhow::bail!("No test files with @verifies annotations found — nothing to freeze.");
    }
    let mut out = format!(
        "# Aegis frozen tests — generated by `aegis tests freeze`. Verify with `sha256sum -c` from the repo root.\n\
         # Implementation agents must never write this file or the files listed in it.\n\
         # mode: {:?}\n",
        mode
    );
    for (r, abs) in &s.test_files {
        out.push_str(&format!("{}  {}\n", hash_file(abs)?, r));
    }
    for (r, abs) in &s.test_files {
        for f in test_fns_in(&std::fs::read_to_string(abs)?) {
            out.push_str(&format!("# test {} {}\n", r, f));
        }
    }
    let path = lock_path(doc_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok((path, s.test_files.len()))
}

/// Returns problems; empty means every test file is frozen and unchanged. `Err` means no lock exists.
pub fn verify(root: &Path, doc_dir: &Path, src_dirs: &[PathBuf]) -> Result<Vec<String>> {
    let lock = read_lock(doc_dir)?.with_context(|| {
        format!(
            "{} missing — run `aegis tests freeze` after the test review",
            LOCK_FILE
        )
    })?;
    let s = scan(root, src_dirs)?;
    let mut problems = Vec::new();

    for m in &s.mixed {
        problems.push(format!(
            "MIXED    {}: contains @verifies and @implements; tests must live in dedicated files",
            m
        ));
    }
    for (file, expected) in &lock {
        let abs = root.join(file);
        if !abs.exists() {
            problems.push(format!(
                "DELETED  {}: frozen test file no longer exists",
                file
            ));
        } else if &hash_file(&abs)? != expected {
            problems.push(format!(
                "MODIFIED {}: frozen test file was changed after freeze",
                file
            ));
        }
    }
    let locked = locked_tests(doc_dir)?;
    for (file, test_fn) in &locked {
        let abs = root.join(file);
        if abs.exists() {
            if let Ok(text) = std::fs::read_to_string(&abs) {
                let current_fns = test_fns_in(&text);
                if !current_fns.contains(test_fn) {
                    problems.push(format!(
                        "DELETED  {}::{}: frozen test function was deleted or renamed",
                        file, test_fn
                    ));
                }
            }
        }
    }
    for file in s.test_files.keys() {
        if !lock.contains_key(file) {
            problems.push(format!(
                "UNFROZEN {}: contains @verifies but is not in {}",
                file, LOCK_FILE
            ));
        }
    }
    Ok(problems)
}
