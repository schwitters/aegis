//! Deviation scan: every lint suppression must sit directly below an `AEGIS-DEVIATION(<RULE-ID>): <justification>`
//! marker that names a rule whose `deviation` field is `permitted` (see `deviation_policy` in the rulesets).

use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::rulesets::RulesetFile;

/// What a rule allows, by rule ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviationRule {
    Permitted,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub location: String,
    pub message: String,
}

/// Suppression syntaxes per language: a line containing one of these must be covered by a marker.
pub fn suppression_tokens(language: &str) -> &'static [&'static str] {
    match language {
        "rust" => &["#[allow(", "#![allow(", "#[expect(", "#![expect("],
        "c" | "c11" | "cpp" | "cpp20" | "c++" => &["NOLINT", "cppcheck-suppress", "AEGIS_IGNORE_RESULT("],
        "java" => &["@SuppressWarnings", "CHECKSTYLE:OFF", "NOPMD"],
        "python" => &["# noqa", "# type: ignore", "# nosec"],
        _ => &[],
    }
}

fn source_extensions(language: &str) -> &'static [&'static str] {
    match language {
        "rust" => &["rs"],
        "c" | "c11" => &["c", "h"],
        "cpp" | "cpp20" | "c++" => &["cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"],
        "java" => &["java"],
        "python" => &["py"],
        _ => &[],
    }
}

/// Rule IDs and their deviation field, over all loaded rulesets. Rules without the field count as forbidden.
pub fn rule_table(rulesets: &[RulesetFile]) -> BTreeMap<String, DeviationRule> {
    let mut table = BTreeMap::new();
    for rf in rulesets {
        for rule in &rf.rules {
            let kind = if rule.deviation.as_deref() == Some("permitted") {
                DeviationRule::Permitted
            } else {
                DeviationRule::Forbidden
            };
            table.insert(rule.id.clone(), kind);
            if let Some(legacy) = &rule.legacy_id {
                table.insert(legacy.clone(), kind);
            }
        }
    }
    table
}

fn marker_regex() -> Regex {
    Regex::new(r"AEGIS-DEVIATION\(\s*(RULE-[A-Za-z0-9_-]+)\s*\)\s*:?\s*(.*)").unwrap()
}

/// Strips trailing comment closers so `/* AEGIS-DEVIATION(X): why */` has the justification `why`.
fn justification(raw: &str) -> &str {
    raw.trim().trim_end_matches("*/").trim()
}

/// Checks one file's text. `location` prefixes findings (usually the path).
pub fn scan_text(text: &str, location: &str, language: &str, rules: &BTreeMap<String, DeviationRule>) -> Vec<Finding> {
    let tokens = suppression_tokens(language);
    let marker_re = marker_regex();
    let lines: Vec<&str> = text.lines().collect();
    let mut findings = Vec::new();

    // 1. Every marker names a known rule that permits deviations, and says why.
    for (idx, line) in lines.iter().enumerate() {
        if let Some(cap) = marker_re.captures(line) {
            let id = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let why = justification(cap.get(2).map(|m| m.as_str()).unwrap_or(""));
            let at = format!("{}:{}", location, idx + 1);
            match rules.get(id) {
                None => findings.push(Finding { location: at.clone(), message: format!("marker references unknown rule {}", id) }),
                Some(DeviationRule::Forbidden) => findings.push(Finding {
                    location: at.clone(),
                    message: format!("rule {} does not permit deviations", id),
                }),
                Some(DeviationRule::Permitted) => {}
            }
            if why.is_empty() || why.starts_with('<') {
                findings.push(Finding { location: at, message: format!("marker for {} has no justification", id) });
            }
        }
    }

    // 2. Every suppression is covered: walking upwards over blank lines and further attributes / suppressions, the
    //    first other line must carry a marker.
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if language == "rust" && trimmed.starts_with("//") {
            continue; // comments and doc text mentioning #[allow(..)] are not suppressions
        }
        if !tokens.iter().any(|t| line.contains(t)) || marker_re.is_match(line) {
            continue;
        }
        let mut covered = false;
        let mut up = idx;
        while up > 0 {
            up -= 1;
            let above = lines[up].trim();
            if above.is_empty() {
                continue;
            }
            if marker_re.is_match(above) {
                covered = true;
                break;
            }
            let is_attribute = above.starts_with("#[") || above.starts_with("#![") || above.starts_with('@');
            if is_attribute || tokens.iter().any(|t| above.contains(t)) {
                continue;
            }
            break;
        }
        if !covered {
            findings.push(Finding {
                location: format!("{}:{}", location, idx + 1),
                message: format!("suppression without AEGIS-DEVIATION marker: {}", line.trim()),
            });
        }
    }
    findings
}

/// Scans every source file of `language` under `dirs` (skipping build output and VCS folders).
pub fn scan_dirs(dirs: &[PathBuf], root: &Path, language: &str, rules: &BTreeMap<String, DeviationRule>) -> Vec<Finding> {
    let exts = source_extensions(language);
    let mut findings = Vec::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        let walker = WalkDir::new(dir).into_iter().filter_entry(|e| !crate::traceability::is_skipped_dir(e.path()));
        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !path.is_file() || !exts.contains(&ext) {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(path).display().to_string();
            match std::fs::read_to_string(path) {
                Ok(text) => findings.extend(scan_text(&text, &rel, language, rules)),
                // Fail closed: a file that cannot be read cannot be shown to be free of suppressions.
                Err(e) => findings.push(Finding { location: rel, message: format!("unreadable source file: {}", e) }),
            }
        }
    }
    findings
}
