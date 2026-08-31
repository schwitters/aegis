pub mod gate;
pub mod impact;
pub mod init;
pub mod issue;
pub mod profile;
pub mod traceability;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_id_parsing() {
        let text = "[REQ-001, REQ-002, UC-010, TEST-999]";
        let ids = traceability::parse_ids(text);
        assert_eq!(ids, vec!["REQ-001", "REQ-002", "UC-010", "TEST-999"]);
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
        assert_eq!(problems.len(), 3);
        assert!(problems.iter().any(|p| p.contains("GAP      REQ-002: no use case defined")));
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
}
