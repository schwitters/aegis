use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
}
