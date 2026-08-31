use crate::traceability::{self, Trace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadius {
    pub requirements: Vec<String>,
    pub usecases: Vec<String>,
    pub tests: Vec<String>,
    pub code: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub depth: String,
    pub blast: BlastRadius,
    pub plan: Vec<String>,
}

pub fn resolve_reqs_from_code(trace: &Trace, code_file: &str) -> Vec<String> {
    let mut hits = BTreeSet::new();
    for (req, locs) in &trace.code_impl {
        for loc in locs {
            if loc.starts_with(code_file) || loc.contains(code_file) {
                hits.insert(req.clone());
            }
        }
    }
    hits.into_iter().collect()
}

pub fn compute_impact(
    trace: &Trace,
    targets: &[String],
    code_file: Option<&str>,
    root_str: &str,
) -> BlastRadius {
    let (_, req_to_uc, req_to_test) = traceability::build_rows(trace);

    let mut seed_reqs = BTreeSet::new();
    let mut seed_ucs = BTreeSet::new();
    let mut seed_tests = BTreeSet::new();

    for t in targets {
        if t.starts_with("REQ-") {
            seed_reqs.insert(t.clone());
        } else if t.starts_with("UC-") {
            seed_ucs.insert(t.clone());
        } else if t.starts_with("TEST-") {
            seed_tests.insert(t.clone());
        }
    }

    if let Some(file) = code_file {
        for r in resolve_reqs_from_code(trace, file) {
            seed_reqs.insert(r);
        }
    }

    // Von UC / TEST zurück auf Requirements
    for uc in &seed_ucs {
        if let Some(reqs) = trace.uc_implements.get(uc) {
            for r in reqs {
                seed_reqs.insert(r.clone());
            }
        }
    }
    for t in &seed_tests {
        if let Some(reqs) = trace.test_verifies.get(t) {
            for r in reqs {
                seed_reqs.insert(r.clone());
            }
        }
    }

    // Vorwärts: alle abhängigen Artefakte einsammeln
    let mut affected_uc = seed_ucs;
    let mut affected_test = seed_tests;
    let mut affected_code = BTreeSet::new();

    for r in &seed_reqs {
        if let Some(ucs) = req_to_uc.get(r) {
            for uc in ucs {
                affected_uc.insert(uc.clone());
            }
        }
        if let Some(tests) = req_to_test.get(r) {
            for t in tests {
                affected_test.insert(t.clone());
            }
        }
        if let Some(code_locs) = trace.code_impl.get(r) {
            for loc in code_locs {
                let cleaned = if loc.starts_with(root_str) {
                    loc.strip_prefix(root_str).unwrap_or(loc).to_string()
                } else {
                    loc.clone()
                };
                affected_code.insert(cleaned);
            }
        }
    }

    BlastRadius {
        requirements: seed_reqs.into_iter().collect(),
        usecases: affected_uc.into_iter().collect(),
        tests: affected_test.into_iter().collect(),
        code: affected_code.into_iter().collect(),
    }
}

pub fn classify_depth(kind: &str, _blast: &BlastRadius) -> String {
    match kind {
        "wish" | "feature" => "functional (Requirements/Usecases)".to_string(),
        "bug" => "implementation (Testplan/Code) — assuming requirements are correct".to_string(),
        _ => "functional (Requirements/Usecases) — clarify change type first".to_string(),
    }
}

pub fn make_plan(kind: &str, depth: &str, _blast: &BlastRadius) -> Vec<String> {
    let mut plan = Vec::new();
    plan.push("Create issue in doc/issues/ referencing affected IDs".to_string());

    if depth.starts_with("functional") || depth.starts_with("concept") {
        if kind == "wish" || kind == "feature" {
            plan.push("Formulate or extend requirement (assign REQ-ID)".to_string());
        } else {
            plan.push("Correct affected requirement".to_string());
        }
        plan.push("Requirement review with Frontier models (blind, parallel)".to_string());
        plan.push("Adapt/extend use case(s) and link to REQ".to_string());
    }

    plan.push("Update test plan: Add/update test(s) for changed requirement (verifies: REQ)".to_string());
    plan.push("Update agent instructions (if coding constraints are affected)".to_string());
    plan.push("Implementation by 27B model (with @implements REQ annotation)".to_string());
    plan.push("Final review of changes with Frontier models".to_string());
    plan.push("Run execution gate (profile-based) until green".to_string());
    plan.push("Close issue; regenerate traceability matrix".to_string());

    plan
}

pub fn render_report_text(kind: &str, depth: &str, blast: &BlastRadius, plan: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# Impact Analysis\n\n");
    out.push_str(&format!("**Kind:** {}   **Impact Depth:** {}\n\n", kind, depth));
    out.push_str("## Affected Artifacts (Blast Radius)\n\n");

    let req_str = if blast.requirements.is_empty() { "—".to_string() } else { blast.requirements.join(", ") };
    let uc_str = if blast.usecases.is_empty() { "—".to_string() } else { blast.usecases.join(", ") };
    let test_str = if blast.tests.is_empty() { "—".to_string() } else { blast.tests.join(", ") };

    out.push_str(&format!("- Requirements: {}\n", req_str));
    out.push_str(&format!("- Usecases:     {}\n", uc_str));
    out.push_str(&format!("- Tests:        {}\n", test_str));

    if blast.code.is_empty() {
        out.push_str("- Code:         —\n");
    } else {
        out.push_str("- Code:\n");
        for c in &blast.code {
            out.push_str(&format!("    - {}\n", c));
        }
    }

    out.push_str("\n## Proposed Plan (Confirm step by step)\n\n");
    for (i, step) in plan.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, step));
    }

    out
}
