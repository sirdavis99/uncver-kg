use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub storage: StorageConfig,
    pub llm: LLMConfig,
    pub agents: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub base_path: PathBuf,
    pub auto_save: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let base_path = directories::ProjectDirs::from("com", "kg-core", "kg-core")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data"));

        Self {
            base_path,
            auto_save: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: Some("gemma2:2b".to_string()),
            base_url: Some("http://localhost:11434".to_string()),
            api_key: None,
            temperature: 0.7,
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub enable_researcher: bool,
    pub enable_actor: bool,
    pub enable_reviewer: bool,
    pub async_review: bool,
    pub confidence_thresholds: ConfidenceThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceThresholds {
    pub draft_max: u8,
    pub unverified_max: u8,
    pub gold_min: u8,
}

impl Default for ConfidenceThresholds {
    fn default() -> Self {
        Self {
            draft_max: 50,
            unverified_max: 80,
            gold_min: 80,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enable_researcher: true,
            enable_actor: true,
            enable_reviewer: true,
            async_review: true,
            confidence_thresholds: ConfidenceThresholds::default(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            llm: LLMConfig::default(),
            agents: AgentConfig::default(),
        }
    }
}

impl Config {
    pub fn from_file(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = if path
                .extension()
                .map_or(false, |e| e == "yaml" || e == "yml")
            {
                serde_yaml::from_str(&content)?
            } else {
                serde_json::from_str(&content)?
            };
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self, path: impl Into<PathBuf>) -> anyhow::Result<()> {
        let path = path.into();
        let content = if path
            .extension()
            .map_or(false, |e| e == "yaml" || e == "yml")
        {
            serde_yaml::to_string(&self)?
        } else {
            serde_json::to_string_pretty(&self)?
        };
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        directories::ProjectDirs::from("com", "kg-core", "kg-core")
            .map(|dirs| dirs.config_dir().join("config.json"))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }
}
