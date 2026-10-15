//! Approved baselines and suspect links.
//!
//! `doc/baseline.lock` records the content hash of every specification artifact (REQ, ARCH, UC, TEST) at the moment a
//! human approved it. An artifact whose content differs from its baseline is `UNREVIEWED`; every artifact and code
//! location downstream of a changed REQ/ARCH/TEST is `SUSPECT` until it has been re-reviewed and re-baselined.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::traceability::{self, Trace};

pub const BASELINE_FILE: &str = "baseline.lock";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaselineEntry {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Baseline {
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub artifacts: BTreeMap<String, BaselineEntry>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BaselineDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl BaselineDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    /// Added and changed artifacts: everything whose current content was never approved.
    pub fn unapproved(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .added
            .iter()
            .chain(self.changed.iter())
            .cloned()
            .collect();
        v.sort();
        v
    }
}

/// SHA-256 over the file content with normalized line endings, hex encoded.
pub fn hash_file(path: &Path) -> Result<String> {
    let content =
        std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut normalized = Vec::with_capacity(content.len());
    let mut i = 0;
    while i < content.len() {
        if content[i] == b'\r' && content.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        normalized.push(content[i]);
        i += 1;
    }
    Ok(hex(&Sha256::digest(&normalized)))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn current(root: &Path, trace: &Trace) -> Result<BTreeMap<String, BaselineEntry>> {
    let mut out = BTreeMap::new();
    for (id, path) in trace.artifacts.iter().chain(trace.spec_files.iter()) {
        out.insert(
            id.clone(),
            BaselineEntry {
                file: rel(root, path),
                sha256: hash_file(path)?,
            },
        );
    }
    Ok(out)
}

pub fn load(doc_dir: &Path) -> Result<Option<Baseline>> {
    let path = doc_dir.join(BASELINE_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let b: Baseline = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(Some(b))
}

pub fn diff(baseline: &Baseline, now: &BTreeMap<String, BaselineEntry>) -> BaselineDiff {
    let mut d = BaselineDiff::default();
    for (id, entry) in now {
        match baseline.artifacts.get(id) {
            None => d.added.push(id.clone()),
            Some(old) if old.sha256 != entry.sha256 => d.changed.push(id.clone()),
            _ => {}
        }
    }
    for id in baseline.artifacts.keys() {
        if !now.contains_key(id) {
            d.removed.push(id.clone());
        }
    }
    d
}

/// Problems caused by unapproved changes: UNREVIEWED artifacts and their SUSPECT downstream links.
pub fn check(trace: &Trace, d: &BaselineDiff) -> Vec<String> {
    let mut problems = Vec::new();
    for id in &d.added {
        problems.push(format!(
            "UNREVIEWED {}: new since baseline (review, then `aegis baseline --approve`)",
            id
        ));
    }
    for id in &d.changed {
        problems.push(format!(
            "UNREVIEWED {}: changed since baseline (review, then `aegis baseline --approve`)",
            id
        ));
    }
    for id in &d.removed {
        problems.push(format!("UNREVIEWED {}: removed since baseline", id));
    }

    let down = traceability::downstream(trace);
    let mut seen = BTreeSet::new();
    for up in d.changed.iter().chain(d.removed.iter()) {
        for item in down.get(up).into_iter().flatten() {
            if seen.insert((item.clone(), up.clone())) {
                problems.push(format!(
                    "SUSPECT  {}: upstream {} changed since baseline",
                    item, up
                ));
            }
        }
        let code = if up.starts_with("REQ-") {
            trace.code_impl.get(up)
        } else if up.starts_with("TEST-") {
            trace.test_annot.get(up)
        } else {
            None
        };
        for loc in code.into_iter().flatten() {
            problems.push(format!(
                "SUSPECT  {}: annotates {}, which changed since baseline",
                loc, up
            ));
        }
        if let Some(file) = up.strip_prefix("SPEC:") {
            for (id, sources) in &trace.sources {
                if sources.iter().any(|s| {
                    s == file
                        || s.starts_with(&format!("{}#", file))
                        || (s.ends_with('/') && file.starts_with(s.as_str()))
                }) {
                    problems.push(format!(
                        "SUSPECT  {}: cites {}, which changed since baseline",
                        id, file
                    ));
                }
            }
        }
        for (_, page) in trace.doc_refs.get(up).into_iter().flatten() {
            problems.push(format!(
                "SUSPECT  {}: documents {}, which changed since baseline",
                page, up
            ));
        }
    }
    problems
}

pub fn write(doc_dir: &Path, baseline: &Baseline) -> Result<PathBuf> {
    let path = doc_dir.join(BASELINE_FILE);
    let mut out = String::from(
        "# Aegis approved baseline — generated by `aegis baseline --approve`. Do not edit by hand.\n\
         # Implementation agents must never write this file (see doc/prompts/06_27b_implementation.md).\n",
    );
    out.push_str(&serde_yaml::to_string(baseline)?);
    std::fs::write(&path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

/// Structural problems that forbid approving a baseline (GAPs are allowed: specs are baselined before code exists;
/// with spec/ present, unsourced artifacts and uncovered spec sections block the approval).
pub fn blocking_for_approval(problems: &[String]) -> Vec<String> {
    problems
        .iter()
        .filter(|p| {
            ["INVALID", "DUPLICATE", "ORPHAN", "UNSOURCED", "UNCOVERED"]
                .iter()
                .any(|k| p.starts_with(k))
        })
        .filter(|p| !p.contains("annotated in code") && !p.contains("annotated in test"))
        .cloned()
        .collect()
}
