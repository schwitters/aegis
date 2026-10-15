use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub profile: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub rigor: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub rulesets: Vec<String>,
    #[serde(default)]
    pub quality_axes: HashMap<String, String>,
    #[serde(default)]
    pub gate: HashMap<String, GateStageConfig>,
    #[serde(default)]
    pub deliverables: Vec<String>,
    #[serde(default)]
    pub coding_constraints: Vec<String>,
    #[serde(default)]
    pub agent_instructions: Option<String>,
    #[serde(default)]
    pub agent_loop: Option<AgentLoopConfig>,
}

/// Inner implementation loop of the implementation model (see doc/prompts/06_27b_implementation.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoopConfig {
    /// Gate attempts per task before the model must stop and escalate
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    /// Gate stages the model runs inside its loop (`aegis gate --only ...`)
    #[serde(default = "default_fast_stages")]
    pub fast_stages: Vec<String>,
}

fn default_max_iterations() -> u32 {
    3
}

fn default_fast_stages() -> Vec<String> {
    ["build", "lint", "test_freeze", "tests"].iter().map(|s| s.to_string()).collect()
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            fast_stages: default_fast_stages(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateStageConfig {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub standard: Option<String>,
    #[serde(default)]
    pub fail_on: Option<String>,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub threshold: Option<u32>,
    #[serde(default)]
    pub fail_below: Option<bool>,
    #[serde(default)]
    pub bidirectional: Option<bool>,
    /// Replaces the built-in check of an external stage; exit code 0 = pass
    #[serde(default)]
    pub command: Option<String>,
    /// Source roots scanned for annotations (traceability, test_freeze); default: [src, tests]
    #[serde(default)]
    pub paths: Vec<String>,
    /// traceability: `required` fails the gate without an approved doc/baseline.lock
    #[serde(default)]
    pub baseline: Option<String>,
    /// traceability: `required` fails the gate without handbooks under docs/ (UC documentation coverage)
    #[serde(default)]
    pub documentation: Option<String>,
    /// reviews: required blind approvals per stage, e.g. [spec, requirements, "architecture:2"]
    #[serde(default)]
    pub required: Vec<String>,
    /// architecture: layer -> layers it may depend on (`*` = any); crates match by `-<layer>` name suffix
    #[serde(default)]
    pub layers: std::collections::BTreeMap<String, Vec<String>>,
    /// architecture: third-party crate -> layers that may depend on it directly
    #[serde(default)]
    pub restricted: std::collections::BTreeMap<String, Vec<String>>,
}

impl Profile {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Profile not found: {}", path.as_ref().display()))?;
        let profile: Profile = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse profile YAML: {}", path.as_ref().display()))?;
        Ok(profile)
    }

    pub fn is_stage_active(&self, stage_name: &str) -> bool {
        self.gate.get(stage_name).map(|s| s.active).unwrap_or(false)
    }

    pub fn get_stage_config(&self, stage_name: &str) -> Option<&GateStageConfig> {
        self.gate.get(stage_name)
    }

    /// Source roots for annotation scans: `gate.traceability.paths`, else `src` and `tests`.
    pub fn source_paths(&self, root: &Path) -> Vec<PathBuf> {
        let configured = self.get_stage_config("traceability").map(|c| c.paths.clone()).unwrap_or_default();
        let rels = if configured.is_empty() { vec!["src".to_string(), "tests".to_string()] } else { configured };
        rels.iter().map(|p| root.join(p)).collect()
    }

    pub fn agent_loop(&self) -> AgentLoopConfig {
        self.agent_loop.clone().unwrap_or_default()
    }
}
