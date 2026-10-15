use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Default, Clone)]
pub struct Trace {
    pub requirements: BTreeMap<String, PathBuf>,
    /// ARCH-ID -> requirements the architecture element satisfies
    pub arch_satisfies: BTreeMap<String, Vec<String>>,
    pub uc_implements: BTreeMap<String, Vec<String>>,
    /// TEST-ID -> verified IDs (REQ-xxx for requirement tests, ARCH-xxx for integration tests)
    pub test_verifies: BTreeMap<String, Vec<String>>,
    pub code_impl: BTreeMap<String, Vec<String>>,
    pub test_annot: BTreeMap<String, Vec<String>>,
    /// Every specification artifact ID -> defining file (input for baselines)
    pub artifacts: BTreeMap<String, PathBuf>,
    /// Structural defects found while reading docs or code. Never skipped silently.
    pub doc_errors: Vec<String>,
    /// UC-ID -> handbook that must document it: user | operator | none (default: user)
    pub uc_audience: BTreeMap<String, String>,
    /// Artifact ID -> handbook pages documenting it (`documents:` frontmatter), as (audience, file)
    pub doc_refs: BTreeMap<String, Vec<(String, String)>>,
    /// True once a documentation tree was scanned: only then is UC documentation coverage enforced
    pub documentation_scanned: bool,
    /// Artifact ID -> `source:` citations into spec/ (`spec/<file>.md#<anchor>`)
    pub sources: BTreeMap<String, Vec<String>>,
    /// Sections of the specification documents (key `spec/<file>.md#<anchor>`)
    pub spec_sections: BTreeMap<String, crate::spec::Section>,
    /// `SPEC:<path>` -> file, for baselines
    pub spec_files: BTreeMap<String, PathBuf>,
    /// True once spec/ was read: citations are then checked
    pub spec_scanned: bool,
}

/// Root of the delivered documentation (one MkDocs site per handbook); `doc/` holds the process artifacts.
pub const DOCS_DIR: &str = "docs";

/// Handbooks (first directory level under the documentation root, one MkDocs site each).
pub const HANDBOOKS: [&str; 4] = ["user", "operator", "technical", "hosting"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRow {
    pub req: String,
    #[serde(default)]
    pub architecture: Vec<String>,
    pub usecases: Vec<String>,
    pub tests: Vec<String>,
    pub code: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceReport {
    pub rows: Vec<TraceRow>,
    pub problems: Vec<String>,
}

fn id_regex() -> Regex {
    Regex::new(r"\b(REQ|UC|TEST|ARCH)-([A-Za-z0-9_-]+)\b").unwrap()
}

pub fn parse_ids(raw: &str) -> Vec<String> {
    let id_re = id_regex();
    let mut ids = Vec::new();
    for cap in id_re.captures_iter(raw) {
        if let (Some(prefix), Some(suffix)) = (cap.get(1), cap.get(2)) {
            ids.push(format!("{}-{}", prefix.as_str(), suffix.as_str()));
        }
    }
    ids
}

/// Files in artifact directories that are not artifacts themselves.
fn is_index_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("README.md") | Some("index.md")
    )
}

/// Extracts the metadata mapping of an artifact: YAML frontmatter (`--- ... ---`, optionally with a ```yaml fence
/// inside) takes precedence; otherwise the first ```yaml block carrying an `id`.
/// `Ok(None)` means the file has no metadata with an `id`; `Err` means metadata exists but is malformed.
pub fn extract_metadata(content: &str) -> std::result::Result<Option<serde_yaml::Mapping>, String> {
    let content = content.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let id_key = serde_yaml::Value::String("id".to_string());

    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let inner: String = rest[..end]
                .lines()
                .filter(|l| !l.trim_start().starts_with("```"))
                .collect::<Vec<_>>()
                .join("\n");
            let val: serde_yaml::Value = serde_yaml::from_str(&inner)
                .map_err(|e| format!("invalid YAML frontmatter: {}", e))?;
            if let Some(map) = val.as_mapping() {
                if map.contains_key(&id_key) {
                    return Ok(Some(map.clone()));
                }
            }
        } else {
            return Err("unterminated YAML frontmatter (missing closing `---`)".to_string());
        }
    }

    let yaml_block_re = Regex::new(r"(?s)```ya?ml\s*\n(.*?)```").unwrap();
    let mut first_error = None;
    for cap in yaml_block_re.captures_iter(&content) {
        let block = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        match serde_yaml::from_str::<serde_yaml::Value>(block) {
            Ok(val) => {
                if let Some(map) = val.as_mapping() {
                    if map.contains_key(&id_key) {
                        return Ok(Some(map.clone()));
                    }
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(format!("invalid ```yaml block: {}", e));
                }
            }
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

/// Reads an ID list field (`[A, B]`, a single string, or absent). Entries that are not valid IDs are reported.
fn id_list(
    map: &serde_yaml::Mapping,
    key: &str,
    errors: &mut Vec<String>,
    owner: &str,
) -> Vec<String> {
    let Some(raw) = map.get(serde_yaml::Value::String(key.to_string())) else {
        return Vec::new();
    };
    let items: Vec<String> = match raw {
        serde_yaml::Value::Null => Vec::new(),
        serde_yaml::Value::String(s) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .map(|v| match v {
                serde_yaml::Value::String(s) => s.trim().to_string(),
                other => format!("{:?}", other),
            })
            .collect(),
        other => {
            errors.push(format!(
                "INVALID  {}: field `{}` must be a list of IDs, got {:?}",
                owner, key, other
            ));
            return Vec::new();
        }
    };
    let full_id = Regex::new(r"^(REQ|UC|TEST|ARCH)-[A-Za-z0-9_-]+$").unwrap();
    let mut ids = Vec::new();
    for item in items {
        if full_id.is_match(&item) {
            ids.push(item);
        } else {
            errors.push(format!(
                "INVALID  {}: `{}` entry `{}` is not a valid artifact ID",
                owner, key, item
            ));
        }
    }
    ids
}

pub fn collect_docs(doc_dir: &Path, trace: &mut Trace) -> Result<()> {
    if !doc_dir.exists() {
        trace.doc_errors.push(format!(
            "INVALID  documentation directory {} does not exist",
            doc_dir.display()
        ));
        return Ok(());
    }

    // (subdir, prefix, strict): in strict dirs every Markdown file must be an artifact; architecture/ may also hold
    // free-form documents (overviews, diagrams) without metadata.
    let subdirs = [
        ("requirements", "REQ-", true),
        ("architecture", "ARCH-", false),
        ("usecases", "UC-", true),
        ("testplan", "TEST-", true),
    ];

    for (subdir, expected_prefix, strict) in subdirs {
        let base = doc_dir.join(subdir);
        if !base.exists() {
            continue;
        }

        let mut files: Vec<PathBuf> = WalkDir::new(&base)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| {
                p.is_file()
                    && p.extension().and_then(|s| s.to_str()) == Some("md")
                    && !is_index_file(p)
            })
            .collect();
        files.sort();

        for path in files {
            let shown = path.display().to_string();
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    trace
                        .doc_errors
                        .push(format!("INVALID  {}: unreadable ({})", shown, e));
                    continue;
                }
            };
            let map = match extract_metadata(&content) {
                Ok(Some(map)) => map,
                Ok(None) => {
                    if strict {
                        trace.doc_errors.push(format!(
                            "INVALID  {}: no metadata block with `id` (YAML frontmatter or ```yaml block)",
                            shown
                        ));
                    }
                    continue;
                }
                Err(e) => {
                    trace.doc_errors.push(format!("INVALID  {}: {}", shown, e));
                    continue;
                }
            };

            let id_val = map
                .get(serde_yaml::Value::String("id".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !id_val.starts_with(expected_prefix) {
                trace.doc_errors.push(format!(
                    "INVALID  {}: id `{}` does not match directory {}/ (expected {}xxx)",
                    shown, id_val, subdir, expected_prefix
                ));
                continue;
            }
            if let Some(prev) = trace.artifacts.get(&id_val) {
                trace.doc_errors.push(format!(
                    "DUPLICATE {}: defined in {} and {}",
                    id_val,
                    prev.display(),
                    shown
                ));
                continue;
            }
            trace.artifacts.insert(id_val.clone(), path.clone());
            match map.get(serde_yaml::Value::String("source".to_string())) {
                None | Some(serde_yaml::Value::Null) => {}
                Some(serde_yaml::Value::String(s)) => {
                    trace
                        .sources
                        .insert(id_val.clone(), vec![s.trim().to_string()]);
                }
                Some(serde_yaml::Value::Sequence(seq)) => {
                    let list: Vec<String> = seq
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .collect();
                    if list.len() != seq.len() {
                        trace.doc_errors.push(format!(
                            "INVALID  {}: `source` entries must be strings",
                            id_val
                        ));
                    }
                    trace.sources.insert(id_val.clone(), list);
                }
                Some(other) => trace.doc_errors.push(format!(
                    "INVALID  {}: `source` must be a string or a list, got {:?}",
                    id_val, other
                )),
            }

            match expected_prefix {
                "REQ-" => {
                    trace.requirements.insert(id_val, path);
                }
                "ARCH-" => {
                    let sat = id_list(&map, "satisfies", &mut trace.doc_errors, &id_val);
                    trace.arch_satisfies.insert(id_val, sat);
                }
                "UC-" => {
                    let impls = id_list(&map, "implements", &mut trace.doc_errors, &id_val);
                    let audience = map
                        .get(serde_yaml::Value::String("audience".to_string()))
                        .and_then(|v| v.as_str())
                        .unwrap_or("user")
                        .to_string();
                    if !matches!(audience.as_str(), "user" | "operator" | "none") {
                        trace.doc_errors.push(format!(
                            "INVALID  {}: audience `{}` (allowed: user, operator, none)",
                            id_val, audience
                        ));
                    }
                    trace.uc_audience.insert(id_val.clone(), audience);
                    trace.uc_implements.insert(id_val, impls);
                }
                _ => {
                    let verifs = id_list(&map, "verifies", &mut trace.doc_errors, &id_val);
                    trace.test_verifies.insert(id_val, verifs);
                }
            }
        }
    }
    Ok(())
}

/// Scans the handbooks under `docs_root` (`<root>/<handbook>/**/*.md`, build output skipped) for `documents:`
/// frontmatter. Pages without frontmatter are fine; malformed frontmatter and unknown handbooks are errors.
pub fn collect_documentation(docs_root: &Path, trace: &mut Trace) -> Result<()> {
    trace.documentation_scanned = true;
    let mut files: Vec<PathBuf> = WalkDir::new(docs_root)
        .into_iter()
        .filter_entry(|e| {
            !is_skipped_dir(e.path())
                && !(e.path().is_dir() && e.file_name().to_str() == Some("site"))
        })
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    files.sort();
    for path in files {
        let shown = path.display().to_string();
        let rel = path.strip_prefix(docs_root).unwrap_or(&path);
        let Some(handbook) = rel
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .map(String::from)
        else {
            continue;
        };
        if rel.components().count() < 2 {
            continue; // files directly under the docs root (e.g. a README) belong to no handbook
        }
        if !HANDBOOKS.contains(&handbook.as_str()) {
            trace.doc_errors.push(format!(
                "INVALID  {}: unknown handbook `{}` (allowed: {})",
                shown,
                handbook,
                HANDBOOKS.join(", ")
            ));
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c.replace("\r\n", "\n"),
            Err(e) => {
                trace
                    .doc_errors
                    .push(format!("INVALID  {}: unreadable ({})", shown, e));
                continue;
            }
        };
        let Some(rest) = content.strip_prefix("---\n") else {
            continue;
        };
        let Some(end) = rest.find("\n---") else {
            trace
                .doc_errors
                .push(format!("INVALID  {}: unterminated YAML frontmatter", shown));
            continue;
        };
        let map = match serde_yaml::from_str::<serde_yaml::Value>(&rest[..end]) {
            Ok(serde_yaml::Value::Mapping(m)) => m,
            Ok(_) => continue,
            Err(e) => {
                trace.doc_errors.push(format!(
                    "INVALID  {}: invalid YAML frontmatter: {}",
                    shown, e
                ));
                continue;
            }
        };
        for id in id_list(&map, "documents", &mut trace.doc_errors, &shown) {
            trace
                .doc_refs
                .entry(id)
                .or_default()
                .push((handbook.clone(), shown.clone()));
        }
    }
    Ok(())
}

/// Reads everything a trace needs: doc/ artifacts, optionally source annotations, and — when present — the
/// handbooks under docs/ and the specification under spec/.
pub fn collect_project(root: &Path, doc_dir: &Path, src_dirs: Option<&[PathBuf]>) -> Result<Trace> {
    let mut trace = Trace::default();
    collect_docs(doc_dir, &mut trace)?;
    if let Some(dirs) = src_dirs {
        collect_code(dirs, &mut trace)?;
    }
    let docs_root = root.join(DOCS_DIR);
    if docs_root.exists() {
        collect_documentation(&docs_root, &mut trace)?;
    }
    let spec_root = root.join(crate::spec::SPEC_DIR);
    if spec_root.exists() {
        crate::spec::collect_spec(root, &spec_root, &mut trace)?;
    }
    Ok(trace)
}

pub const SOURCE_EXTENSIONS: [&str; 10] =
    ["py", "js", "ts", "go", "rs", "java", "c", "cpp", "h", "hpp"];

/// Build output, dependencies and VCS/tool folders are never scanned (they are not the project's source).
pub fn is_skipped_dir(path: &Path) -> bool {
    path.is_dir()
        && matches!(
            path.file_name().and_then(|s| s.to_str()),
            Some(
                "target"
                    | "node_modules"
                    | ".git"
                    | ".hg"
                    | ".svn"
                    | "build"
                    | "dist"
                    | "vendor"
                    | "mutants.out"
            )
        )
}

/// All source files under the given directories, sorted (build output and VCS folders skipped).
pub fn source_files(src_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for src_dir in src_dirs {
        for entry in WalkDir::new(src_dir)
            .into_iter()
            .filter_entry(|e| !is_skipped_dir(e.path()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|e| SOURCE_EXTENSIONS.contains(&e))
                    .unwrap_or(false)
            {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    files
}

pub fn collect_code(src_dirs: &[PathBuf], trace: &mut Trace) -> Result<()> {
    let existing: Vec<PathBuf> = src_dirs.iter().filter(|d| d.exists()).cloned().collect();
    if existing.is_empty() {
        let looked: Vec<String> = src_dirs.iter().map(|d| d.display().to_string()).collect();
        trace.doc_errors.push(format!(
            "INVALID  no source directory found (looked in: {})",
            looked.join(", ")
        ));
        return Ok(());
    }

    let implements_re = Regex::new(r"@implements\s+(REQ-[A-Za-z0-9_-]+)").unwrap();
    let verifies_re = Regex::new(r"@verifies\s+(TEST-[A-Za-z0-9_-]+)").unwrap();

    for path in source_files(&existing) {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                trace.doc_errors.push(format!(
                    "INVALID  {}: unreadable source file ({})",
                    path.display(),
                    e
                ));
                continue;
            }
        };
        for (lineno, line) in content.lines().enumerate() {
            let line_idx = lineno + 1;
            for cap in implements_re.captures_iter(line) {
                if let Some(req) = cap.get(1) {
                    trace
                        .code_impl
                        .entry(req.as_str().to_string())
                        .or_default()
                        .push(format!("{}:{}", path.display(), line_idx));
                }
            }
            for cap in verifies_re.captures_iter(line) {
                if let Some(t) = cap.get(1) {
                    trace
                        .test_annot
                        .entry(t.as_str().to_string())
                        .or_default()
                        .push(format!("{}:{}", path.display(), line_idx));
                }
            }
        }
    }
    Ok(())
}

fn invert(map: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, vs) in map {
        for v in vs {
            out.entry(v.clone()).or_default().push(k.clone());
        }
    }
    for list in out.values_mut() {
        list.sort();
        list.dedup();
    }
    out
}

/// Downstream index: upstream ID (REQ/ARCH) -> ARCH, UC and TEST IDs that reference it.
pub fn downstream(trace: &Trace) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for inverted in [
        invert(&trace.arch_satisfies),
        invert(&trace.uc_implements),
        invert(&trace.test_verifies),
    ] {
        for (k, vs) in inverted {
            out.entry(k).or_default().extend(vs);
        }
    }
    for list in out.values_mut() {
        list.sort();
        list.dedup();
    }
    out
}

pub type TraceRowsAndMaps = (
    Vec<TraceRow>,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, Vec<String>>,
);

pub fn build_rows(trace: &Trace) -> TraceRowsAndMaps {
    let req_to_uc = invert(&trace.uc_implements);
    let req_to_test = invert(&trace.test_verifies);
    let req_to_arch = invert(&trace.arch_satisfies);

    let mut rows = Vec::new();
    for req in trace.requirements.keys() {
        rows.push(TraceRow {
            req: req.clone(),
            architecture: req_to_arch.get(req).cloned().unwrap_or_default(),
            usecases: req_to_uc.get(req).cloned().unwrap_or_default(),
            tests: req_to_test.get(req).cloned().unwrap_or_default(),
            code: trace.code_impl.get(req).cloned().unwrap_or_default(),
        });
    }
    (rows, req_to_uc, req_to_test)
}

pub fn find_problems(trace: &Trace, rows: &[TraceRow]) -> Vec<String> {
    let mut problems = trace.doc_errors.clone();
    let known_reqs: BTreeSet<&String> = trace.requirements.keys().collect();
    let known_arch: BTreeSet<&String> = trace.arch_satisfies.keys().collect();
    let known_tests: BTreeSet<&String> = trace.test_verifies.keys().collect();
    let arch_to_test = invert(&trace.test_verifies);

    if trace.requirements.is_empty() {
        problems.push(
            "EMPTY    no requirements defined under doc/requirements/ — nothing is traced"
                .to_string(),
        );
    }

    // 1. Gaps
    for row in rows {
        if row.architecture.is_empty() {
            problems.push(format!(
                "GAP      {}: not allocated to any architecture element (ARCH-xxx `satisfies`)",
                row.req
            ));
        }
        if row.usecases.is_empty() {
            problems.push(format!("GAP      {}: no use case defined", row.req));
        }
        if row.tests.is_empty() {
            problems.push(format!("GAP      {}: no test in test plan", row.req));
        }
        if row.code.is_empty() {
            problems.push(format!(
                "GAP      {}: no @implements annotation in code",
                row.req
            ));
        }
    }
    for (arch, reqs) in &trace.arch_satisfies {
        if reqs.is_empty() {
            problems.push(format!("GAP      {}: satisfies no requirement", arch));
        }
        if !arch_to_test.contains_key(arch) {
            problems.push(format!(
                "GAP      {}: no integration test verifies this architecture element",
                arch
            ));
        }
    }
    for (uc, reqs) in &trace.uc_implements {
        if reqs.is_empty() {
            problems.push(format!("GAP      {}: implements no requirement", uc));
        }
    }
    for (t, ids) in &trace.test_verifies {
        if ids.is_empty() {
            problems.push(format!("GAP      {}: verifies nothing", t));
        }
        if !trace.test_annot.contains_key(t) {
            problems.push(format!(
                "GAP      {}: specified in test plan but not implemented (no @verifies annotation)",
                t
            ));
        }
    }

    // 2. Orphaned code annotations: @implements points to unknown REQ
    for (req, locs) in &trace.code_impl {
        if !known_reqs.contains(req) {
            let loc = locs.first().map(|s| s.as_str()).unwrap_or("unknown");
            problems.push(format!(
                "ORPHAN   {}: annotated in code ({}), but no requirement defined",
                req, loc
            ));
        }
    }

    // 3. Orphaned upstream references in ARCH / UC / TEST metadata
    let check_refs =
        |problems: &mut Vec<String>, owner: &str, refs: &[String], allowed: &[&str]| {
            for r in refs {
                let exists = (r.starts_with("REQ-") && known_reqs.contains(r))
                    || (r.starts_with("ARCH-") && known_arch.contains(r));
                let prefix_ok = allowed.iter().any(|p| r.starts_with(p));
                if !prefix_ok {
                    problems.push(format!(
                        "INVALID  {}: may not reference {} (allowed: {})",
                        owner,
                        r,
                        allowed.join(", ")
                    ));
                } else if !exists {
                    problems.push(format!(
                        "ORPHAN   {}: references {}, which does not exist",
                        owner, r
                    ));
                }
            }
        };
    for (arch, reqs) in &trace.arch_satisfies {
        check_refs(&mut problems, arch, reqs, &["REQ-"]);
    }
    for (uc, reqs) in &trace.uc_implements {
        check_refs(&mut problems, uc, reqs, &["REQ-"]);
    }
    for (t, ids) in &trace.test_verifies {
        check_refs(&mut problems, t, ids, &["REQ-", "ARCH-"]);
    }

    // 4. Documentation coverage: every UC in the handbook of its audience; `documents:` only to known artifacts
    if trace.documentation_scanned {
        for (uc, audience) in &trace.uc_audience {
            if audience == "none" {
                continue;
            }
            let documented = trace
                .doc_refs
                .get(uc)
                .map(|pages| pages.iter().any(|(h, _)| h == audience))
                .unwrap_or(false);
            if !documented {
                problems.push(format!(
                    "GAP      {}: not documented in the {} handbook (page with `documents: [{}]`)",
                    uc, audience, uc
                ));
            }
        }
        for (id, pages) in &trace.doc_refs {
            if !trace.artifacts.contains_key(id) {
                let page = pages.first().map(|(_, f)| f.as_str()).unwrap_or("unknown");
                problems.push(format!(
                    "ORPHAN   {}: documented in {}, but not defined",
                    id, page
                ));
            }
        }
    }

    // 5. Citations into spec/ resolve; REQ/ARCH/UC are sourced
    if trace.spec_scanned {
        problems.extend(crate::spec::citation_problems(trace));
    }

    // 6. Orphaned @verifies in code
    for (t, locs) in &trace.test_annot {
        if !known_tests.contains(t) {
            let loc = locs.first().map(|s| s.as_str()).unwrap_or("unknown");
            problems.push(format!(
                "ORPHAN   {}: annotated in test ({}), but not in test plan",
                t, loc
            ));
        }
    }

    problems
}

pub fn render_matrix_markdown(rows: &[TraceRow]) -> String {
    let mut lines = vec![
        "| Requirement | Architecture | Usecases | Tests | Code |".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];
    let list = |v: &Vec<String>| {
        if v.is_empty() {
            "— MISSING".to_string()
        } else {
            v.join(", ")
        }
    };
    for row in rows {
        let code = if row.code.is_empty() {
            "— MISSING".to_string()
        } else {
            format!("{} location(s)", row.code.len())
        };
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            row.req,
            list(&row.architecture),
            list(&row.usecases),
            list(&row.tests),
            code
        ));
    }
    lines.join("\n")
}

pub fn render_report_markdown(rows: &[TraceRow], problems: &[String]) -> String {
    let mut output = String::new();
    output.push_str("# Traceability Matrix\n\n");
    output.push_str(&render_matrix_markdown(rows));
    output.push_str("\n\n");

    if problems.is_empty() {
        output.push_str("## No issues found — traceability chain complete.\n");
    } else {
        output.push_str(&format!("## {} Issue(s) found\n\n", problems.len()));
        for p in problems {
            output.push_str(&format!("- {}\n", p));
        }
    }
    output
}
