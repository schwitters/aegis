//! Specification coverage: the source documents in `spec/` are traced into the Aegis artifacts.
//!
//! Every REQ, ARCH and UC cites its origin (`source: [spec/<file>.md#<heading-anchor>]`). This module parses the
//! headings of all specification documents (GitHub-style anchors), checks that every citation resolves, and that
//! every section is cited by at least one artifact — directly or through one of its sub-sections — or listed as a
//! justified exception in `doc/spec-coverage.yaml`. Nothing about coverage is left to a model's judgement.

use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use walkdir::WalkDir;

use crate::traceability::Trace;

/// Root of the specification documents.
pub const SPEC_DIR: &str = "spec";
/// Exceptions for sections that intentionally map to no artifact.
pub const EXCEPTIONS_FILE: &str = "spec-coverage.yaml";
/// Generated coverage report.
pub const REPORT_FILE: &str = "spec-coverage.md";
/// Heading levels that must be covered (deeper headings are citable but not required).
pub const DEFAULT_DEPTH: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// `spec/<file>.md#<anchor>` (or `spec/<file>.md` for a document without headings)
    pub key: String,
    pub file: String,
    pub level: usize,
    pub title: String,
    /// Key of the enclosing section, if any
    pub parent: Option<String>,
    /// Position in reading order (files sorted by path, headings in document order)
    pub order: usize,
}

#[derive(Debug, Default, Deserialize)]
pub struct Exceptions {
    #[serde(default)]
    pub exceptions: Vec<Exception>,
}

#[derive(Debug, Deserialize)]
pub struct Exception {
    pub section: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub issue: Option<String>,
}

/// GitHub-style heading anchor: lowercase, markdown stripped, punctuation removed, spaces to hyphens.
pub fn slug(title: &str) -> String {
    let link_re = Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap();
    let text = link_re.replace_all(title, "$1");
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// Sections of one Markdown document (ATX headings outside code fences), with GitHub's duplicate suffixes.
pub fn parse_sections(file: &str, content: &str) -> Vec<Section> {
    let heading_re = Regex::new(r"^(#{1,6})\s+(.*?)\s*#*\s*$").unwrap();
    let mut sections = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut fence: Option<String> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(f) = &fence {
            if trimmed.starts_with(f.as_str()) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(trimmed[..3].to_string());
            continue;
        }
        let Some(cap) = heading_re.captures(line) else { continue };
        let level = cap[1].len();
        let title = cap[2].replace(['*', '`'], "");
        let base = slug(&title);
        let n = seen.entry(base.clone()).or_insert(0);
        let anchor = if *n == 0 { base.clone() } else { format!("{}-{}", base, n) };
        *n += 1;
        let key = format!("{}#{}", file, anchor);
        while stack.last().map(|(l, _)| *l >= level).unwrap_or(false) {
            stack.pop();
        }
        let parent = stack.last().map(|(_, k)| k.clone());
        stack.push((level, key.clone()));
        let order = sections.len();
        sections.push(Section { key, file: file.to_string(), level, title: title.trim().to_string(), parent, order });
    }
    if sections.is_empty() {
        sections.push(Section { key: file.to_string(), file: file.to_string(), level: 1, title: file.to_string(), parent: None, order: 0 });
    }
    sections
}

/// Reads `spec_root` into the trace (`spec_sections`, `spec_files`): Markdown documents by heading, every other file
/// (OpenAPI, JSON Schema, diagrams, …) as one section keyed by its path, so nothing in spec/ escapes coverage.
pub fn collect_spec(root: &Path, spec_root: &Path, trace: &mut Trace) -> Result<()> {
    trace.spec_scanned = true;
    let mut files: Vec<_> = WalkDir::new(spec_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && !p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(false))
        .collect();
    files.sort();
    if files.is_empty() {
        trace.doc_errors.push(format!("EMPTY    no specification documents under {}", spec_root.display()));
    }
    for path in files {
        let rel = path.strip_prefix(root).unwrap_or(&path).display().to_string();
        trace.spec_files.insert(format!("SPEC:{}", rel), path.clone());
        let base = trace.spec_sections.len();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            let content = std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
            for mut s in parse_sections(&rel, &content) {
                s.order += base;
                trace.spec_sections.insert(s.key.clone(), s);
            }
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(&rel).to_string();
            trace.spec_sections.insert(
                rel.clone(),
                Section { key: rel.clone(), file: rel, level: 1, title: name, parent: None, order: base },
            );
        }
    }
    Ok(())
}

/// Citations that do not resolve, and REQ/ARCH/UC without any citation. Part of `find_problems` once spec/ is read.
pub fn citation_problems(trace: &Trace) -> Vec<String> {
    let mut problems = Vec::new();
    let files: BTreeSet<String> = trace.spec_files.keys().map(|k| k.trim_start_matches("SPEC:").to_string()).collect();
    for (id, sources) in &trace.sources {
        for s in sources {
            let resolves = trace.spec_sections.contains_key(s) || (!s.contains('#') && files.contains(s));
            if !resolves {
                problems.push(format!("INVALID  {}: source `{}` does not resolve to a spec file/heading", id, s));
            }
        }
    }
    for id in trace.artifacts.keys() {
        let needs_source = id.starts_with("REQ-") || id.starts_with("ARCH-") || id.starts_with("UC-");
        if needs_source && trace.sources.get(id).map(|s| s.is_empty()).unwrap_or(true) {
            problems.push(format!("UNSOURCED {}: no `source:` citation into {}/", id, SPEC_DIR));
        }
    }
    problems
}

/// Section key -> artifact IDs citing it directly.
pub fn citations(trace: &Trace) -> BTreeMap<String, BTreeSet<String>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (id, sources) in &trace.sources {
        for s in sources {
            map.entry(s.clone()).or_default().insert(id.clone());
        }
    }
    map
}

pub fn load_exceptions(doc_dir: &Path) -> Result<Exceptions> {
    let path = doc_dir.join(EXCEPTIONS_FILE);
    if !path.exists() {
        return Ok(Exceptions::default());
    }
    let content = std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    serde_yaml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

/// Coverage: every section up to `depth` is cited (itself or a descendant) or a valid exception.
pub fn coverage_problems(trace: &Trace, exceptions: &Exceptions, issue_ids: &BTreeSet<String>, depth: usize) -> Vec<String> {
    let mut problems = Vec::new();
    let cited = citations(trace);
    let mut covered: BTreeSet<String> = BTreeSet::new();
    for key in cited.keys() {
        let mut cur = Some(key.clone());
        while let Some(k) = cur {
            cur = trace.spec_sections.get(&k).and_then(|s| s.parent.clone());
            covered.insert(k);
        }
    }
    let mut excepted: BTreeSet<String> = BTreeSet::new();
    for e in &exceptions.exceptions {
        if !trace.spec_sections.contains_key(&e.section) {
            problems.push(format!("INVALID  {}: exception for unknown section `{}`", EXCEPTIONS_FILE, e.section));
            continue;
        }
        match (&e.reason, &e.issue) {
            (_, Some(i)) if !issue_ids.contains(i) => {
                problems.push(format!("ORPHAN   {}: exception `{}` references {}, which does not exist", EXCEPTIONS_FILE, e.section, i))
            }
            (None, None) => problems.push(format!("INVALID  {}: exception `{}` needs a reason or an issue", EXCEPTIONS_FILE, e.section)),
            (Some(r), None) if r.trim().is_empty() => {
                problems.push(format!("INVALID  {}: exception `{}` has an empty reason", EXCEPTIONS_FILE, e.section))
            }
            _ => {}
        }
        // an exception covers the section and all its sub-sections
        for s in trace.spec_sections.values() {
            let mut cur = Some(s.key.clone());
            while let Some(k) = cur {
                if k == e.section {
                    excepted.insert(s.key.clone());
                    break;
                }
                cur = trace.spec_sections.get(&k).and_then(|x| x.parent.clone());
            }
        }
    }
    for s in trace.spec_sections.values() {
        if s.level <= depth && !covered.contains(&s.key) && !excepted.contains(&s.key) {
            problems.push(format!("UNCOVERED {}: \"{}\" is cited by no artifact and not excepted", s.key, s.title));
        }
    }
    problems
}

/// Generated coverage report (doc/spec-coverage.md).
pub fn render_report(trace: &Trace, exceptions: &Exceptions) -> String {
    let cited = citations(trace);
    let exc: BTreeMap<&str, &Exception> = exceptions.exceptions.iter().map(|e| (e.section.as_str(), e)).collect();
    let mut out = String::from(
        "# Specification Coverage\n\n> Generated by `aegis spec check` — do not edit. Exceptions: doc/spec-coverage.yaml.\n\n\
         | Section | Title | Artifacts |\n|---|---|---|\n",
    );
    let mut ordered: Vec<&Section> = trace.spec_sections.values().collect();
    ordered.sort_by_key(|s| s.order);
    for s in ordered {
        let indent = "&nbsp;&nbsp;".repeat(s.level.saturating_sub(1));
        let cell = match (cited.get(&s.key), exc.get(s.key.as_str())) {
            (Some(ids), _) => ids.iter().cloned().collect::<Vec<_>>().join(", "),
            (None, Some(e)) => match (&e.issue, &e.reason) {
                (Some(i), _) => format!("open ({})", i),
                (None, Some(r)) => format!("excepted: {}", r),
                _ => "excepted".to_string(),
            },
            (None, None) => "—".to_string(),
        };
        out.push_str(&format!("| `{}` | {}{} | {} |\n", s.key, indent, s.title, cell));
    }
    out
}
