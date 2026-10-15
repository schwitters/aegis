//! Path-scoped forbidden source patterns (`forbidden_patterns` in rulesets) and path globs.
//!
//! Complements clippy where a rule depends on *where* code lives: e.g. no wall clock in the domain crate, no
//! environment access outside the composition root, no secrets in tracing calls. A finding can only be lifted by an
//! `AEGIS-DEVIATION(<rule>)` marker on the line above, if the owning rule permits deviations and the path is not in a
//! deviation-free zone.

use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::deviation::DeviationRule;
use crate::rulesets::ForbiddenPattern;

/// Glob (`*` within a path segment, `**` across segments, `?` one char) to an anchored regex.
pub fn glob_regex(glob: &str) -> Regex {
    let mut re = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    re.push_str("(?:.*/)?");
                    i += 3;
                } else {
                    re.push_str(".*");
                    i += 2;
                }
                continue;
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    re.push('$');
    Regex::new(&re).unwrap_or_else(|_| Regex::new("^$").unwrap())
}

pub fn matches_any(rel: &str, globs: &[Regex]) -> bool {
    globs.iter().any(|g| g.is_match(rel))
}

/// Findings `PATTERN <file>:<line>: <message>` for all source files under `dirs`.
pub fn scan(
    patterns: &[ForbiddenPattern],
    dirs: &[PathBuf],
    root: &Path,
    rules: &BTreeMap<String, DeviationRule>,
    no_deviation_zones: &[Regex],
) -> Vec<String> {
    let compiled: Vec<(&ForbiddenPattern, Vec<Regex>, Option<Regex>)> = patterns
        .iter()
        .map(|p| (p, p.paths.iter().map(|g| glob_regex(g)).collect(), Regex::new(&p.pattern).ok()))
        .collect();
    let mut findings = Vec::new();
    for (p, _, re) in &compiled {
        if re.is_none() {
            findings.push(format!("INVALID  forbidden pattern of {}: `{}` is not a valid regex", p.rule, p.pattern));
        }
    }
    for file in crate::traceability::source_files(dirs) {
        let rel = file.strip_prefix(root).unwrap_or(&file).display().to_string();
        let applicable: Vec<_> = compiled.iter().filter(|(_, g, re)| re.is_some() && matches_any(&rel, g)).collect();
        if applicable.is_empty() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            findings.push(format!("PATTERN  {}: unreadable", rel));
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") || t.starts_with('#') && !t.starts_with("#[") {
                continue;
            }
            for (p, _, re) in &applicable {
                if !re.as_ref().map(|r| r.is_match(line)).unwrap_or(false) {
                    continue;
                }
                let marker = format!("AEGIS-DEVIATION({})", p.rule);
                let covered = i > 0 && lines[i - 1].contains(&marker);
                let permitted = rules.get(&p.rule) == Some(&DeviationRule::Permitted) && !matches_any(&rel, no_deviation_zones);
                if !(covered && permitted) {
                    findings.push(format!("PATTERN  {}:{}: [{}] {}", rel, i + 1, p.rule, p.message));
                }
            }
        }
    }
    findings
}
