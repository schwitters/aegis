use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::profile::Profile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    pub instruction: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub linter_check: Option<String>,
    #[serde(default)]
    pub legacy_id: Option<String>,
    #[serde(default)]
    pub enforcement: Option<String>,
    #[serde(default)]
    pub deviation: Option<String>,
    #[serde(default)]
    pub example_bad: Option<String>,
    #[serde(default)]
    pub example_good: Option<String>,
}

/// v1 linter block (`linter_config:` in rust-safety.yaml and older rulesets).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinterConfig {
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub config_file: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// One tool entry of a v2 `toolchain:` block. Unknown keys are ignored so every language can add its own.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolConfig {
    #[serde(default)]
    pub config_file: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// The parts of a v2 `toolchain:` block that `aegis gate` executes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Toolchain {
    #[serde(default)]
    pub clippy: Option<ToolConfig>,
    /// Extra shell commands per gate stage key (`lint`, `static_analysis`, `security`, `tests`, ...), appended to the
    /// stage's built-in commands. A command whose tool is missing makes the stage not executable (fail closed).
    #[serde(default)]
    pub gate_commands: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviationPolicy {
    /// e.g. `// AEGIS-DEVIATION(<RULE-ID>): <justification>`
    #[serde(default)]
    pub marker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesetFile {
    pub ruleset: String,
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub standard: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub linter_config: Option<LinterConfig>,
    #[serde(default)]
    pub toolchain: Option<Toolchain>,
    #[serde(default)]
    pub deviation_policy: Option<DeviationPolicy>,
    #[serde(default)]
    pub rules: Vec<RuleItem>,
}

impl RulesetFile {
    /// The deviation marker for one rule, as the implementation model must write it.
    pub fn marker_for(&self, rule_id: &str) -> String {
        let template = self
            .deviation_policy
            .as_ref()
            .and_then(|p| p.marker.clone())
            .unwrap_or_else(|| default_marker(self.language.as_deref()));
        template.replace("<RULE-ID>", rule_id)
    }

    /// Extra clippy flags declared by the ruleset (v2 `toolchain.clippy.flags`, else v1 `linter_config.flags`).
    pub fn clippy_flags(&self) -> Vec<String> {
        if let Some(flags) = self.toolchain.as_ref().and_then(|t| t.clippy.as_ref()).map(|c| c.flags.clone()) {
            return flags;
        }
        match &self.linter_config {
            Some(cfg) if cfg.tool.as_deref() == Some("clippy") => cfg.flags.clone(),
            _ => Vec::new(),
        }
    }

    /// Extra gate commands for a stage key.
    pub fn gate_commands(&self, stage: &str) -> Vec<String> {
        self.toolchain
            .as_ref()
            .and_then(|t| t.gate_commands.get(stage).cloned())
            .unwrap_or_default()
    }
}

/// Comment syntax of the default deviation marker when a ruleset does not declare one.
pub fn default_marker(language: Option<&str>) -> String {
    match language.unwrap_or("") {
        "c" | "c11" => "/* AEGIS-DEVIATION(<RULE-ID>): <justification> */".to_string(),
        "python" => "# AEGIS-DEVIATION(<RULE-ID>): <justification>".to_string(),
        _ => "// AEGIS-DEVIATION(<RULE-ID>): <justification>".to_string(),
    }
}

/// Markdown fence language for code examples of a ruleset.
pub fn fence_language(language: Option<&str>) -> &'static str {
    match language.unwrap_or("") {
        "c" | "c11" => "c",
        "cpp" | "cpp20" | "c++" => "cpp",
        "rust" => "rust",
        "java" => "java",
        "python" => "python",
        _ => "text",
    }
}

/// Only the rulesets a profile references explicitly (what `aegis gate` executes). Missing files are errors.
pub fn load_referenced(root: &Path, profile: &Profile) -> Result<Vec<RulesetFile>> {
    profile
        .rulesets
        .iter()
        .map(|rel| {
            let full = root.join(rel);
            if !full.exists() {
                anyhow::bail!("Profile {} references missing ruleset {}", profile.profile, rel);
            }
            load_ruleset(&full)
        })
        .collect()
}

/// Parses one ruleset file. Fail closed: unreadable or unparsable rulesets are errors, never skipped.
pub fn load_ruleset(path: &Path) -> Result<RulesetFile> {
    let content = fs::read_to_string(path).with_context(|| format!("Failed to read ruleset {}", path.display()))?;
    serde_yaml::from_str::<RulesetFile>(&content).with_context(|| format!("Failed to parse ruleset {}", path.display()))
}

/// The rulesets a profile references (paths relative to `root`). A referenced file that does not exist is an error.
/// Without a profile (or with an empty `rulesets:` list) every `*.yaml` under `doc/rules/` is loaded, as
/// `aegis instructions` always did.
pub fn load_for_profile(root: &Path, profile: Option<&Profile>) -> Result<Vec<(PathBuf, RulesetFile)>> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Some(prof) = profile {
        for rel in &prof.rulesets {
            let full = root.join(rel);
            if !full.exists() {
                anyhow::bail!("Profile {} references missing ruleset {}", prof.profile, rel);
            }
            files.push(full);
        }
    }
    if files.is_empty() {
        let rules_dir = root.join("doc").join("rules");
        if rules_dir.exists() {
            for entry in WalkDir::new(&rules_dir).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                let ext = p.extension().and_then(|s| s.to_str());
                if p.is_file() && (ext == Some("yaml") || ext == Some("yml")) {
                    files.push(p.to_path_buf());
                }
            }
            files.sort();
        }
    }
    files.into_iter().map(|p| load_ruleset(&p).map(|rf| (p, rf))).collect()
}
