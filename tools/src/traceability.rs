use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Default, Clone)]
pub struct Trace {
    pub requirements: BTreeMap<String, PathBuf>,
    pub uc_implements: BTreeMap<String, Vec<String>>,
    pub test_verifies: BTreeMap<String, Vec<String>>,
    pub code_impl: BTreeMap<String, Vec<String>>,
    pub test_annot: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRow {
    pub req: String,
    pub usecases: Vec<String>,
    pub tests: Vec<String>,
    pub code: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceReport {
    pub rows: Vec<TraceRow>,
    pub problems: Vec<String>,
}

pub fn parse_ids(raw: &str) -> Vec<String> {
    let id_re = Regex::new(r"\b(REQ|UC|TEST)-([A-Za-z0-9_-]+)\b").unwrap();
    let mut ids = Vec::new();
    for cap in id_re.captures_iter(raw) {
        if let (Some(prefix), Some(suffix)) = (cap.get(1), cap.get(2)) {
            ids.push(format!("{}-{}", prefix.as_str(), suffix.as_str()));
        }
    }
    ids
}

pub fn collect_docs(doc_dir: &Path, trace: &mut Trace) -> Result<()> {
    if !doc_dir.exists() {
        return Ok(());
    }

    let yaml_block_re = Regex::new(r"(?s)```yaml\s*(.*?)\s*```").unwrap();

    let subdirs = [
        ("requirements", "REQ-"),
        ("usecases", "UC-"),
        ("testplan", "TEST-"),
    ];

    for (subdir, expected_prefix) in subdirs {
        let base = doc_dir.join(subdir);
        if !base.exists() {
            continue;
        }

        for entry in WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                let content = std::fs::read_to_string(path)?;
                for cap in yaml_block_re.captures_iter(&content) {
                    if let Some(block) = cap.get(1) {
                        let block_text = block.as_str();
                        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(block_text) {
                            if let Some(map) = val.as_mapping() {
                                let id_val = map
                                    .get(&serde_yaml::Value::String("id".to_string()))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .trim();

                                if id_val.starts_with(expected_prefix) {
                                    if expected_prefix == "REQ-" {
                                        trace.requirements.insert(id_val.to_string(), path.to_path_buf());
                                    } else if expected_prefix == "UC-" {
                                        let mut impls = Vec::new();
                                        if let Some(raw_impl) = map.get(&serde_yaml::Value::String("implements".to_string())) {
                                            let s = format!("{:?}", raw_impl);
                                            impls.extend(parse_ids(&s));
                                        }
                                        trace.uc_implements.insert(id_val.to_string(), impls);
                                    } else if expected_prefix == "TEST-" {
                                        let mut verifs = Vec::new();
                                        if let Some(raw_verif) = map.get(&serde_yaml::Value::String("verifies".to_string())) {
                                            let s = format!("{:?}", raw_verif);
                                            verifs.extend(parse_ids(&s));
                                        }
                                        trace.test_verifies.insert(id_val.to_string(), verifs);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn collect_code(src_dirs: &[PathBuf], trace: &mut Trace) -> Result<()> {
    let valid_exts = [
        "py", "js", "ts", "go", "rs", "java", "c", "cpp", "h", "hpp",
    ];
    let implements_re = Regex::new(r"@implements\s+(REQ-[A-Za-z0-9_-]+)").unwrap();
    let verifies_re = Regex::new(r"@verifies\s+(TEST-[A-Za-z0-9_-]+)").unwrap();

    for src_dir in src_dirs {
        if !src_dir.exists() {
            continue;
        }

        for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if valid_exts.contains(&ext) {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            for (lineno, line) in content.lines().enumerate() {
                                let line_idx = lineno + 1;
                                for cap in implements_re.captures_iter(line) {
                                    if let Some(req) = cap.get(1) {
                                        trace.code_impl
                                            .entry(req.as_str().to_string())
                                            .or_default()
                                            .push(format!("{}:{}", path.display(), line_idx));
                                    }
                                }
                                for cap in verifies_re.captures_iter(line) {
                                    if let Some(t) = cap.get(1) {
                                        trace.test_annot
                                            .entry(t.as_str().to_string())
                                            .or_default()
                                            .push(format!("{}:{}", path.display(), line_idx));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn build_rows(trace: &Trace) -> (Vec<TraceRow>, BTreeMap<String, Vec<String>>, BTreeMap<String, Vec<String>>) {
    let mut req_to_uc: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (uc, reqs) in &trace.uc_implements {
        for r in reqs {
            req_to_uc.entry(r.clone()).or_default().push(uc.clone());
        }
    }
    for list in req_to_uc.values_mut() {
        list.sort();
    }

    let mut req_to_test: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (t, reqs) in &trace.test_verifies {
        for r in reqs {
            req_to_test.entry(r.clone()).or_default().push(t.clone());
        }
    }
    for list in req_to_test.values_mut() {
        list.sort();
    }

    let mut rows = Vec::new();
    for req in trace.requirements.keys() {
        rows.push(TraceRow {
            req: req.clone(),
            usecases: req_to_uc.get(req).cloned().unwrap_or_default(),
            tests: req_to_test.get(req).cloned().unwrap_or_default(),
            code: trace.code_impl.get(req).cloned().unwrap_or_default(),
        });
    }

    (rows, req_to_uc, req_to_test)
}

pub fn find_problems(trace: &Trace, rows: &[TraceRow]) -> Vec<String> {
    let mut problems = Vec::new();
    let known_reqs: BTreeSet<&String> = trace.requirements.keys().collect();
    let known_tests: BTreeSet<&String> = trace.test_verifies.keys().collect();

    // 1. Gaps
    for row in rows {
        if row.usecases.is_empty() {
            problems.push(format!("GAP      {}: no use case defined", row.req));
        }
        if row.tests.is_empty() {
            problems.push(format!("GAP      {}: no test in test plan", row.req));
        }
        if row.code.is_empty() {
            problems.push(format!("GAP      {}: no @implements annotation in code", row.req));
        }
    }

    // 2. Orphaned code annotations: @implements points to unknown REQ
    for (req, locs) in &trace.code_impl {
        if !known_reqs.contains(req) {
            let loc = locs.first().map(|s| s.as_str()).unwrap_or("unknown");
            problems.push(format!("ORPHAN   {}: annotated in code ({}), but no requirement defined", req, loc));
        }
    }

    // 3. Orphaned UC references
    for (uc, reqs) in &trace.uc_implements {
        for r in reqs {
            if !known_reqs.contains(r) {
                problems.push(format!("ORPHAN   {}: references {}, which does not exist", uc, r));
            }
        }
    }

    // 4. Orphaned Test references
    for (t, reqs) in &trace.test_verifies {
        for r in reqs {
            if !known_reqs.contains(r) {
                problems.push(format!("ORPHAN   {}: references {}, which does not exist", t, r));
            }
        }
    }

    // 5. Orphaned @verifies in code
    for (t, locs) in &trace.test_annot {
        if !known_tests.contains(t) {
            let loc = locs.first().map(|s| s.as_str()).unwrap_or("unknown");
            problems.push(format!("ORPHAN   {}: annotated in test ({}), but not in test plan", t, loc));
        }
    }

    problems
}

pub fn render_matrix_markdown(rows: &[TraceRow]) -> String {
    let mut lines = vec![
        "| Requirement | Usecases | Tests | Code |".to_string(),
        "|---|---|---|---|".to_string(),
    ];
    for row in rows {
        let uc = if row.usecases.is_empty() {
            "— MISSING".to_string()
        } else {
            row.usecases.join(", ")
        };
        let test = if row.tests.is_empty() {
            "— MISSING".to_string()
        } else {
            row.tests.join(", ")
        };
        let code = if row.code.is_empty() {
            "— MISSING".to_string()
        } else {
            format!("{} location(s)", row.code.len())
        };
        lines.push(format!("| {} | {} | {} | {} |", row.req, uc, test, code));
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
