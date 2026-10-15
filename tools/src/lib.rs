pub mod architecture;
pub mod baseline;
pub mod codectx;
pub mod deviation;
pub mod docs;
pub mod escalate;
pub mod gate;
pub mod impact;
pub mod init;
pub mod instructions;
pub mod issue;
pub mod migrations;
pub mod patterns;
pub mod profile;
pub mod review;
pub mod rulesets;
pub mod spec;
pub mod tasks;
pub mod testfreeze;
pub mod traceability;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_id_parsing() {
        let text = "[REQ-001, REQ-FUNC-002, UC-QS-010, TEST-CAP-999]";
        let ids = traceability::parse_ids(text);
        assert_eq!(ids, vec!["REQ-001", "REQ-FUNC-002", "UC-QS-010", "TEST-CAP-999"]);
    }

    #[test]
    fn test_traceability_matrix_and_problems() {
        let mut trace = traceability::Trace::default();
        trace.requirements.insert("REQ-001".to_string(), PathBuf::from("doc/requirements/auth.md"));
        trace.requirements.insert("REQ-002".to_string(), PathBuf::from("doc/requirements/auth.md"));

        trace.uc_implements.insert("UC-001".to_string(), vec!["REQ-001".to_string()]);
        trace.test_verifies.insert("TEST-001".to_string(), vec!["REQ-001".to_string()]);
        trace.code_impl.insert("REQ-001".to_string(), vec!["src/auth.rs:10".to_string()]);

        let (rows, _, _) = traceability::build_rows(&trace);
        assert_eq!(rows.len(), 2);

        let problems = traceability::find_problems(&trace, &rows);
        // REQ-001/REQ-002 unallocated to ARCH, REQ-002 without UC/test/code, TEST-001 not implemented
        assert_eq!(problems.len(), 6, "{:#?}", problems);
        assert!(problems.iter().any(|p| p.contains("GAP      REQ-002: no use case defined")));
        assert!(problems.iter().any(|p| p.contains("TEST-001: specified in test plan but not implemented")));
    }

    #[test]
    fn test_impact_analysis() {
        let mut trace = traceability::Trace::default();
        trace.requirements.insert("REQ-001".to_string(), PathBuf::from("doc/requirements/auth.md"));
        trace.uc_implements.insert("UC-001".to_string(), vec!["REQ-001".to_string()]);
        trace.test_verifies.insert("TEST-001".to_string(), vec!["REQ-001".to_string()]);
        trace.code_impl.insert("REQ-001".to_string(), vec!["src/auth.rs:10".to_string()]);

        let blast = impact::compute_impact(&trace, &["UC-001".to_string()], None, "/root/");
        assert_eq!(blast.requirements, vec!["REQ-001"]);
        assert_eq!(blast.usecases, vec!["UC-001"]);
        assert_eq!(blast.tests, vec!["TEST-001"]);
        assert_eq!(blast.code, vec!["src/auth.rs:10"]);

        let depth = impact::classify_depth("wish", &blast);
        assert!(depth.starts_with("functional"));
        let plan = impact::make_plan("wish", &depth, &blast);
        assert!(plan.len() >= 6);
    }

    #[test]
    fn test_issue_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("dev_process_test_{}", std::process::id()));
        let doc_dir = temp_dir.join("doc");
        std::fs::create_dir_all(&doc_dir).unwrap();

        let (id, file) = issue::create_issue(
            &doc_dir,
            issue::CreateIssueOptions {
                title: "Test Bug".to_string(),
                r#type: "bug".to_string(),
                severity: "high".to_string(),
                stage: Some("implementation".to_string()),
                source_doc: Some("src/main.rs".to_string()),
                source_finding: None,
                assignee: Some("agent".to_string()),
                related: vec!["REQ-001".to_string()],
                context: Some("Kontext".to_string()),
                description: Some("Beschreibung".to_string()),
            },
        )
        .unwrap();

        assert_eq!(id, "ISSUE-0001");
        assert!(file.exists());

        let issues = issue::collect_issues(&doc_dir).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].meta.status, "open");
        assert_eq!(issues[0].meta.severity, "high");

        issue::close_issue(&doc_dir, "ISSUE-0001").unwrap();
        let issues_after = issue::collect_issues(&doc_dir).unwrap();
        assert_eq!(issues_after[0].meta.status, "done");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_instructions_synthesis() {
        let temp_dir = std::env::temp_dir().join(format!("dev_process_synth_{}", std::process::id()));
        let doc_dir = temp_dir.join("doc");
        std::fs::create_dir_all(&doc_dir).unwrap();

        let profile_content = "profile: test-profile\nlanguage: rust\ncoding_constraints:\n  - Strict memory safety.\n";
        std::fs::write(doc_dir.join("profile.yaml"), profile_content).unwrap();

        let synth = instructions::synthesize_instructions(instructions::SynthesizeOptions {
            root: temp_dir.clone(),
            profile_path: None,
            out: None,
        })
        .unwrap();

        assert!(synth.contains("test-profile"));
        assert!(synth.contains("Strict memory safety."));
        assert!(synth.contains("@implements REQ-XXX"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aegis_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &std::path::Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    /// A complete, consistent spec + code chain.
    fn full_chain(root: &std::path::Path) {
        write(root, "doc/requirements/REQ-001.md", "---\nid: REQ-001\ntitle: Login\n---\nbody\n");
        write(root, "doc/architecture/ARCH-001.md", "---\nid: ARCH-001\ntitle: Auth\nsatisfies: [REQ-001]\n---\n");
        write(root, "doc/architecture/overview.md", "# Free-form overview without metadata\n");
        write(root, "doc/usecases/UC-001.md", "```yaml\nid: UC-001\nimplements: [REQ-001]\n```\n");
        write(root, "doc/testplan/TEST-001.md", "---\nid: TEST-001\nverifies: [REQ-001]\n---\n");
        write(root, "doc/testplan/TEST-002.md", "---\nid: TEST-002\nverifies: [ARCH-001]\ntype: integration\n---\n");
        write(root, "src/auth.rs", "/// @implements REQ-001\nfn login() {}\n");
        write(root, "tests/auth_test.rs", "/// @verifies TEST-001\n#[test] fn a() {}\n/// @verifies TEST-002\n#[test] fn b() {}\n");
    }

    fn trace_of(root: &std::path::Path) -> (traceability::Trace, Vec<String>) {
        let mut trace = traceability::Trace::default();
        traceability::collect_docs(&root.join("doc"), &mut trace).unwrap();
        traceability::collect_code(&[root.join("src"), root.join("tests")], &mut trace).unwrap();
        let (rows, _, _) = traceability::build_rows(&trace);
        let problems = traceability::find_problems(&trace, &rows);
        (trace, problems)
    }

    #[test]
    fn test_metadata_extraction_formats() {
        let fm = traceability::extract_metadata("---\nid: REQ-1\n---\nx").unwrap().unwrap();
        assert!(fm.contains_key(serde_yaml::Value::String("id".into())));
        // legacy scaffold: fence inside frontmatter
        assert!(traceability::extract_metadata("---\n```yaml\nid: REQ-1\n```\n---\n").unwrap().is_some());
        assert!(traceability::extract_metadata("text\n```yaml\nid: UC-1\n```\n").unwrap().is_some());
        assert!(traceability::extract_metadata("no metadata").unwrap().is_none());
        assert!(traceability::extract_metadata("---\nid: [unclosed\n---\n").is_err());
        assert!(traceability::extract_metadata("```yaml\nid: : :\n  - x\n```\n").is_err());
    }

    #[test]
    fn test_complete_chain_has_no_problems() {
        let root = temp_root("chain");
        full_chain(&root);
        let (_, problems) = trace_of(&root);
        assert!(problems.is_empty(), "{:#?}", problems);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_malformed_docs_fail_closed() {
        let root = temp_root("malformed");
        full_chain(&root);
        write(&root, "doc/requirements/REQ-002.md", "---\nid: REQ-002\ntitle: [broken\n---\n");
        write(&root, "doc/requirements/notes.md", "just prose, no id\n");
        write(&root, "doc/requirements/dup.md", "---\nid: REQ-001\n---\n");
        write(&root, "doc/usecases/UC-009.md", "---\nid: REQ-009\n---\n");
        write(&root, "doc/testplan/TEST-003.md", "---\nid: TEST-003\nverifies: [REQ-001, banana]\n---\n");
        let (_, problems) = trace_of(&root);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("REQ-002.md: invalid YAML frontmatter"), "{:#?}", problems);
        assert!(has("notes.md: no metadata block"));
        assert!(has("DUPLICATE REQ-001"));
        assert!(has("does not match directory usecases/"));
        assert!(has("entry `banana` is not a valid artifact ID"));
        assert!(has("TEST-003: specified in test plan but not implemented"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_empty_spec_is_not_green() {
        let root = temp_root("empty");
        std::fs::create_dir_all(root.join("doc")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let (_, problems) = trace_of(&root);
        assert!(problems.iter().any(|p| p.starts_with("EMPTY")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_architecture_gaps_and_orphans() {
        let root = temp_root("arch");
        full_chain(&root);
        write(&root, "doc/architecture/ARCH-002.md", "---\nid: ARCH-002\nsatisfies: [REQ-404]\n---\n");
        write(&root, "doc/usecases/UC-002.md", "---\nid: UC-002\nimplements: [ARCH-001]\n---\n");
        let (_, problems) = trace_of(&root);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("ORPHAN   ARCH-002: references REQ-404"), "{:#?}", problems);
        assert!(has("ARCH-002: no integration test verifies"));
        assert!(has("INVALID  UC-002: may not reference ARCH-001"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_baseline_marks_downstream_suspect() {
        let root = temp_root("baseline");
        full_chain(&root);
        let (trace, _) = trace_of(&root);
        let b = baseline::Baseline {
            approved_by: Some("tester".into()),
            approved_at: None,
            artifacts: baseline::current(&root, &trace).unwrap(),
        };
        baseline::write(&root.join("doc"), &b).unwrap();
        let loaded = baseline::load(&root.join("doc")).unwrap().unwrap();
        assert!(baseline::diff(&loaded, &baseline::current(&root, &trace).unwrap()).is_empty());

        write(&root, "doc/requirements/REQ-001.md", "---\nid: REQ-001\ntitle: Login v2\n---\nchanged\n");
        let (trace, _) = trace_of(&root);
        let d = baseline::diff(&loaded, &baseline::current(&root, &trace).unwrap());
        assert_eq!(d.changed, vec!["REQ-001"]);
        let problems = baseline::check(&trace, &d);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("UNREVIEWED REQ-001: changed"), "{:#?}", problems);
        for item in ["ARCH-001", "UC-001", "TEST-001"] {
            assert!(has(&format!("SUSPECT  {}: upstream REQ-001", item)), "{} not suspect", item);
        }
        assert!(has("src/auth.rs:1: annotates REQ-001"));
        // TEST-002 verifies ARCH-001, not REQ-001: unchanged ARCH keeps it clean
        assert!(!has("SUSPECT  TEST-002"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_frozen_tests_detect_tampering() {
        let root = temp_root("freeze");
        full_chain(&root);
        let doc = root.join("doc");
        let srcs = vec![root.join("src"), root.join("tests")];
        let (_, n) = testfreeze::freeze(&root, &doc, &srcs).unwrap();
        assert_eq!(n, 1);
        assert!(testfreeze::verify(&root, &doc, &srcs).unwrap().is_empty());

        write(&root, "tests/auth_test.rs", "/// @verifies TEST-001\n#[test] #[ignore] fn a() {}\n");
        write(&root, "tests/extra_test.rs", "/// @verifies TEST-002\n#[test] fn c() {}\n");
        write(&root, "src/mixed.rs", "/// @implements REQ-001\nfn x() {}\n/// @verifies TEST-001\n#[test] fn t() {}\n");
        let problems = testfreeze::verify(&root, &doc, &srcs).unwrap();
        let has = |needle: &str| problems.iter().any(|p| p.starts_with(needle));
        assert!(has("MODIFIED tests/auth_test.rs"), "{:#?}", problems);
        assert!(has("UNFROZEN tests/extra_test.rs"));
        assert!(has("MIXED    src/mixed.rs"));
        assert!(testfreeze::freeze(&root, &doc, &srcs).is_err(), "mixed files must not be freezable");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn review(id: &str, stage: &str, reviewer: &str, verdict: &str, findings: Vec<review::Finding>) -> review::Review {
        review::Review {
            file_path: PathBuf::from(format!("doc/reviews/{}.md", id)),
            meta: review::ReviewMeta {
                id: id.into(),
                stage: stage.into(),
                artifacts: vec![],
                reviewer: reviewer.into(),
                author: Some("model-a".into()),
                blind: true,
                date: None,
                verdict: verdict.into(),
                findings,
            },
        }
    }

    fn finding(id: &str, severity: &str, status: &str, disposition: Option<&str>, issue: Option<&str>) -> review::Finding {
        review::Finding {
            id: id.into(),
            severity: severity.into(),
            title: "t".into(),
            status: status.into(),
            disposition: disposition.map(|s| s.into()),
            issue: issue.map(|s| s.into()),
        }
    }

    #[test]
    fn test_review_records_check() {
        let issues: std::collections::BTreeSet<String> = ["ISSUE-0001".to_string()].into_iter().collect();
        let ok = vec![
            review("REVIEW-0001", "spec", "model-b", "approved_with_revisions", vec![
                finding("F-1", "critical", "accepted", Some("fixed in v0.2"), None),
                finding("F-2", "medium", "deferred", None, Some("ISSUE-0001")),
                finding("F-3", "low", "open", None, None),
            ]),
            review("REVIEW-0002", "architecture", "model-b", "approved", vec![]),
            review("REVIEW-0003", "architecture", "model-c", "approved", vec![]),
        ];
        let req = vec!["spec".to_string(), "architecture:2".to_string()];
        assert!(review::check(&ok, &issues, &req).is_empty(), "{:#?}", review::check(&ok, &issues, &req));

        let bad = vec![
            review("REVIEW-0001", "spec", "model-a", "approved", vec![
                finding("F-1", "high", "open", None, None),
                finding("F-2", "low", "rejected", None, None),
                finding("F-3", "low", "deferred", None, Some("ISSUE-0099")),
            ]),
            review("REVIEW-0002", "architecture", "model-b", "approved", vec![]),
            review("REVIEW-0003", "testplan", "model-b", "rejected", vec![]),
        ];
        let problems = review::check(&bad, &issues, &req);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("not an independent review"), "{:#?}", problems);
        assert!(has("REVIEW-0001/F-1 [high]"));
        assert!(has("REVIEW-0001/F-2: status `rejected` requires a disposition"));
        assert!(has("deferred to ISSUE-0099, which does not exist"));
        assert!(has("REJECTED REVIEW-0003"));
        assert!(has("architecture review: 2 independent blind approval(s) required, 1 found"));
    }

    #[test]
    fn test_mutation_score_parsing() {
        let root = temp_root("mutants");
        let summary = root.join("summary.json");
        std::fs::write(&summary, r#"{"caught": 7, "missed": 2, "timeout": 1, "unviable": 5}"#).unwrap();
        assert_eq!(gate::mutation_score(&summary).unwrap(), (8, 10));
        let list = root.join("list.json");
        std::fs::write(
            &list,
            r#"{"outcomes": [{"summary": "Success"}, {"summary": "CaughtMutant"}, {"summary": "MissedMutant"}, {"summary": "Unviable"}]}"#,
        )
        .unwrap();
        assert_eq!(gate::mutation_score(&list).unwrap(), (1, 2));
        assert!(gate::mutation_score(&root.join("missing.json")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn gate_with(root: &std::path::Path, yaml: &str) -> gate::GateRunner {
        let prof: profile::Profile = serde_yaml::from_str(yaml).unwrap();
        gate::GateRunner::new(root.to_path_buf(), prof)
    }

    #[test]
    fn test_gate_external_stages_fail_closed() {
        let root = temp_root("gate_ext");
        // A load-test tool on PATH is not a load test.
        let mut g = gate_with(&root, "profile: t\nlanguage: rust\ngate:\n  load_test:\n    active: true\n");
        assert!(!g.run_all().unwrap());
        assert!(matches!(g.results.last().unwrap().1, gate::StageResult::NotExecutable(_)));

        // Coverage without threshold is not a pass.
        let mut g = gate_with(&root, "profile: t\nlanguage: python\ngate:\n  coverage:\n    active: true\n");
        assert!(!g.run_all().unwrap());

        // Promised but unscanned security checks fail.
        let mut g = gate_with(&root, "profile: t\nlanguage: java\ngate:\n  security:\n    active: true\n    checks: [secrets_scan]\n");
        assert!(!g.run_all().unwrap());

        // Command overrides drive external stages, but never internal ones.
        let mut g = gate_with(&root, "profile: t\ngate:\n  load_test:\n    active: true\n    command: \"true\"\n");
        assert!(g.run_all().unwrap());
        let mut g = gate_with(&root, "profile: t\ngate:\n  load_test:\n    active: true\n    command: \"exit 3\"\n");
        assert!(!g.run_all().unwrap());
        assert_eq!(g.results.last().unwrap().1, gate::StageResult::Failed(3));
        let mut g = gate_with(&root, "profile: t\ngate:\n  traceability:\n    active: true\n    command: \"true\"\n");
        assert!(!g.run_all().unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_gate_selection_and_baseline_requirement() {
        let root = temp_root("gate_sel");
        full_chain(&root);
        let yaml = "profile: t\ngate:\n  traceability:\n    active: true\n    baseline: required\n  load_test:\n    active: true\n";
        // Without an approved baseline the required traceability stage is red.
        let mut g = gate_with(&root, yaml).with_selection(vec!["traceability".into()], vec![]).unwrap();
        assert!(!g.run_all().unwrap());

        let (trace, _) = trace_of(&root);
        let b = baseline::Baseline {
            approved_by: Some("t".into()),
            approved_at: None,
            artifacts: baseline::current(&root, &trace).unwrap(),
        };
        baseline::write(&root.join("doc"), &b).unwrap();
        let mut g = gate_with(&root, yaml).with_selection(vec![], vec!["load_test".into()]).unwrap();
        assert!(g.run_all().unwrap());
        assert!(g.results.iter().any(|(_, r)| *r == gate::StageResult::Skipped));

        assert!(gate_with(&root, yaml).with_selection(vec!["nonsense".into()], vec![]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    const RULESET_WITH_TOOLCHAIN: &str = "ruleset: t\nlanguage: rust\n\
deviation_policy:\n  marker: \"// AEGIS-DEVIATION(<RULE-ID>): <justification>\"\n\
toolchain:\n  gate_commands:\n    load_test:\n      - GATE_CMD\n\
rules:\n  - id: RULE-T-001\n    title: ok to deviate\n    instruction: x\n    deviation: permitted\n\
\x20 - id: RULE-T-002\n    title: never\n    instruction: y\n    deviation: forbidden\n";

    fn gate_with_ruleset(root: &std::path::Path, gate_cmd: &str, extra_gate: &str) -> gate::GateRunner {
        write(root, "doc/rules/t.yaml", &RULESET_WITH_TOOLCHAIN.replace("GATE_CMD", gate_cmd));
        gate_with(
            root,
            &format!("profile: t\nlanguage: rust\nrulesets: [doc/rules/t.yaml]\ngate:\n{}", extra_gate),
        )
    }

    #[test]
    fn test_command_tool_detection() {
        assert_eq!(gate::command_tool("gitleaks detect --no-banner").as_deref(), Some("gitleaks"));
        assert_eq!(gate::command_tool("RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps").as_deref(), Some("cargo"));
        assert_eq!(gate::command_tool("cargo deny --all-features check").as_deref(), Some("cargo-deny"));
        assert_eq!(gate::command_tool("cargo +nightly fmt").as_deref(), Some("cargo"));
        assert_eq!(gate::command_tool("! grep -rn 'x y' templates/").as_deref(), Some("grep"));
        assert_eq!(gate::command_tool("true"), None);
    }

    #[test]
    fn test_gate_runs_ruleset_commands_fail_closed() {
        let root = temp_root("gate_rs_cmds");
        let stage = "  load_test:\n    active: true\n    command: \"true\"\n";
        let mut g = gate_with_ruleset(&root, "\"exit 4\"", stage);
        assert!(g.ruleset_error.is_none());
        assert!(!g.run_all().unwrap());
        assert_eq!(g.results.last().unwrap().1, gate::StageResult::Failed(4));

        let mut g = gate_with_ruleset(&root, "aegis-no-such-tool --check", stage);
        assert!(!g.run_all().unwrap());
        assert!(matches!(g.results.last().unwrap().1, gate::StageResult::NotExecutable(_)));

        let mut g = gate_with_ruleset(&root, "\"true\"", stage);
        assert!(g.run_all().unwrap(), "{:?}", g.results);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_gate_rejects_broken_ruleset_setup() {
        let root = temp_root("gate_rs_broken");
        // referenced ruleset missing
        let mut g = gate_with(&root, "profile: t\nrulesets: [doc/rules/missing.yaml]\ngate: {}\n");
        assert!(g.ruleset_error.is_some());
        assert!(!g.run_all().unwrap());
        assert_eq!(g.results[0].0, "Rulesets");
        // gate_commands for a stage that does not exist
        write(&root, "doc/rules/t.yaml", "ruleset: t\ntoolchain:\n  gate_commands:\n    lnit: [\"true\"]\nrules: []\n");
        let g = gate_with(&root, "profile: t\nrulesets: [doc/rules/t.yaml]\ngate: {}\n");
        assert!(g.ruleset_error.as_deref().unwrap_or("").contains("lnit"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_gate_lint_runs_deviation_scan() {
        let root = temp_root("gate_deviation");
        let lint = "  lint:\n    active: true\n    command: \"true\"\n  traceability:\n    paths: [src]\n";
        write(&root, "src/lib.rs", "#[allow(dead_code)]\nfn unused() {}\n");
        let mut g = gate_with_ruleset(&root, "\"true\"", lint);
        assert!(!g.run_all().unwrap(), "unmarked suppression must fail lint");

        write(&root, "src/lib.rs", "// AEGIS-DEVIATION(RULE-T-002): wanted\n#[expect(dead_code, reason = \"x\")]\nfn unused() {}\n");
        let mut g = gate_with_ruleset(&root, "\"true\"", lint);
        assert!(!g.run_all().unwrap(), "forbidden rule cannot be deviated from");

        write(&root, "src/lib.rs", "// AEGIS-DEVIATION(RULE-T-001): generated code\n#[expect(dead_code, reason = \"x\")]\nfn unused() {}\n");
        let mut g = gate_with_ruleset(&root, "\"true\"", lint);
        assert!(g.run_all().unwrap(), "{:?}", g.results);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_instructions_render_ruleset_marker_and_fence() {
        let root = temp_root("instr_rs");
        write(&root, "doc/rules/t.yaml", "ruleset: t\nlanguage: rust\nrules:\n  - id: RULE-T-001\n    title: t\n    instruction: x\n    deviation: permitted\n    example_good: |\n      let x = 1;\n");
        write(&root, "doc/profile.yaml", "profile: p\nlanguage: rust\nrulesets: [doc/rules/t.yaml]\n");
        let out = instructions::synthesize_instructions(instructions::SynthesizeOptions {
            root: root.clone(),
            profile_path: None,
            out: None,
        })
        .unwrap();
        assert!(out.contains("`// AEGIS-DEVIATION(RULE-T-001): <justification>`"), "{}", out);
        assert!(out.contains("```rust"));
        // a profile referencing a missing ruleset is an error, not an empty section
        write(&root, "doc/profile.yaml", "profile: p\nrulesets: [doc/rules/nope.yaml]\n");
        assert!(instructions::synthesize_instructions(instructions::SynthesizeOptions {
            root: root.clone(),
            profile_path: None,
            out: None,
        })
        .is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_init_web_profile_scaffolds_complete_setup() {
        let root = temp_root("init_web");
        init::run_init(&root, init::InitOptions { profile: "web-enterprise".into(), lang: "rust".into(), brownfield: false, name: None }).unwrap();
        for f in ["clippy.toml", "deny.toml", "doc/profile.yaml", "doc/rules/rust-strict.yaml", "doc/rules/rust-service.yaml",
                  "doc/rules/rust-web-enterprise.yaml", "doc/rules/rust-web-htmx.yaml", "doc/rules/rust-web-pwa.yaml", "doc/rules/workspace-lints.toml"] {
            assert!(root.join(f).exists(), "{} missing", f);
        }
        let prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();
        assert_eq!(prof.profile, "web-enterprise");
        let g = gate::GateRunner::new(root.clone(), prof);
        assert!(g.ruleset_error.is_none(), "{:?}", g.ruleset_error);
        assert_eq!(g.rulesets.len(), 3);
        assert!(init::run_init(&root, init::InitOptions { profile: "web-enterprise".into(), lang: "java".into(), brownfield: false, name: None }).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn policy() -> (std::collections::BTreeMap<String, Vec<String>>, std::collections::BTreeMap<String, Vec<String>>) {
        let layers: std::collections::BTreeMap<String, Vec<String>> = [
            ("domain", vec![]),
            ("application", vec!["domain"]),
            ("storage", vec!["domain", "application"]),
            ("api", vec!["domain", "application"]),
            ("server", vec!["domain", "application", "storage", "api"]),
            ("tests", vec!["*"]),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
        .collect();
        let restricted = [("sqlx".to_string(), vec!["storage".to_string()])].into_iter().collect();
        (layers, restricted)
    }

    #[test]
    fn test_architecture_layer_policy() {
        let (layers, restricted) = policy();
        assert_eq!(architecture::layer_of("shop-domain", &layers), Some("domain"));
        assert_eq!(architecture::layer_of("shop-api", &layers), Some("api"));
        assert_eq!(architecture::layer_of("shop-apiary", &layers), None);
        let meta = serde_json::json!({ "packages": [
            { "name": "shop-domain", "dependencies": [ { "name": "shop-storage", "kind": null } ] },
            { "name": "shop-application", "dependencies": [ { "name": "shop-domain", "kind": null } ] },
            { "name": "shop-storage", "dependencies": [ { "name": "shop-application", "kind": null }, { "name": "sqlx", "kind": null } ] },
            { "name": "shop-api", "dependencies": [ { "name": "sqlx", "kind": "dev" }, { "name": "serde", "kind": null } ] },
            { "name": "shop-utils", "dependencies": [] },
            { "name": "shop-tests", "dependencies": [ { "name": "shop-storage", "kind": null } ] }
        ]});
        let problems = architecture::check(&meta, &layers, &restricted);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("shop-domain (domain) -> shop-storage (storage)"), "{:#?}", problems);
        assert!(has("shop-api (api) -> sqlx [dev]: restricted"));
        assert!(has("UNLAYERED shop-utils"));
        assert_eq!(problems.len(), 3, "{:#?}", problems);
        assert!(!architecture::check(&meta, &Default::default(), &restricted).is_empty(), "empty policy must fail");
    }

    #[test]
    fn test_init_scaffold_satisfies_layer_policy() {
        let root = temp_root("init_layers");
        init::run_init(&root, init::InitOptions { profile: "web-enterprise".into(), lang: "rust".into(), brownfield: false, name: Some("My Shop".into()) }).unwrap();
        for f in ["Cargo.toml", "crates/my-shop-domain/src/lib.rs", "crates/my-shop-server/src/main.rs", "tests/Cargo.toml",
                  "tests/it/main.rs", "openapi/my-shop-openapi.yaml", "migrations/.gitkeep"] {
            assert!(root.join(f).exists(), "{} missing", f);
        }
        let out = std::process::Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps", "--offline"])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();
        let cfg = prof.get_stage_config("architecture").cloned().unwrap();
        let problems = architecture::check(&meta, &cfg.layers, &cfg.restricted);
        assert!(problems.is_empty(), "{:#?}", problems);
        assert_eq!(meta["packages"].as_array().unwrap().len(), 6);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_documentation_coverage_and_suspect_pages() {
        let root = temp_root("docs_cov");
        full_chain(&root);
        write(&root, "doc/usecases/UC-002.md", "---\nid: UC-002\nimplements: [REQ-001]\naudience: operator\n---\n");
        write(&root, "doc/usecases/UC-003.md", "---\nid: UC-003\nimplements: [REQ-001]\naudience: none\n---\n");
        write(&root, "doc/usecases/UC-004.md", "---\nid: UC-004\nimplements: [REQ-001]\naudience: admins\n---\n");
        write(&root, "docs/user/content/login.md", "---\ndocuments: [UC-001, UC-002]\n---\n# Login\n");
        write(&root, "docs/user/site/ignored.md", "---\ndocuments: [UC-999]\n---\n");
        write(&root, "docs/user/content/broken.md", "---\ndocuments: [UC-001\n---\n");
        write(&root, "docs/operator/content/x.md", "---\ndocuments: [UC-404]\n---\n");
        write(&root, "docs/manuals/content/y.md", "# wrong handbook\n");
        let (mut trace, _) = trace_of(&root);
        traceability::collect_documentation(&root.join("docs"), &mut trace).unwrap();
        let (rows, _, _) = traceability::build_rows(&trace);
        let problems = traceability::find_problems(&trace, &rows);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        // UC-002 is documented, but only in the user handbook: it belongs to the operator handbook
        assert!(has("UC-002: not documented in the operator handbook"), "{:#?}", problems);
        assert!(!has("UC-001: not documented"));
        assert!(!has("UC-003"), "audience none needs no page");
        assert!(has("UC-004: audience `admins`"));
        assert!(has("broken.md: invalid YAML frontmatter"));
        assert!(has("ORPHAN   UC-404: documented in"));
        assert!(has("unknown handbook `manuals`"));
        assert!(!has("UC-999"), "build output must be skipped");

        // a changed use case marks its documentation pages SUSPECT
        let b = baseline::Baseline { approved_by: None, approved_at: None, artifacts: baseline::current(&root, &trace).unwrap() };
        write(&root, "doc/usecases/UC-001.md", "---\nid: UC-001\nimplements: [REQ-001]\n---\nchanged\n");
        let (mut trace, _) = trace_of(&root);
        traceability::collect_documentation(&root.join("docs"), &mut trace).unwrap();
        let d = baseline::diff(&b, &baseline::current(&root, &trace).unwrap());
        let suspects = baseline::check(&trace, &d);
        assert!(suspects.iter().any(|p| p.contains("login.md: documents UC-001")), "{:#?}", suspects);
        let blast = impact::compute_impact(&trace, &["UC-001".to_string()], None, &format!("{}/", root.display()));
        assert_eq!(blast.documentation, vec!["docs/user/content/login.md".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_docs_csp_hashes() {
        // SHA-256("abc") in base64, the standard test vector
        use sha2::Digest;
        assert_eq!(docs::base64(&sha2::Sha256::digest(b"abc")), "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=");
        assert_eq!(docs::base64(b"a"), "YQ==");
        assert_eq!(docs::base64(b"ab"), "YWI=");
        let mut h = docs::CspHashes::default();
        docs::scan_html(
            "<script>var a=1;</script><script src=\"x.js\"></script><script id=\"__config\" type=\"application/json\">{}</script>\
             <SCRIPT type=\"module\">import x</SCRIPT><style>p{}</style><p style=\"color:red\">x</p><scripts>no</scripts>",
            &mut h,
        );
        assert_eq!(h.script.len(), 2, "{:?}", h.script);
        assert_eq!(h.style.len(), 1);
        assert_eq!(h.style_attributes, 1);
        assert!(h.script.iter().all(|s| s.starts_with("'sha256-") && s.ends_with("='")));
    }

    #[test]
    fn test_init_web_scaffolds_handbooks() {
        let root = temp_root("init_docs");
        init::run_init(&root, init::InitOptions { profile: "web-enterprise".into(), lang: "rust".into(), brownfield: false, name: Some("shop".into()) }).unwrap();
        for book in traceability::HANDBOOKS {
            let yml = std::fs::read_to_string(root.join(format!("docs/{}/mkdocs.yml", book))).unwrap();
            let v: serde_yaml::Value = serde_yaml::from_str(&yml).unwrap();
            assert!(v["site_name"].as_str().unwrap().starts_with("shop "));
            assert_eq!(v["theme"]["font"], serde_yaml::Value::Bool(false), "no Google Fonts");
            assert!(root.join(format!("docs/{}/content/index.md", book)).exists());
        }
        for f in ["docs/build.sh", "docs/collect-legal.sh", "docs/about.hbs", "docs/requirements.txt", "about.toml",
                  "docs/operator/content/legal/index.md", "docs/technical/content/api.md"] {
            assert!(root.join(f).exists(), "{} missing", f);
        }
        let legal = std::fs::read_to_string(root.join("docs/collect-legal.sh")).unwrap();
        assert!(legal.contains("crates/shop-server/"), "slug substituted");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_spec_sections_and_anchors() {
        assert_eq!(spec::slug("3.2 Login & Sessions (MVP)"), "32-login--sessions-mvp");
        assert_eq!(spec::slug("Use `TenantScope` [here](x.md)"), "use-tenantscope-here");
        let md = "# Product\n\n## Goals\n\n```\n# not a heading\n```\n## Goals\n### Sub *goal*\n# Annex\n";
        let s = spec::parse_sections("spec/p.md", md);
        let keys: Vec<&str> = s.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["spec/p.md#product", "spec/p.md#goals", "spec/p.md#goals-1", "spec/p.md#sub-goal", "spec/p.md#annex"]);
        assert_eq!(s[3].parent.as_deref(), Some("spec/p.md#goals-1"));
        assert_eq!(s[4].parent, None);
        assert_eq!(spec::parse_sections("spec/plain.md", "no headings")[0].key, "spec/plain.md");
    }

    #[test]
    fn test_spec_coverage_citations_and_suspects() {
        let root = temp_root("spec_cov");
        full_chain(&root);
        write(&root, "spec/product.md", "# Product\n## Login\n## Reporting\n## History\n");
        write(&root, "spec/nfr.md", "# Quality\n## Performance\n");
        // REQ-001 cites a section; ARCH and UC cite nothing / something broken
        write(&root, "doc/requirements/REQ-001.md", "---\nid: REQ-001\ntitle: Login\nsource: [spec/product.md#login]\n---\n");
        write(&root, "doc/usecases/UC-001.md", "---\nid: UC-001\nimplements: [REQ-001]\nsource: spec/product.md#logn\n---\n");
        write(&root, "doc/spec-coverage.yaml", "exceptions:\n  - section: spec/product.md#history\n    reason: document history\n  - section: spec/nfr.md#quality\n    issue: ISSUE-0042\n  - section: spec/product.md#nope\n    reason: x\n");
        let trace = traceability::collect_project(&root, &root.join("doc"), Some(&[root.join("src"), root.join("tests")])).unwrap();
        assert!(trace.spec_scanned);
        let (rows, _, _) = traceability::build_rows(&trace);
        let problems = traceability::find_problems(&trace, &rows);
        let has = |ps: &Vec<String>, needle: &str| ps.iter().any(|p| p.contains(needle));
        assert!(has(&problems, "UC-001: source `spec/product.md#logn` does not resolve"), "{:#?}", problems);
        assert!(has(&problems, "UNSOURCED ARCH-001"));
        assert!(!has(&problems, "UNSOURCED REQ-001"));
        assert!(!has(&problems, "UNSOURCED TEST"), "tests need no source");

        let cov = spec::coverage_problems(&trace, &spec::load_exceptions(&root.join("doc")).unwrap(), &Default::default(), 3);
        assert!(has(&cov, "UNCOVERED spec/product.md#reporting"), "{:#?}", cov);
        assert!(!has(&cov, "UNCOVERED spec/product.md#product"), "parent covered through its cited child");
        assert!(!has(&cov, "spec/product.md#history\""), "excepted");
        assert!(has(&cov, "references ISSUE-0042, which does not exist"));
        assert!(has(&cov, "exception for unknown section `spec/product.md#nope`"));
        assert!(!has(&cov, "UNCOVERED spec/nfr.md#performance"), "covered by the exception of its parent");
        assert!(!baseline::blocking_for_approval(&cov).is_empty(), "uncovered spec blocks approval");
        let report = spec::render_report(&trace, &spec::load_exceptions(&root.join("doc")).unwrap());
        assert!(report.contains("| `spec/product.md#login` |") && report.contains("REQ-001"));

        // a changed spec file marks the artifacts citing it SUSPECT
        let b = baseline::Baseline { approved_by: None, approved_at: None, artifacts: baseline::current(&root, &trace).unwrap() };
        assert!(b.artifacts.contains_key("SPEC:spec/product.md"));
        write(&root, "spec/product.md", "# Product\n## Login\nnow with SSO\n## Reporting\n## History\n");
        let trace = traceability::collect_project(&root, &root.join("doc"), None).unwrap();
        let d = baseline::diff(&b, &baseline::current(&root, &trace).unwrap());
        let suspects = baseline::check(&trace, &d);
        assert!(has(&suspects, "SUSPECT  REQ-001: cites spec/product.md"), "{:#?}", suspects);
        assert!(has(&suspects, "UNREVIEWED SPEC:spec/product.md"));
        let _ = std::fs::remove_dir_all(&root);
    }

    const TASK_PROFILE: &str = "profile: t\nlanguage: rust\ncoding_constraints: [\"Use typed errors.\"]\n\
gate:\n  architecture:\n    layers:\n      domain: []\n      api: [domain]\n      tests: [\"*\"]\n\
agent_loop:\n  max_iterations: 2\n  fast_stages: [build, tests]\n";

    fn task_project(root: &std::path::Path) {
        full_chain(root);
        write(root, "doc/architecture/ARCH-001.md", "---\nid: ARCH-001\ntitle: Auth domain\ncrate: shop-domain\nsatisfies: [REQ-001]\n---\nOwns credentials. See RULE-RS-001.\n");
        write(root, "doc/requirements/REQ-002.md", "---\nid: REQ-002\ntitle: Login endpoint\n---\nPOST /login returns 204.\n");
        write(root, "doc/architecture/ARCH-002.md", "---\nid: ARCH-002\ntitle: HTTP\ncrate: shop-api\nsatisfies: [REQ-002]\n---\nAxum routes.\n");
        write(root, "doc/usecases/UC-002.md", "---\nid: UC-002\nimplements: [REQ-002]\n---\n");
        write(root, "doc/testplan/TEST-003.md", "---\nid: TEST-003\ntitle: login 204\nverifies: [REQ-002]\n---\n");
        write(root, "tests/api_test.rs", "/// @verifies TEST-003\n#[test]\nfn login_returns_204() {}\n");
        write(root, "src/api.rs", "/// @implements REQ-002\nfn login() {}\n");
        write(root, "doc/profile.yaml", TASK_PROFILE);
    }

    #[test]
    fn test_tasks_require_baseline_and_frozen_tests() {
        let root = temp_root("tasks_pre");
        task_project(&root);
        let prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();
        let r = tasks::generate(&root, &prof, "doc/profile.yaml", 6000, false).unwrap();
        assert_eq!(r.tasks, 0);
        assert!(r.problems.iter().any(|p| p.contains("no approved baseline")), "{:?}", r.problems);
        assert!(r.problems.iter().any(|p| p.contains("tests.lock missing")));
        assert!(!root.join("doc/tasks").exists(), "nothing written without approval");

        let r = tasks::generate(&root, &prof, "doc/profile.yaml", 6000, true).unwrap();
        assert!(r.tasks > 0);
        assert!(std::fs::read_to_string(root.join("doc/tasks/TASK-REQ-001.md")).unwrap().contains("**DRAFT**"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_tasks_generate_ordered_self_contained_packets() {
        let root = temp_root("tasks_gen");
        task_project(&root);
        let (trace, _) = trace_of(&root);
        let b = baseline::Baseline { approved_by: Some("t".into()), approved_at: None, artifacts: baseline::current(&root, &trace).unwrap() };
        baseline::write(&root.join("doc"), &b).unwrap();
        testfreeze::freeze(&root, &root.join("doc"), &[root.join("src"), root.join("tests")]).unwrap();
        let prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();

        let r = tasks::generate(&root, &prof, "doc/profile.yaml", 6000, false).unwrap();
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(r.tasks, 3, "two requirements + one ARCH integration packet");
        let readme = std::fs::read_to_string(root.join("doc/tasks/README.md")).unwrap();
        let pos = |s: &str| readme.find(s).unwrap();
        assert!(pos("TASK-REQ-001") < pos("TASK-REQ-002"), "domain before api");
        assert!(pos("TASK-REQ-002") < pos("TASK-ARCH-001"), "integration last");

        let t1 = std::fs::read_to_string(root.join("doc/tasks/TASK-REQ-001.md")).unwrap();
        assert!(t1.contains("crate `shop-domain`, layer `domain`"));
        assert!(t1.contains("`a` (tests/auth_test.rs)"), "frozen test function named: {}", t1);
        assert!(t1.contains("RULE-RS-001"), "rule mentioned in ARCH is listed");
        assert!(t1.contains("Use typed errors."));
        assert!(t1.contains("aegis gate --profile doc/profile.yaml --only build,tests"));
        assert!(t1.contains("at most **2 runs**"));
        assert!(t1.contains("`crates/shop-domain/`"));
        let t2 = std::fs::read_to_string(root.join("doc/tasks/TASK-REQ-002.md")).unwrap();
        assert!(t2.contains("`login_returns_204` (tests/api_test.rs)"));

        // stale packets are replaced, oversize packets are reported
        write(&root, "doc/tasks/TASK-REQ-999.md", "old");
        let r = tasks::generate(&root, &prof, "doc/profile.yaml", 50, false).unwrap();
        assert!(!root.join("doc/tasks/TASK-REQ-999.md").exists());
        assert!(r.problems.iter().any(|p| p.starts_with("OVERSIZE TASK-REQ-001")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_codectx_extracts_items() {
        let src = "/// Doc\n#[test]\nfn t() {\n    let s = \"}{\";\n    assert_eq!(f('}'), 1); // }\n}\nfn next() {}\n";
        let lines: Vec<&str> = src.lines().collect();
        assert_eq!(codectx::item_end(&lines, 2), Some(5));
        assert_eq!(codectx::item_end(&["pub struct Unit;"], 0), Some(0));
        let root = temp_root("codectx");
        write(&root, "crates/shop-domain/src/lib.rs",
            "/// Credentials.\npub struct Credentials {\n    pub email: String,\n}\n\n/// Verify.\n// STUB\npub fn verify(c: &Credentials) -> Result<UserId, AuthError> {\n    todo!()\n}\n\npub fn done() -> u8 { 1 }\n\npub enum AuthError { Invalid }\npub struct UserId(u64);\n");
        let stubs = codectx::stubs(&[root.join("crates/shop-domain")], &root);
        assert_eq!(stubs.len(), 1, "{:?}", stubs);
        assert!(stubs[0].1.starts_with("/// Verify.") && stubs[0].1.contains("todo!()"));
        let types = codectx::referenced_types(&[stubs[0].1.clone()], &[root.join("crates")], &root);
        assert_eq!(types.keys().cloned().collect::<Vec<_>>(), vec!["AuthError", "Credentials", "UserId"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_tasks_embed_test_sources_stubs_and_types() {
        let root = temp_root("tasks_ctx");
        task_project(&root);
        write(&root, "crates/shop-domain/src/lib.rs",
            "/// Login check.\n// STUB\npub fn check(c: &Login) -> bool {\n    todo!()\n}\n\n/// Login input.\npub struct Login {\n    pub user: String,\n}\n");
        let mut prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();
        prof.gate.insert("traceability".into(), profile::GateStageConfig { paths: vec!["src".into(), "tests".into(), "crates".into()], ..Default::default() });
        let r = tasks::generate(&root, &prof, "doc/profile.yaml", 6000, true).unwrap();
        assert!(r.tasks > 0);
        let t1 = std::fs::read_to_string(root.join("doc/tasks/TASK-REQ-001.md")).unwrap();
        assert!(t1.contains("```rust\n// tests/auth_test.rs:1\n/// @verifies TEST-001\n#[test] fn a() {}"), "{}", t1);
        assert!(t1.contains("### Stubs to Implement") && t1.contains("pub fn check(c: &Login) -> bool"));
        assert!(t1.contains("### Types Used by the Tests and Stubs") && t1.contains("pub struct Login"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_parse_libtest_and_test_fns() {
        let out = "running 3 tests\ntest it::login::ok_case ... ok\ntest it::login::bad_case ... FAILED\ntest slow ... ignored\n";
        let r = testfreeze::parse_libtest(out);
        assert_eq!(r.get("it::login::ok_case").map(|s| s.as_str()), Some("ok"));
        assert_eq!(r.get("it::login::bad_case").map(|s| s.as_str()), Some("FAILED"));
        assert_eq!(r.get("slow").map(|s| s.as_str()), Some("ignored"));
        assert_eq!(testfreeze::test_fns_in("/// @verifies TEST-1\n#[test]\nfn a() {}\n// @verifies TEST-2\n#[tokio::test]\nasync fn b() {}\n"), vec!["a", "b"]);
    }

    fn cargo_project(root: &std::path::Path, test_body: &str) {
        write(root, "Cargo.toml", "[package]\nname = \"freezeprobe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n");
        write(root, "src/lib.rs", "/// @implements REQ-001\npub fn add(a: i32, b: i32) -> i32 {\n    let _ = (a, b);\n    todo!()\n}\n");
        write(root, "tests/it.rs", test_body);
    }

    #[test]
    fn test_freeze_requires_compiling_failing_tests() {
        let root = temp_root("freeze_cargo");
        let doc = root.join("doc");
        let srcs = vec![root.join("src"), root.join("tests")];
        // does not compile (wrong signature)
        cargo_project(&root, "/// @verifies TEST-001\n#[test]\nfn adds() { assert_eq!(freezeprobe::add(1), 2); }\n");
        let e = testfreeze::freeze_checked(&root, &doc, &srcs, testfreeze::FreezeMode::FailFirst).unwrap_err();
        assert!(format!("{:#}", e).contains("do not compile"), "{:#}", e);
        // compiles and fails against the stub -> frozen
        cargo_project(&root, "/// @verifies TEST-001\n#[test]\nfn adds() { assert_eq!(freezeprobe::add(1, 1), 2); }\n");
        testfreeze::freeze_checked(&root, &doc, &srcs, testfreeze::FreezeMode::FailFirst).unwrap();
        assert!(testfreeze::locked_tests(&doc).unwrap().contains(&("tests/it.rs".to_string(), "adds".to_string())));
        // a new test that already passes is rejected; characterization accepts it
        write(&root, "tests/it.rs", "/// @verifies TEST-001\n#[test]\nfn adds() { assert_eq!(freezeprobe::add(1, 1), 2); }\n/// @verifies TEST-001\n#[test]\nfn trivially_true() { assert!(true); }\n");
        let e = testfreeze::freeze_checked(&root, &doc, &srcs, testfreeze::FreezeMode::FailFirst).unwrap_err();
        assert!(format!("{:#}", e).contains("trivially_true (tests/it.rs): passes"), "{:#}", e);
        assert!(!format!("{:#}", e).contains("adds ("), "previously frozen tests are not re-checked");
        testfreeze::freeze_checked(&root, &doc, &srcs, testfreeze::FreezeMode::Characterization).unwrap();
        // ignored tests never freeze
        write(&root, "tests/it.rs", "/// @verifies TEST-001\n#[test]\n#[ignore]\nfn later() { assert_eq!(freezeprobe::add(2, 2), 4); }\n");
        let e = testfreeze::freeze_checked(&root, &doc, &srcs, testfreeze::FreezeMode::Characterization).unwrap_err();
        assert!(format!("{:#}", e).contains("#[ignore]d"), "{:#}", e);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migration_safety_check() {
        let sql = "CREATE TABLE a (id INTEGER);\n\nDROP TABLE old_a;\n\n-- AEGIS-DEVIATION(RULE-RWE-015): unused since 1.4\nALTER TABLE a\n  DROP COLUMN legacy;\n\nDELETE FROM a WHERE id = 1;\nDELETE FROM a; -- oops\nUPDATE a SET id = 2;\n-- AEGIS-DEVIATION(RULE-RWE-015):\nTRUNCATE a;\nALTER TABLE a RENAME TO b;\n";
        let f = migrations::check_text(sql, "migrations/0002.sql", "RULE-RWE-015");
        let has = |needle: &str| f.iter().any(|x| x.contains(needle));
        assert!(has("0002.sql:3: drops a schema object"), "{:#?}", f);
        assert!(!has(":6:"), "marked deviation passes");
        assert!(!has(":9:"), "DELETE with WHERE is fine");
        assert!(has(":10: deletes all rows"));
        assert!(has(":11: updates all rows"));
        assert!(has(":13: truncates"), "marker without justification does not count");
        assert!(has(":14: renames"));
        assert_eq!(f.len(), 5, "{:#?}", f);
    }

    #[test]
    fn test_escalation_bundle() {
        let root = temp_root("escalate");
        task_project(&root);
        let git = |args: &[&str]| { std::process::Command::new("git").args(args).current_dir(&root).output().unwrap(); };
        git(&["init", "-q"]);
        git(&["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
        git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"]);
        let prof = profile::Profile::load_from_file(root.join("doc/profile.yaml")).unwrap();
        tasks::generate(&root, &prof, "doc/profile.yaml", 6000, true).unwrap();
        write(&root, "src/api.rs", "/// @implements REQ-002\nfn login() { let x: u8 = 300; }\n");
        write(&root, "src/new_helper.rs", "pub fn helper() {}\n");
        write(&root, "ESCALATION-REQ-002.md", "# Escalation: REQ-002\nOverflow I cannot fix.\n");
        let path = escalate::bundle(&root, &prof, "doc/profile.yaml", "REQ-002", Some("error[E0080]: literal out of range".into())).unwrap();
        let b = std::fs::read_to_string(path).unwrap();
        assert!(b.contains("at most **2 attempts**"));
        assert!(b.contains("# TASK-REQ-002: Login endpoint"), "task packet embedded");
        assert!(b.contains("Overflow I cannot fix."));
        assert!(b.contains("error[E0080]"));
        assert!(b.contains("+fn login() { let x: u8 = 300; }"), "{}", b);
        assert!(b.contains("src/new_helper.rs (new file)"));
        assert!(!b.contains("doc/tasks/TASK"), "doc/ excluded from the diff");
        assert!(escalate::bundle(&root, &prof, "doc/profile.yaml", "REQ-404", Some(String::new())).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_glob_and_forbidden_patterns() {
        let g = patterns::glob_regex("crates/*-domain/src/**");
        assert!(g.is_match("crates/shop-domain/src/lib.rs"));
        assert!(g.is_match("crates/shop-domain/src/a/b.rs"));
        assert!(!g.is_match("crates/shop-api/src/lib.rs"));
        assert!(patterns::glob_regex("src/**/*.rs").is_match("src/x.rs"));
        assert!(patterns::glob_regex("*-domain").is_match("shop-domain") && !patterns::glob_regex("*-domain").is_match("a/shop-domain"));

        let root = temp_root("patterns");
        write(&root, "crates/shop-domain/src/lib.rs", "// Utc::now is forbidden here (comment ok)\npub fn f() { let _ = chrono::Utc::now(); }\n// AEGIS-DEVIATION(RULE-P-OK): ISSUE-0001 test\nfn g() { std::env::var(\"X\"); }\n");
        write(&root, "crates/shop-api/src/lib.rs", "// AEGIS-DEVIATION(RULE-P-OK): ISSUE-0001 legacy\nfn g() { std::env::var(\"X\"); }\nfn h() { std::env::var(\"Y\"); }\n");
        let pats = vec![
            rulesets::ForbiddenPattern { rule: "RULE-P-NO".into(), paths: vec!["crates/*-domain/src/**".into()], pattern: r"\bUtc::now".into(), message: "no clock".into() },
            rulesets::ForbiddenPattern { rule: "RULE-P-OK".into(), paths: vec!["crates/**/src/**".into()], pattern: r"std::env::".into(), message: "no env".into() },
        ];
        let rules: std::collections::BTreeMap<String, deviation::DeviationRule> =
            [("RULE-P-NO".to_string(), deviation::DeviationRule::Forbidden), ("RULE-P-OK".to_string(), deviation::DeviationRule::Permitted)].into_iter().collect();
        let zones = vec![patterns::glob_regex("crates/*-domain/**")];
        let f = patterns::scan(&pats, &[root.join("crates")], &root, &rules, &zones);
        let has = |needle: &str| f.iter().any(|x| x.contains(needle));
        assert!(has("shop-domain/src/lib.rs:2: [RULE-P-NO]"), "{:#?}", f);
        assert!(!has("shop-domain/src/lib.rs:1:"), "comment lines are skipped");
        assert!(has("shop-domain/src/lib.rs:4: [RULE-P-OK]"), "marker does not count inside a deviation-free zone");
        assert!(!has("shop-api/src/lib.rs:2:"), "permitted rule + marker outside the zone");
        assert!(has("shop-api/src/lib.rs:3: [RULE-P-OK]"));
        assert_eq!(f.len(), 3, "{:#?}", f);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_deviation_policy_issue_and_zones() {
        let policy = deviation::Policy {
            require_issue: true,
            issue_ids: ["ISSUE-0007".to_string()].into_iter().collect(),
            forbid_in: vec![patterns::glob_regex("crates/*-domain/**")],
        };
        let text = "// AEGIS-DEVIATION(RULE-X-1): no issue here\n// AEGIS-DEVIATION(RULE-X-1): ISSUE-0099 unknown\n// AEGIS-DEVIATION(RULE-X-1): ISSUE-0007 tracked\n";
        let f = deviation::policy_findings(text, "crates/shop-api/src/a.rs", &policy);
        assert_eq!(f.len(), 2, "{:?}", f);
        assert!(f[0].message.contains("must reference"));
        assert!(f[1].message.contains("ISSUE-0099"));
        let f = deviation::policy_findings("// AEGIS-DEVIATION(RULE-X-1): ISSUE-0007 tracked\n", "crates/shop-domain/src/a.rs", &policy);
        assert!(f.iter().any(|x| x.message.contains("not permitted in this path")));
    }

    #[test]
    fn test_architecture_allowlist() {
        let (layers, restricted) = policy();
        let allowed = [("domain".to_string(), vec!["serde".to_string(), "thiserror".to_string()])].into_iter().collect();
        let meta = serde_json::json!({ "packages": [
            { "name": "shop-domain", "dependencies": [
                { "name": "serde", "kind": null }, { "name": "tokio", "kind": null }, { "name": "proptest", "kind": "dev" } ] },
            { "name": "shop-api", "dependencies": [ { "name": "tokio", "kind": null } ] }
        ]});
        let p = architecture::check_with_allowlist(&meta, &layers, &restricted, &allowed);
        assert_eq!(p.len(), 1, "{:#?}", p);
        assert!(p[0].contains("shop-domain (domain) -> tokio [normal]: not in the layer's allowlist"));
        let bad: std::collections::BTreeMap<String, Vec<String>> = [("nolayer".to_string(), vec![])].into_iter().collect();
        assert!(architecture::check_with_allowlist(&meta, &layers, &restricted, &bad).iter().any(|x| x.contains("unknown layer nolayer")));
    }

    #[test]
    fn test_ignore_reason_and_shipped_patterns() {
        let src = "/// @verifies TEST-1\n#[test]\n#[ignore = \"needs the 2 GB model, CI job models\"]\nfn big() {}\n#[test]\n#[ignore]\nfn lazy() {}\n";
        assert!(testfreeze::ignored_with_reason(src, "big"));
        assert!(!testfreeze::ignored_with_reason(src, "lazy"));
        // every pattern shipped in the rulesets is a valid regex and owned by an existing rule
        for f in ["rust-web-enterprise.yaml", "rust-strict.yaml", "rust-service.yaml"] {
            let rf = rulesets::load_ruleset(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../doc/rules").join(f)).unwrap();
            let ids: std::collections::BTreeSet<&str> = rf.rules.iter().map(|r| r.id.as_str()).collect();
            for p in &rf.forbidden_patterns {
                assert!(regex::Regex::new(&p.pattern).is_ok(), "{}: {}", p.rule, p.pattern);
                assert!(ids.contains(p.rule.as_str()), "pattern owned by unknown rule {}", p.rule);
            }
        }
        let rf = rulesets::load_ruleset(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../doc/rules/rust-web-enterprise.yaml")).unwrap();
        let clock = regex::Regex::new(&rf.forbidden_patterns[0].pattern).unwrap();
        assert!(clock.is_match("let t = chrono::Utc::now();") && !clock.is_match("let t = clock.now();"));
        let secrets = regex::Regex::new(&rf.forbidden_patterns[2].pattern).unwrap();
        assert!(secrets.is_match("tracing::info!(user = %id, token = %t, \"login\");"));
        assert!(!secrets.is_match("tracing::info!(user = %id, \"login\");"));
    }
}
