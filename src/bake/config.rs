use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::error::{Result, SxmcError};

/// A baked connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeConfig {
    pub name: String,
    pub source_type: SourceType,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<PathBuf>,
    pub auth_headers: Vec<String>,
    pub env_vars: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Stdio,
    Http,
    Api,
    Spec,
    Graphql,
}

/// Store for baked configurations. Persists to ~/.config/sxmc/bakes.json
pub struct BakeStore {
    path: PathBuf,
    configs: HashMap<String, BakeConfig>,
}

impl BakeStore {
    /// Load the bake store from disk.
    pub fn load() -> Result<Self> {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("sxmc");

        std::fs::create_dir_all(&dir)
            .map_err(|e| SxmcError::Other(format!("Failed to create config dir: {}", e)))?;

        let path = dir.join("bakes.json");
        let configs = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| SxmcError::Other(format!("Failed to read bakes: {}", e)))?;
            serde_json::from_str(&content)?
        } else {
            HashMap::new()
        };

        Ok(Self { path, configs })
    }

    /// Save the store to disk.
    fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.configs)?;
        let parent = self.path.parent().ok_or_else(|| {
            SxmcError::Other(format!(
                "Failed to determine bake config directory for {}",
                self.path.display()
            ))
        })?;
        let mut temp = NamedTempFile::new_in(parent)
            .map_err(|e| SxmcError::Other(format!("Failed to create temp bake file: {}", e)))?;
        use std::io::Write;
        temp.write_all(json.as_bytes())
            .map_err(|e| SxmcError::Other(format!("Failed to write temp bake file: {}", e)))?;
        temp.flush()
            .map_err(|e| SxmcError::Other(format!("Failed to flush temp bake file: {}", e)))?;
        temp.persist(&self.path)
            .map_err(|e| SxmcError::Other(format!("Failed to persist bakes: {}", e)))?;
        Ok(())
    }

    /// Create a new baked config.
    pub fn create(&mut self, config: BakeConfig) -> Result<()> {
        if self.configs.contains_key(&config.name) {
            return Err(SxmcError::Other(format!(
                "Bake '{}' already exists. Use update or remove first.",
                config.name
            )));
        }
        self.configs.insert(config.name.clone(), config);
        self.save()
    }

    /// Update an existing baked config.
    pub fn update(&mut self, config: BakeConfig) -> Result<()> {
        if !self.configs.contains_key(&config.name) {
            return Err(SxmcError::Other(format!(
                "Bake '{}' not found",
                config.name
            )));
        }
        self.configs.insert(config.name.clone(), config);
        self.save()
    }

    /// Remove a baked config.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if self.configs.remove(name).is_none() {
            return Err(SxmcError::Other(format!("Bake '{}' not found", name)));
        }
        self.save()
    }

    /// Get a baked config by name.
    pub fn get(&self, name: &str) -> Option<&BakeConfig> {
        self.configs.get(name)
    }

    /// List all baked configs.
    pub fn list(&self) -> Vec<&BakeConfig> {
        let mut configs: Vec<_> = self.configs.values().collect();
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        configs
    }

    /// Show details for a baked config.
    pub fn show(&self, name: &str) -> Option<&BakeConfig> {
        self.configs.get(name)
    }
}

impl std::fmt::Display for BakeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?}: {})", self.name, self.source_type, self.source)?;
        if let Some(timeout) = self.timeout_seconds {
            write!(f, " [timeout={}s]", timeout)?;
        }
        if let Some(ref base_dir) = self.base_dir {
            write!(f, " [base-dir={}]", base_dir.display())?;
        }
        if let Some(ref desc) = self.description {
            write!(f, " — {}", desc)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_persists_valid_json() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bakes.json");
        let mut configs = HashMap::new();
        configs.insert(
            "demo".into(),
            BakeConfig {
                name: "demo".into(),
                source_type: SourceType::Stdio,
                source: "sxmc serve".into(),
                base_dir: Some(PathBuf::from("/tmp/demo")),
                auth_headers: Vec::new(),
                env_vars: Vec::new(),
                timeout_seconds: Some(15),
                description: Some("demo".into()),
            },
        );

        let store = BakeStore { path, configs };
        store.save().unwrap();

        let written = std::fs::read_to_string(&store.path).unwrap();
        let parsed: HashMap<String, BakeConfig> = serde_json::from_str(&written).unwrap();
        assert!(parsed.contains_key("demo"));
        assert_eq!(parsed["demo"].timeout_seconds, Some(15));
        assert_eq!(parsed["demo"].base_dir, Some(PathBuf::from("/tmp/demo")));
    }
}
