use anyhow::{bail, Context, Result};
use chrono::Local;
use colored::Colorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueFrontmatter {
    pub id: String,
    pub title: String,
    #[serde(default = "default_status")]
    pub status: String, // open | in_progress | done | reopened
    #[serde(default = "default_type")]
    pub r#type: String, // bug | task | improvement | question | risk
    #[serde(default = "default_severity")]
    pub severity: String, // low | medium | high | critical
    #[serde(default)]
    pub source_doc: Option<String>,
    #[serde(default)]
    pub source_finding: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub closed: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub related: Vec<String>,
}

fn default_status() -> String {
    "open".to_string()
}
fn default_type() -> String {
    "bug".to_string()
}
fn default_severity() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub file_path: PathBuf,
    pub meta: IssueFrontmatter,
    #[serde(skip_serializing)]
    pub body: String,
}

pub fn parse_issue_file(path: &Path) -> Result<Issue> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Konnte Issue nicht lesen: {}", path.display()))?;

    let fm_re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$").unwrap();
    if let Some(cap) = fm_re.captures(&content) {
        let yaml_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();

        let meta: IssueFrontmatter = serde_yaml::from_str(yaml_str)
            .with_context(|| format!("Ungültige YAML-Frontmatter in {}", path.display()))?;

        Ok(Issue {
            file_path: path.to_path_buf(),
            meta,
            body,
        })
    } else {
        bail!("Keine YAML-Frontmatter (--- ... ---) in {} gefunden", path.display());
    }
}

pub fn collect_issues(doc_dir: &Path) -> Result<Vec<Issue>> {
    let issues_dir = doc_dir.join("issues");
    if !issues_dir.exists() {
        return Ok(Vec::new());
    }

    let mut issues = Vec::new();
    for entry in WalkDir::new(&issues_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            if let Ok(issue) = parse_issue_file(path) {
                issues.push(issue);
            }
        }
    }

    issues.sort_by(|a, b| a.meta.id.cmp(&b.meta.id));
    Ok(issues)
}

pub fn get_next_issue_id(issues: &[Issue]) -> String {
    let re = Regex::new(r"ISSUE-(\d+)").unwrap();
    let mut max_id = 0;

    for issue in issues {
        if let Some(cap) = re.captures(&issue.meta.id) {
            if let Some(num_str) = cap.get(1) {
                if let Ok(num) = num_str.as_str().parse::<u32>() {
                    if num > max_id {
                        max_id = num;
                    }
                }
            }
        }
    }

    format!("ISSUE-{:04}", max_id + 1)
}

pub struct CreateIssueOptions {
    pub title: String,
    pub r#type: String,
    pub severity: String,
    pub stage: Option<String>,
    pub source_doc: Option<String>,
    pub source_finding: Option<String>,
    pub assignee: Option<String>,
    pub related: Vec<String>,
    pub context: Option<String>,
    pub description: Option<String>,
}

pub fn create_issue(doc_dir: &Path, opts: CreateIssueOptions) -> Result<(String, PathBuf)> {
    let issues_dir = doc_dir.join("issues");
    std::fs::create_dir_all(&issues_dir)?;

    let existing = collect_issues(doc_dir)?;
    let id = get_next_issue_id(&existing);
    let filename = format!("{}.md", id);
    let target_file = issues_dir.join(&filename);

    let today = Local::now().format("%Y-%m-%d").to_string();

    let meta = IssueFrontmatter {
        id: id.clone(),
        title: opts.title.clone(),
        status: "open".to_string(),
        r#type: opts.r#type,
        severity: opts.severity,
        source_doc: opts.source_doc,
        source_finding: opts.source_finding,
        stage: opts.stage.or_else(|| Some("implementation".to_string())),
        created: Some(today.clone()),
        updated: Some(today.clone()),
        closed: None,
        assignee: opts.assignee,
        related: opts.related,
    };

    let yaml_str = serde_yaml::to_string(&meta)?;

    let context_text = opts.context.unwrap_or_else(|| "Derived automatically from observation / review.".to_string());
    let desc_text = opts.description.unwrap_or_else(|| opts.title.clone());

    let full_content = format!(
        "---\n{}---\n\n## Context\n\n{}\n\n## Description\n\n{}\n\n## Acceptance Criteria\n\n- [ ] Root cause resolved / requirement fulfilled\n- [ ] Tests added/updated and verified\n- [ ] Execution gate passes\n\n## History\n\n- {}: created\n",
        yaml_str, context_text, desc_text, today
    );

    std::fs::write(&target_file, full_content)?;
    Ok((id, target_file))
}

pub fn close_issue(doc_dir: &Path, issue_id: &str) -> Result<PathBuf> {
    let issues = collect_issues(doc_dir)?;
    let target_id = if issue_id.starts_with("ISSUE-") {
        issue_id.to_string()
    } else if let Ok(num) = issue_id.parse::<u32>() {
        format!("ISSUE-{:04}", num)
    } else {
        issue_id.to_string()
    };

    let issue = issues
        .iter()
        .find(|i| i.meta.id == target_id)
        .with_context(|| format!("Issue {} not found", target_id))?;

    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut meta = issue.meta.clone();
    meta.status = "done".to_string();
    meta.closed = Some(today.clone());
    meta.updated = Some(today.clone());

    let yaml_str = serde_yaml::to_string(&meta)?;

    let mut body = issue.body.clone();
    if body.contains("## History") || body.contains("## Verlauf") {
        body.push_str(&format!("- {}: closed (`status: done`)\n", today));
    }

    let full_content = format!("---\n{}---\n{}", yaml_str, body);
    std::fs::write(&issue.file_path, full_content)?;

    Ok(issue.file_path.clone())
}

pub fn render_issues_table(issues: &[Issue]) -> String {
    if issues.is_empty() {
        return "No issues found in doc/issues/.\n".to_string();
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "{:<12} {:<10} {:<10} {:<12} {:<16} {:<30} {:<15}",
        "ID", "STATUS", "SEVERITY", "TYPE", "STAGE", "TITLE", "RELATED"
    ));
    lines.push("-".repeat(110));

    for issue in issues {
        let id_colored = issue.meta.id.bold().to_string();
        let status_colored = match issue.meta.status.as_str() {
            "open" => "open".red().to_string(),
            "in_progress" => "in_progress".yellow().to_string(),
            "done" => "done".green().to_string(),
            _ => issue.meta.status.clone(),
        };
        let sev_colored = match issue.meta.severity.as_str() {
            "critical" => "critical".red().bold().to_string(),
            "high" => "high".red().to_string(),
            "medium" => "medium".yellow().to_string(),
            "low" => "low".cyan().to_string(),
            _ => issue.meta.severity.clone(),
        };

        let title_truncated = if issue.meta.title.len() > 28 {
            format!("{}...", &issue.meta.title[..25])
        } else {
            issue.meta.title.clone()
        };

        let related_str = if issue.meta.related.is_empty() {
            "—".to_string()
        } else {
            issue.meta.related.join(", ")
        };

        let stage_str = issue.meta.stage.as_deref().unwrap_or("—");

        lines.push(format!(
            "{:<12} {:<10} {:<10} {:<12} {:<16} {:<30} {:<15}",
            id_colored,
            status_colored,
            sev_colored,
            issue.meta.r#type,
            stage_str,
            title_truncated,
            related_str
        ));
    }

    lines.join("\n") + "\n"
}
