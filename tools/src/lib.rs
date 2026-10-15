pub mod baseline;
pub mod gate;
pub mod impact;
pub mod init;
pub mod instructions;
pub mod issue;
pub mod profile;
pub mod review;
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
}
