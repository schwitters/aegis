//! Review records: every blind review is persisted as `doc/reviews/REVIEW-XXXX.md` with its findings and their
//! disposition, so review results are auditable work products instead of transient chat output.

use anyhow::{bail, Context, Result};
use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const STAGES: [&str; 6] = ["spec", "requirements", "architecture", "usecases", "testplan", "implementation"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub severity: String, // low | medium | high | critical
    pub title: String,
    #[serde(default = "default_finding_status")]
    pub status: String, // open | accepted | rejected | deferred
    #[serde(default)]
    pub disposition: Option<String>,
    #[serde(default)]
    pub issue: Option<String>,
}

fn default_finding_status() -> String {
    "open".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewMeta {
    pub id: String,
    pub stage: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    pub reviewer: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub blind: bool,
    #[serde(default)]
    pub date: Option<String>,
    pub verdict: String, // pending | approved | approved_with_revisions | rejected
    #[serde(default)]
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub file_path: PathBuf,
    pub meta: ReviewMeta,
}

impl Review {
    pub fn open_findings(&self) -> usize {
        self.meta.findings.iter().filter(|f| f.status == "open").count()
    }
}

pub fn parse_review_file(path: &Path) -> Result<Review> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let content = content.replace("\r\n", "\n");
    let fm_re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*(\n|$)").unwrap();
    let cap = fm_re
        .captures(&content)
        .with_context(|| format!("No YAML frontmatter (--- ... ---) in {}", path.display()))?;
    let meta: ReviewMeta = serde_yaml::from_str(cap.get(1).map(|m| m.as_str()).unwrap_or(""))
        .with_context(|| format!("Invalid review frontmatter in {}", path.display()))?;
    Ok(Review {
        file_path: path.to_path_buf(),
        meta,
    })
}

pub fn collect_reviews(doc_dir: &Path) -> Result<Vec<Review>> {
    let dir = doc_dir.join("reviews");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|s| s.to_str()) == Some("md")
                && p.file_name().and_then(|s| s.to_str()) != Some("README.md")
        })
        .collect();
    files.sort();
    files.iter().map(|p| parse_review_file(p)).collect()
}

/// `required` entries are `<stage>` or `<stage>:<n>` (n distinct, independent reviewers with a non-rejecting verdict).
fn parse_required(entry: &str) -> Result<(String, usize)> {
    let (stage, n) = match entry.split_once(':') {
        Some((s, n)) => (s.trim(), n.trim().parse::<usize>().with_context(|| format!("bad count in `{}`", entry))?),
        None => (entry.trim(), 1),
    };
    if !STAGES.contains(&stage) {
        bail!("unknown review stage `{}` (allowed: {})", stage, STAGES.join(", "));
    }
    Ok((stage.to_string(), n))
}

/// Gate check over all review records. `issue_ids` are the existing ISSUE-IDs (deferred findings must point to one).
pub fn check(reviews: &[Review], issue_ids: &BTreeSet<String>, required: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = BTreeSet::new();

    for r in reviews {
        let m = &r.meta;
        let at = r.file_path.display();
        if !ids.insert(m.id.clone()) {
            problems.push(format!("DUPLICATE {}: review ID used twice ({})", m.id, at));
        }
        if !STAGES.contains(&m.stage.as_str()) {
            problems.push(format!("INVALID  {}: unknown stage `{}`", m.id, m.stage));
        }
        match m.verdict.as_str() {
            "approved" | "approved_with_revisions" | "rejected" => {}
            "pending" => problems.push(format!("PENDING  {}: review not completed (verdict: pending)", m.id)),
            other => problems.push(format!("INVALID  {}: unknown verdict `{}`", m.id, other)),
        }
        if m.author.as_deref().map(|a| a.eq_ignore_ascii_case(&m.reviewer)).unwrap_or(false) {
            problems.push(format!("INVALID  {}: reviewer `{}` is the author — not an independent review", m.id, m.reviewer));
        }
        let mut fids = BTreeSet::new();
        for f in &m.findings {
            let fid = format!("{}/{}", m.id, f.id);
            if !fids.insert(f.id.clone()) {
                problems.push(format!("DUPLICATE {}: finding ID used twice", fid));
            }
            let blocking = matches!(f.severity.as_str(), "high" | "critical");
            if !matches!(f.severity.as_str(), "low" | "medium" | "high" | "critical") {
                problems.push(format!("INVALID  {}: unknown severity `{}`", fid, f.severity));
            }
            let has_disposition = f.disposition.as_deref().map(|d| !d.trim().is_empty()).unwrap_or(false);
            match f.status.as_str() {
                "open" if blocking => {
                    problems.push(format!("OPEN     {} [{}]: {} — unresolved blocker", fid, f.severity, f.title))
                }
                "open" => {}
                "accepted" | "rejected" if !has_disposition => {
                    problems.push(format!("INVALID  {}: status `{}` requires a disposition (rationale)", fid, f.status))
                }
                "accepted" | "rejected" => {}
                "deferred" => match f.issue.as_deref() {
                    Some(i) if issue_ids.contains(i) => {}
                    Some(i) => problems.push(format!("ORPHAN   {}: deferred to {}, which does not exist", fid, i)),
                    None => problems.push(format!("INVALID  {}: deferred findings must reference an ISSUE-xxxx", fid)),
                },
                other => problems.push(format!("INVALID  {}: unknown status `{}`", fid, other)),
            }
        }
    }

    // The most recent review of each stage must not reject the artifact.
    let mut latest: BTreeMap<&str, &Review> = BTreeMap::new();
    for r in reviews {
        latest.insert(r.meta.stage.as_str(), r);
    }
    for (stage, r) in &latest {
        if r.meta.verdict == "rejected" {
            problems.push(format!("REJECTED {}: latest {} review rejects the artifact", r.meta.id, stage));
        }
    }

    for entry in required {
        match parse_required(entry) {
            Ok((stage, n)) => {
                let reviewers: BTreeSet<String> = reviews
                    .iter()
                    .filter(|r| r.meta.stage == stage && r.meta.blind)
                    .filter(|r| matches!(r.meta.verdict.as_str(), "approved" | "approved_with_revisions"))
                    .map(|r| r.meta.reviewer.to_lowercase())
                    .collect();
                if reviewers.len() < n {
                    problems.push(format!(
                        "MISSING  {} review: {} independent blind approval(s) required, {} found",
                        stage,
                        n,
                        reviewers.len()
                    ));
                }
            }
            Err(e) => problems.push(format!("INVALID  gate.reviews.required: {}", e)),
        }
    }
    problems
}

pub fn next_id(doc_dir: &Path) -> Result<String> {
    let id_re = Regex::new(r"^REVIEW-(\d+)").unwrap();
    let mut max = 0;
    let dir = doc_dir.join("reviews");
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let name = entry?.file_name().to_string_lossy().to_string();
            if let Some(n) = id_re.captures(&name).and_then(|c| c[1].parse::<u32>().ok()) {
                max = max.max(n);
            }
        }
    }
    Ok(format!("REVIEW-{:04}", max + 1))
}

pub struct CreateReviewOptions {
    pub stage: String,
    pub artifacts: Vec<String>,
    pub reviewer: String,
    pub author: Option<String>,
}

pub fn create_review(doc_dir: &Path, opts: CreateReviewOptions) -> Result<(String, PathBuf)> {
    if !STAGES.contains(&opts.stage.as_str()) {
        bail!("Unknown stage `{}` (allowed: {})", opts.stage, STAGES.join(", "));
    }
    let id = next_id(doc_dir)?;
    let dir = doc_dir.join("reviews");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.md", id));
    let meta = ReviewMeta {
        id: id.clone(),
        stage: opts.stage,
        artifacts: opts.artifacts,
        reviewer: opts.reviewer,
        author: opts.author,
        blind: true,
        date: Some(Local::now().format("%Y-%m-%d").to_string()),
        verdict: "pending".to_string(),
        findings: Vec::new(),
    };
    let content = format!(
        "---\n{}---\n\n\
         <!-- Paste the reviewer's full report below. Record every finding in `findings:` above with\n\
         \x20    status: open | accepted | rejected | deferred, a disposition (rationale) for accepted/rejected,\n\
         \x20    and issue: ISSUE-xxxx for deferred findings. Then set the verdict. -->\n\n## Review Report\n",
        serde_yaml::to_string(&meta)?
    );
    std::fs::write(&path, content)?;
    Ok((id, path))
}

pub fn render_table(reviews: &[Review]) -> String {
    let mut out = String::from("ID           STAGE           VERDICT                  REVIEWER         OPEN/TOTAL\n");
    for r in reviews {
        out.push_str(&format!(
            "{:<12} {:<15} {:<24} {:<16} {}/{}\n",
            r.meta.id,
            r.meta.stage,
            r.meta.verdict,
            r.meta.reviewer,
            r.open_findings(),
            r.meta.findings.len()
        ));
    }
    out
}
