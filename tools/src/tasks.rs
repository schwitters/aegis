//! Work packets for the implementation model, generated deterministically from the approved artifacts.
//!
//! One packet per requirement (`doc/tasks/TASK-<REQ-ID>.md`) plus one integration packet per architecture element
//! with integration tests (`TASK-<ARCH-ID>.md`). A packet is self-contained: requirement text, use cases,
//! architecture element(s) with crate and interfaces, the frozen test functions to make pass, the rules mentioned in
//! these artifacts, the profile's coding constraints, the fast-gate command, the iteration limit and the escalation
//! format. The queue order follows the layer policy (lowest layer first). Nothing is chosen by a model.

use anyhow::Result;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::architecture;
use crate::baseline;
use crate::profile::Profile;
use crate::rulesets;
use crate::testfreeze;
use crate::traceability;

pub const TASKS_DIR: &str = "doc/tasks";

pub struct TaskReport {
    pub tasks: usize,
    pub problems: Vec<String>,
}

struct Packet {
    id: String,
    sort: (usize, u8, String),
    body: String,
}

/// Rank of every layer in dependency order: layers without dependencies are 0; `*` layers (tests) come last.
pub fn layer_ranks(layers: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, usize> {
    fn rank(l: &str, layers: &BTreeMap<String, Vec<String>>, seen: &mut BTreeSet<String>) -> usize {
        if !seen.insert(l.to_string()) {
            return 0; // cycle: the architecture stage reports it
        }
        let r = layers
            .get(l)
            .map(|deps| deps.iter().filter(|d| *d != "*").map(|d| rank(d, layers, seen) + 1).max().unwrap_or(0))
            .unwrap_or(0);
        seen.remove(l);
        r
    }
    let mut out: BTreeMap<String, usize> = layers.keys().map(|l| (l.clone(), rank(l, layers, &mut BTreeSet::new()))).collect();
    let max = out.values().copied().max().unwrap_or(0);
    for (l, deps) in layers {
        if deps.iter().any(|d| d == "*") {
            out.insert(l.clone(), max + 1);
        }
    }
    out
}

/// Headings of an embedded artifact one level deeper (outside code fences), so they nest under the packet's sections.
fn demote(body: &str) -> String {
    let mut fence = false;
    body.lines()
        .map(|l| {
            if l.trim_start().starts_with("```") {
                fence = !fence;
            }
            if !fence && l.starts_with('#') && l.trim_start_matches('#').starts_with(' ') && !l.starts_with("######") {
                format!("#{}", l)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// File content without its metadata (frontmatter or first ```yaml block with `id`), headings demoted.
fn body_of(path: &Path) -> String {
    demote(&raw_body(path))
}

fn raw_body(path: &Path) -> String {
    let content = std::fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            return rest[end + 4..].trim().to_string();
        }
    }
    let block = Regex::new(r"(?s)```ya?ml\s*\n.*?```").unwrap();
    block.replacen(&content, 1, "").trim().to_string()
}

fn meta_str(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let map = traceability::extract_metadata(&content).ok()??;
    map.get(serde_yaml::Value::String(key.to_string()))?.as_str().map(|s| s.trim().to_string())
}

/// Test function names following `@verifies` annotations (Rust `fn`, Python `def`), else `file:line`.
fn test_functions(locs: &[String], root: &Path) -> Vec<String> {
    let fn_re = Regex::new(r"\b(?:fn|def)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let mut out = Vec::new();
    for loc in locs {
        let (file, line) = match loc.rsplit_once(':') {
            Some((f, l)) => (f, l.parse::<usize>().unwrap_or(0)),
            None => (loc.as_str(), 0),
        };
        let rel = Path::new(file).strip_prefix(root).unwrap_or(Path::new(file)).display().to_string();
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let name = text.lines().skip(line).take(8).find_map(|l| fn_re.captures(l).map(|c| c[1].to_string()));
        out.push(match name {
            Some(n) => format!("`{}` ({})", n, rel),
            None => format!("{}:{}", rel, line),
        });
    }
    out
}

pub fn generate(root: &Path, profile: &Profile, profile_rel: &str, max_tokens: usize, draft: bool) -> Result<TaskReport> {
    let doc_dir = root.join("doc");
    let src_dirs: Vec<PathBuf> = profile.source_paths(root);
    let trace = traceability::collect_project(root, &doc_dir, Some(&src_dirs))?;
    let mut problems = Vec::new();

    // Preconditions: approved baseline, frozen tests, structurally sound specification.
    let (rows, req_to_uc, req_to_test) = traceability::build_rows(&trace);
    let structural = baseline::blocking_for_approval(&traceability::find_problems(&trace, &rows));
    let mut preconditions = structural.clone();
    match baseline::load(&doc_dir)? {
        None => preconditions.push("PRECONDITION no approved baseline (aegis baseline --approve)".to_string()),
        Some(b) => {
            let d = baseline::diff(&b, &baseline::current(root, &trace)?);
            if !d.is_empty() {
                preconditions.push(format!(
                    "PRECONDITION baseline not current: {} changed/new/removed artifact(s)",
                    d.changed.len() + d.added.len() + d.removed.len()
                ));
            }
        }
    }
    match testfreeze::verify(root, &doc_dir, &src_dirs) {
        Ok(p) if p.is_empty() => {}
        Ok(p) => preconditions.push(format!("PRECONDITION frozen tests not intact: {}", p.join("; "))),
        Err(e) => preconditions.push(format!("PRECONDITION {:#}", e)),
    }
    if !preconditions.is_empty() && !draft {
        return Ok(TaskReport { tasks: 0, problems: preconditions });
    }
    let draft_note = if preconditions.is_empty() {
        String::new()
    } else {
        format!(
            "> **DRAFT** — generated with --draft although: {}. Do not hand to the implementation model.\n\n",
            preconditions.join("; ")
        )
    };

    let arch_cfg = profile.get_stage_config("architecture").cloned().unwrap_or_default();
    let ranks = layer_ranks(&arch_cfg.layers);
    let unranked = ranks.values().copied().max().unwrap_or(0) + 2;
    let rulesets = rulesets::load_referenced(root, profile).unwrap_or_default();
    let rule_index: BTreeMap<String, (String, String)> = rulesets
        .iter()
        .flat_map(|rf| rf.rules.iter())
        .map(|r| {
            let first = r.instruction.split(". ").next().unwrap_or("").trim().trim_end_matches('.').to_string();
            (r.id.clone(), (r.title.clone(), first))
        })
        .collect();
    let rule_re = Regex::new(r"RULE-[A-Z]+-[0-9]+").unwrap();
    let agent_loop = profile.agent_loop();
    let fast_gate = format!("aegis gate --profile {} --only {}", profile_rel, agent_loop.fast_stages.join(","));
    let frozen: Vec<String> = testfreeze::read_lock(&doc_dir)?.map(|m| m.keys().cloned().collect()).unwrap_or_default();

    // ARCH element -> (crate, layer)
    let mut arch_info: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    for id in trace.arch_satisfies.keys() {
        let path = &trace.artifacts[id];
        let krate = meta_str(path, "crate");
        let layer = meta_str(path, "layer").or_else(|| {
            krate.as_deref().and_then(|c| architecture::layer_of(c, &arch_cfg.layers)).map(String::from)
        });
        if krate.is_none() && layer.is_none() {
            problems.push(format!("UNPLACED {}: no `crate:` or `layer:` in its metadata — task order and scope unknown", id));
        }
        arch_info.insert(id.clone(), (krate, layer));
    }
    let arch_rank = |a: &str| -> usize {
        arch_info.get(a).and_then(|(_, l)| l.as_ref()).and_then(|l| ranks.get(l)).copied().unwrap_or(unranked)
    };

    let mut req_to_arch: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (a, reqs) in &trace.arch_satisfies {
        for r in reqs {
            req_to_arch.entry(r.clone()).or_default().push(a.clone());
        }
    }

    let rules_section = |texts: &[String]| -> String {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for t in texts {
            for m in rule_re.find_iter(t) {
                ids.insert(m.as_str().to_string());
            }
        }
        let mut out = String::from("## Rules for This Task\n\n");
        for id in &ids {
            match rule_index.get(id) {
                Some((title, first)) => out.push_str(&format!("- **{}** {} — {}.\n", id, title, first)),
                None => out.push_str(&format!("- **{}** (not in the profile's rulesets)\n", id)),
            }
        }
        for c in &profile.coding_constraints {
            out.push_str(&format!("- {}\n", c));
        }
        out.push_str("- All other rules: AGENT_INSTRUCTIONS.md, section 2.\n\n");
        out
    };

    let tests_section = |test_ids: &[String]| -> String {
        let mut out = String::from("## Frozen Tests to Make Pass (read-only)\n\n");
        if test_ids.is_empty() {
            out.push_str("_None — escalate: a task without tests cannot be verified._\n\n");
        }
        for t in test_ids {
            let title = trace.artifacts.get(t).and_then(|p| meta_str(p, "title")).unwrap_or_default();
            let fns = trace.test_annot.get(t).map(|l| test_functions(l, root)).unwrap_or_default();
            out.push_str(&format!(
                "- **{}** {} — {}\n",
                t,
                title,
                if fns.is_empty() { "NOT IMPLEMENTED (escalate)".to_string() } else { fns.join(", ") }
            ));
        }
        out.push('\n');
        out
    };

    let footer = |task_id: &str, escalation: &str, scope: &[String]| -> String {
        format!(
            "## Scope\n\n- Change only: {}\n- Never change: `doc/`, the frozen test files ({}), lock files, `tools/aegis`, other crates.\n\n\
             ## Loop\n\n1. Implement the smallest change; annotate it with the `@implements` IDs of this task.\n\
             2. Run `{}` and read the whole output.\n\
             3. Red: fix the root cause and repeat — at most **{} runs** for {}.\n\
             4. Still red, or the requirement is ambiguous / contradicts a frozen test: stop and write `ESCALATION-{}.md`\n   \
             (blocking question, category, attempts with gate excerpts, root-cause hypothesis, proposed resolution).\n\n\
             ## Done When\n\n- The fast gate is green and every test listed above passes.\n- No frozen file changed (`aegis tests verify`).\n",
            if scope.is_empty() { "the crate(s) of the architecture element(s) above".to_string() } else { scope.join(", ") },
            if frozen.is_empty() { "see doc/testplan/tests.lock".to_string() } else { frozen.join(", ") },
            fast_gate,
            agent_loop.max_iterations,
            task_id,
            escalation
        )
    };

    let mut packets: Vec<Packet> = Vec::new();

    // One packet per requirement
    for (req, req_path) in &trace.requirements {
        let archs = req_to_arch.get(req).cloned().unwrap_or_default();
        let rank = archs.iter().map(|a| arch_rank(a)).max().unwrap_or(unranked);
        let crates: Vec<String> = archs
            .iter()
            .filter_map(|a| arch_info.get(a).and_then(|(c, _)| c.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|c| format!("`crates/{}/`", c))
            .collect();
        let layers: Vec<String> = archs
            .iter()
            .filter_map(|a| arch_info.get(a).and_then(|(_, l)| l.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let tests = req_to_test.get(req).cloned().unwrap_or_default();
        let ucs = req_to_uc.get(req).cloned().unwrap_or_default();
        let title = meta_str(req_path, "title").unwrap_or_default();
        let id = format!("TASK-{}", req);

        let req_body = body_of(req_path);
        let mut texts = vec![req_body.clone()];
        let mut b = format!(
            "---\nid: {id}\nrequirements: [{req}]\narchitecture: [{}]\nlayers: [{}]\ntests: [{}]\ngenerated_by: aegis tasks\n---\n\n\
             <!-- Generated by `aegis tasks` from the approved baseline. Do not edit; regenerate instead. -->\n\n\
             # {id}: {title}\n\n{draft_note}## Requirement {req}\n\n{req_body}\n\n",
            archs.join(", "),
            layers.join(", "),
            tests.join(", "),
        );
        for uc in &ucs {
            let p = &trace.artifacts[uc];
            let body = body_of(p);
            texts.push(body.clone());
            b.push_str(&format!("## Use Case {}: {}\n\n{}\n\n", uc, meta_str(p, "title").unwrap_or_default(), body));
        }
        for a in &archs {
            let p = &trace.artifacts[a];
            let body = body_of(p);
            texts.push(body.clone());
            let (krate, layer) = arch_info.get(a).cloned().unwrap_or((None, None));
            b.push_str(&format!(
                "## Architecture {}: {} (crate `{}`, layer `{}`)\n\n{}\n\n",
                a,
                meta_str(p, "title").unwrap_or_default(),
                krate.unwrap_or_else(|| "?".into()),
                layer.unwrap_or_else(|| "?".into()),
                body
            ));
        }
        b.push_str(&tests_section(&tests));
        b.push_str(&rules_section(&texts));
        b.push_str(&footer(&id, req, &crates));
        packets.push(Packet { id, sort: (rank, 0, req.clone()), body: b });
    }

    // One integration packet per architecture element with integration tests
    let mut arch_tests: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (t, ids) in &trace.test_verifies {
        for i in ids.iter().filter(|i| i.starts_with("ARCH-")) {
            arch_tests.entry(i.clone()).or_default().push(t.clone());
        }
    }
    for (a, tests) in &arch_tests {
        let Some(p) = trace.artifacts.get(a) else { continue };
        let (krate, layer) = arch_info.get(a).cloned().unwrap_or((None, None));
        let id = format!("TASK-{}", a);
        let body = body_of(p);
        let mut b = format!(
            "---\nid: {id}\narchitecture: [{a}]\nlayers: [{}]\ntests: [{}]\ngenerated_by: aegis tasks\n---\n\n\
             <!-- Generated by `aegis tasks` from the approved baseline. Do not edit; regenerate instead. -->\n\n\
             # {id}: integration of {}\n\n{draft_note}All requirement tasks of this element are done; make its integration tests pass.\n\n\
             ## Architecture {a} (crate `{}`)\n\n{body}\n\n",
            layer.clone().unwrap_or_default(),
            tests.join(", "),
            meta_str(p, "title").unwrap_or_default(),
            krate.clone().unwrap_or_else(|| "?".into()),
        );
        b.push_str(&tests_section(tests));
        b.push_str(&rules_section(std::slice::from_ref(&body)));
        let scope: Vec<String> = krate.iter().map(|c| format!("`crates/{}/`", c)).collect();
        b.push_str(&footer(&id, a, &scope));
        packets.push(Packet { id, sort: (unranked + 1, 1, a.clone()), body: b });
    }

    packets.sort_by(|x, y| x.sort.cmp(&y.sort));

    // Write doc/tasks/ (generated files are replaced as a whole)
    let dir = root.join(TASKS_DIR);
    if dir.exists() {
        for e in std::fs::read_dir(&dir)? {
            let p = e?.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("TASK-") && name.ends_with(".md") || name == "README.md" {
                std::fs::remove_file(&p)?;
            }
        }
    }
    std::fs::create_dir_all(&dir)?;
    let mut readme = format!(
        "# Task Queue\n\n> Generated by `aegis tasks` — do not edit. Work strictly in this order, one task at a time.\n\n{draft_note}\
         | # | Task | Layer rank | Tests | ~Tokens |\n|---|---|---|---|---|\n"
    );
    for (i, p) in packets.iter().enumerate() {
        let tokens = p.body.len().div_ceil(4);
        if tokens > max_tokens {
            problems.push(format!(
                "OVERSIZE {}: ~{} tokens > {} — split the requirement or shorten its use cases/architecture text",
                p.id, tokens, max_tokens
            ));
        }
        let tests = p.body.lines().find(|l| l.starts_with("tests: ")).unwrap_or("tests: []").trim_start_matches("tests: ");
        readme.push_str(&format!(
            "| {} | [{}]({}.md) | {} | {} | {} |\n",
            i + 1,
            p.id,
            p.id,
            if p.sort.0 > unranked { "integration".to_string() } else if p.sort.0 == unranked { "unplaced".to_string() } else { p.sort.0.to_string() },
            tests,
            tokens
        ));
        std::fs::write(dir.join(format!("{}.md", p.id)), &p.body)?;
    }
    readme.push_str(&format!(
        "\n## Harness Guardrails\n\nDeny the implementation model writes to `doc/**`, `tools/aegis/**`, `doc/testplan/tests.lock`, \
         `doc/baseline.lock` and the frozen test files; deny `aegis tests freeze`, `aegis baseline`, `git push`. Allow `{}`.\n",
        fast_gate
    ));
    std::fs::write(dir.join("README.md"), readme)?;
    Ok(TaskReport { tasks: packets.len(), problems })
}
