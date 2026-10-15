//! Migration safety: destructive SQL in migrations needs an explicit, justified deviation marker.
//!
//! Checked statements (case-insensitive, comments stripped): DROP TABLE/COLUMN/INDEX/VIEW/TRIGGER/SCHEMA, ALTER TABLE
//! … DROP / RENAME, TRUNCATE, DELETE FROM without WHERE, UPDATE … SET without WHERE. A statement passes only when the
//! comment lines directly above it contain `-- AEGIS-DEVIATION(<rule>): <justification>`.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;
use walkdir::WalkDir;

pub const DEFAULT_RULE: &str = "RULE-RWE-015";

/// Why a statement is destructive, or `None`.
pub fn destructive(statement: &str) -> Option<&'static str> {
    let s = statement
        .to_uppercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let has = |re: &str| Regex::new(re).unwrap().is_match(&s);
    if has(r"\bDROP (TABLE|COLUMN|INDEX|VIEW|TRIGGER|SCHEMA|DATABASE)\b") {
        Some("drops a schema object")
    } else if has(r"\bALTER TABLE\b.*\bDROP\b") {
        Some("drops a column or constraint")
    } else if has(r"\bALTER TABLE\b.*\bRENAME\b") {
        Some("renames a table or column (breaks running code during rollout)")
    } else if has(r"\bTRUNCATE\b") {
        Some("truncates a table")
    } else if has(r"\bDELETE FROM\b") && !has(r"\bWHERE\b") {
        Some("deletes all rows")
    } else if has(r"\bUPDATE\b.*\bSET\b") && !has(r"\bWHERE\b") {
        Some("updates all rows")
    } else {
        None
    }
}

/// Findings for one migration file (`location` prefixes them).
pub fn check_text(text: &str, location: &str, rule: &str) -> Vec<String> {
    let marker = Regex::new(&format!(
        r"AEGIS-DEVIATION\(\s*{}\s*\)\s*:\s*\S",
        regex::escape(rule)
    ))
    .unwrap();
    let mut findings = Vec::new();
    let mut comments: Vec<&str> = Vec::new();
    let mut stmt = String::new();
    let mut start_line = 0;
    let mut stmt_comments: Vec<&str> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if stmt.trim().is_empty() {
            if trimmed.starts_with("--") {
                comments.push(trimmed);
                continue;
            }
            if trimmed.is_empty() {
                comments.clear();
                continue;
            }
            start_line = i + 1;
            stmt_comments = std::mem::take(&mut comments);
        }
        let code = match line.find("--") {
            Some(p) => &line[..p],
            None => line,
        };
        stmt.push_str(code);
        stmt.push('\n');
        if code.trim_end().ends_with(';') {
            if let Some(why) = destructive(&stmt) {
                if !stmt_comments.iter().any(|c| marker.is_match(c)) {
                    let first = stmt.split_whitespace().collect::<Vec<_>>().join(" ");
                    let short: String = first.chars().take(80).collect();
                    findings.push(format!(
                        "MIGRATION {}:{}: {} without `-- AEGIS-DEVIATION({}): <justification>` above it: {}",
                        location, start_line, why, rule, short
                    ));
                }
            }
            stmt.clear();
            stmt_comments.clear();
        }
    }
    findings
}

/// Checks every `*.sql` below `dir` (sorted). A missing directory has nothing to check.
pub fn check_dir(root: &Path, dir: &Path, rule: &str) -> Result<(usize, Vec<String>)> {
    if !dir.exists() {
        return Ok((0, Vec::new()));
    }
    let mut files: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("sql"))
        .collect();
    files.sort();
    let mut findings = Vec::new();
    for f in &files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".up.sql") {
            let down_name = name.replacen(".up.sql", ".down.sql", 1);
            let down_path = f.with_file_name(&down_name);
            if !down_path.exists() {
                let rel = f.strip_prefix(root).unwrap_or(f).display().to_string();
                findings.push(format!(
                    "MIGRATION {}: missing reversible down migration (expected companion {})",
                    rel, down_name
                ));
            }
        }
        let text = std::fs::read_to_string(f)
            .with_context(|| format!("Failed to read {}", f.display()))?;
        let rel = f.strip_prefix(root).unwrap_or(f).display().to_string();
        findings.extend(check_text(&text, &rel, rule));
    }
    Ok((files.len(), findings))
}
