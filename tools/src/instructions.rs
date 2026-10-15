use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

use crate::profile::Profile;
use crate::rulesets;

pub struct SynthesizeOptions {
    pub root: PathBuf,
    pub profile_path: Option<PathBuf>,
    pub out: Option<PathBuf>,
}

pub fn synthesize_instructions(opts: SynthesizeOptions) -> Result<String> {
    let doc_dir = opts.root.join("doc");

    // 1. Load Profile
    let prof_file = opts.profile_path.unwrap_or_else(|| {
        let p1 = doc_dir.join("profile.yaml");
        if p1.exists() {
            p1
        } else {
            let p2 = doc_dir.join("enterprise.yaml");
            if p2.exists() { p2 } else { doc_dir.join("embedded-safety.yaml") }
        }
    });

    // Fail closed: an existing but invalid profile is an error, not "no profile".
    let profile = if prof_file.exists() {
        Some(Profile::load_from_file(&prof_file)?)
    } else {
        None
    };

    let mut out = String::new();
    out.push_str("# AGENT INSTRUCTIONS FOR THE IMPLEMENTATION MODEL (≥27B CODING MODEL)\n\n");
    out.push_str("> Generated automatically by Aegis Systems Engineering Toolkit.\n\n");

    // Section 1: Overview & Domain Profile
    out.push_str("## 1. System Mission & Target Profile\n\n");
    if let Some(ref prof) = profile {
        out.push_str(&format!("- **Domain Profile:** {}\n", prof.profile));
        if let Some(ref rigor) = prof.rigor {
            out.push_str(&format!("- **Safety / Rigor Level:** {}\n", rigor));
        }
        if let Some(ref lang) = prof.language {
            out.push_str(&format!("- **Target Language:** {}\n", lang));
        }
        if let Some(ref desc) = prof.description {
            out.push_str(&format!("- **Overview:** {}\n", desc.trim()));
        }
        if let Some(ref ai) = prof.agent_instructions {
            out.push_str("\n### Profile Directives:\n```\n");
            out.push_str(ai.trim());
            out.push_str("\n```\n");
        }
        if !prof.coding_constraints.is_empty() {
            out.push_str("\n### Hard Coding Constraints:\n");
            for c in &prof.coding_constraints {
                out.push_str(&format!("- {}\n", c));
            }
        }
    } else {
        out.push_str("- **Target Profile:** General High-Reliability Systems\n");
    }

    out.push_str("\n---\n\n");

    // Section 2: Strict Coding Rulesets
    out.push_str("## 2. Mandatory Language Rulesets\n\n");
    // Fail closed: missing, unreadable or unparsable rulesets are errors, never skipped.
    let rulesets = rulesets::load_for_profile(&opts.root, profile.as_ref())?;
    let mut rule_count = 0;

    for (_, rf) in &rulesets {
        out.push_str(&format!("### Ruleset: {} ({})\n\n", rf.ruleset, rf.standard.as_deref().unwrap_or("Standard")));
        let fence = rulesets::fence_language(rf.language.as_deref().or(profile.as_ref().and_then(|p| p.language.as_deref())));
        for rule in &rf.rules {
            rule_count += 1;
            out.push_str(&format!("- **[{}] {}** (Severity: {})\n", rule.id, rule.title, rule.severity.as_deref().unwrap_or("error")));
            out.push_str(&format!("  *Instruction:* {}\n", rule.instruction.trim()));
            if let Some(ref rat) = rule.rationale {
                out.push_str(&format!("  *Rationale:* {}\n", rat.trim()));
            }
            match rule.deviation.as_deref() {
                Some("permitted") => out.push_str(&format!(
                    "  *Deviation:* only with `{}` directly above the statement or suppression attribute.\n",
                    rf.marker_for(&rule.id)
                )),
                Some("forbidden") => out.push_str("  *Deviation:* forbidden.\n"),
                _ => {}
            }
            if let Some(ref bad) = rule.example_bad {
                out.push_str(&format!("\n  Do NOT write:\n\n  ```{}\n", fence));
                for line in bad.trim_end().lines() {
                    out.push_str(&format!("  {}\n", line));
                }
                out.push_str("  ```\n");
            }
            if let Some(ref good) = rule.example_good {
                out.push_str(&format!("\n  Write instead:\n\n  ```{}\n", fence));
                for line in good.trim_end().lines() {
                    out.push_str(&format!("  {}\n", line));
                }
                out.push_str("  ```\n");
            }
            out.push('\n');
        }
        out.push('\n');
    }

    if rule_count == 0 {
        out.push_str("No explicit YAML rulesets configured. Follow standard clean code & safety principles.\n\n");
    }

    out.push_str("---\n\n");

    // Section 3: Traceability & Annotation Mandate
    out.push_str("## 3. Mandatory Source Code Annotations\n\n");
    out.push_str("You **MUST** explicitly link code and tests to specification IDs:\n\n");
    out.push_str("1. **Source Code Functions / Structs:**\n");
    out.push_str("   Above every implemented function, method, or struct, add a comment containing:\n");
    out.push_str("   `@implements REQ-XXX`\n\n");
    out.push_str("2. **Unit & Integration Tests:**\n");
    out.push_str("   Above every automated test case, add a comment containing:\n");
    out.push_str("   `@verifies TEST-XXX`\n\n");
    out.push_str("> [!CAUTION]\n> `aegis gate` will fail automatically if any requirement has missing `@implements` annotations.\n\n");

    out.push_str("3. **Frozen Tests (read-only for you):**\n");
    out.push_str("   The test files listed in `doc/testplan/tests.lock` were written and reviewed before this task and are frozen.\n");
    out.push_str("   You MUST NOT modify, delete, rename, skip or `#[ignore]` them, and you MUST NOT edit `doc/testplan/tests.lock`,\n");
    out.push_str("   `doc/baseline.lock` or anything under `doc/`. Make the frozen tests pass by changing production code only.\n");
    out.push_str("   Never put production code (`@implements`) into a file that contains `@verifies` tests.\n\n");

    out.push_str("---\n\n");

    // Section 3b: Implementation loop protocol
    let agent_loop = profile.as_ref().map(|p| p.agent_loop()).unwrap_or_default();
    let prof_arg = prof_file
        .strip_prefix(&opts.root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| prof_file.display().to_string());
    out.push_str("## 3b. Implementation Loop Protocol\n\n");
    out.push_str("Work on **one task at a time**. A task is one requirement (`REQ-xxx`) together with the frozen tests that verify it.\n");
    out.push_str("For each task:\n\n");
    out.push_str("1. Read the requirement, its architecture element (`ARCH-xxx`) and the frozen tests that verify it.\n");
    out.push_str("2. Implement the smallest change that satisfies the requirement; annotate it with `@implements REQ-xxx`.\n");
    out.push_str("3. Run the fast gate and read its full output:\n\n");
    out.push_str(&format!(
        "   ```bash\n   aegis gate --profile {} --only {}\n   ```\n\n",
        prof_arg,
        agent_loop.fast_stages.join(",")
    ));
    out.push_str(&format!(
        "4. If it is red, fix the root cause and repeat step 3 — at most **{} attempts** per task.\n",
        agent_loop.max_iterations
    ));
    out.push_str("5. If it is still red after the last attempt, or a requirement is ambiguous or contradicts a frozen test, **stop** and\n");
    out.push_str("   write `ESCALATION-<REQ-ID>.md` with: the task, every attempt (change + gate output excerpt), your root-cause\n");
    out.push_str("   hypothesis, and the specification question that blocks you. Do not guess, do not weaken checks, do not\n");
    out.push_str("   add lint suppressions to get green.\n");
    out.push_str("6. When the fast gate is green, continue with the next task. Do not start the next task while the current one is red.\n\n");

    out.push_str("---\n\n");

    // Section 3c: Architecture
    let arch_dir = doc_dir.join("architecture");
    if arch_dir.exists() {
        let mut entries: Vec<PathBuf> = WalkDir::new(&arch_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        entries.sort();
        if !entries.is_empty() {
            out.push_str("## 3c. Architecture (`doc/architecture/`)\n\n");
            for path in entries {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let rel = path.strip_prefix(&opts.root).unwrap_or(&path);
                out.push_str(&format!("### Artifact: {}\n\n", rel.display()));
                out.push_str(content.trim());
                out.push_str("\n\n");
            }
            out.push_str("---\n\n");
        }
    }

    // Section 4: Full Requirements Breakdown
    out.push_str("## 4. Requirements Specification (`doc/requirements/`)\n\n");
    let req_dir = doc_dir.join("requirements");
    let mut req_count = 0;

    if req_dir.exists() {
        let mut entries: Vec<PathBuf> = WalkDir::new(&req_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        entries.sort();

        for path in entries {
            if let Ok(content) = fs::read_to_string(&path) {
                req_count += 1;
                let rel = path.strip_prefix(&opts.root).unwrap_or(&path);
                out.push_str(&format!("### Artifact: {}\n\n", rel.display()));
                out.push_str(content.trim());
                out.push_str("\n\n");
            }
        }
    }

    if req_count == 0 {
        out.push_str("No requirement artifacts found in `doc/requirements/`.\n\n");
    }

    out.push_str("---\n\n");

    // Section 5: Test Plan Specifications
    out.push_str("## 5. Shift-Left Test Plan (`doc/testplan/`)\n\n");
    let test_dir = doc_dir.join("testplan");
    let mut test_count = 0;

    if test_dir.exists() {
        let mut entries: Vec<PathBuf> = WalkDir::new(&test_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.into_path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        entries.sort();

        for path in entries {
            if let Ok(content) = fs::read_to_string(&path) {
                test_count += 1;
                let rel = path.strip_prefix(&opts.root).unwrap_or(&path);
                out.push_str(&format!("### Artifact: {}\n\n", rel.display()));
                out.push_str(content.trim());
                out.push_str("\n\n");
            }
        }
    }

    if test_count == 0 {
        out.push_str("No test plan artifacts found in `doc/testplan/`.\n\n");
    }

    out.push_str("---\n\n");

    // Section 6: Definition of Done for Implementation
    out.push_str("## 6. Definition of Done for Implementation Model\n\n");
    out.push_str("Before considering your task complete, verify that:\n");
    out.push_str("- [ ] Code compiles cleanly with zero warnings.\n");
    out.push_str("- [ ] The fast gate (`aegis gate --only ...`, see 3b) is green for every task.\n");
    out.push_str("- [ ] All frozen tests pass and none of them was modified (`aegis tests verify`).\n");
    out.push_str("- [ ] Every requirement has at least one `@implements REQ-xxx` annotation.\n");
    out.push_str("- [ ] Every task that could not be completed has an `ESCALATION-<REQ-ID>.md` report.\n");
    out.push_str("- [ ] Zero hardcoded secrets, zero unhandled errors, zero undefined behaviors.\n");

    if let Some(out_path) = opts.out {
        let full_out = opts.root.join(&out_path);
        if let Some(parent) = full_out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_out, &out)?;
        println!(
            "{} {}",
            "✔ Synthesized instructions written to:".green().bold(),
            full_out.display().to_string().cyan()
        );
    }

    Ok(out)
}
