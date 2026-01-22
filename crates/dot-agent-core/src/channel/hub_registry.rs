//! Hub registry management
//!
//! Manages the list of registered Hubs in ~/.dot-agent/hubs.toml

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};

use super::types::Hub;

/// Hub registry configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HubRegistry {
    /// Registered hubs
    pub hubs: Vec<Hub>,
}

impl HubRegistry {
    const FILENAME: &'static str = "hubs.toml";

    /// Load hub registry from base directory
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(Self::FILENAME);
        if !path.exists() {
            // Return default with official hub
            return Ok(Self::with_official());
        }

        let content = fs::read_to_string(&path)?;
        let mut registry: Self =
            toml::from_str(&content).map_err(|e| DotAgentError::ConfigParseSimple {
                message: e.to_string(),
            })?;

        // Ensure official hub is always present
        if !registry.hubs.iter().any(|h| h.is_default) {
            registry.hubs.insert(0, Hub::official());
        }

        Ok(registry)
    }

    /// Create registry with official hub
    pub fn with_official() -> Self {
        Self {
            hubs: vec![Hub::official()],
        }
    }

    /// Save hub registry to base directory
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(Self::FILENAME);
        fs::create_dir_all(base_dir)?;
        let content =
            toml::to_string_pretty(self).map_err(|e| DotAgentError::ConfigParseSimple {
                message: e.to_string(),
            })?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Add a hub
    pub fn add(&mut self, hub: Hub) -> Result<()> {
        if self.hubs.iter().any(|h| h.name == hub.name) {
            return Err(DotAgentError::HubAlreadyExists { name: hub.name });
        }
        self.hubs.push(hub);
        Ok(())
    }

    /// Remove a hub by name (cannot remove default hub)
    pub fn remove(&mut self, name: &str) -> Result<Hub> {
        let idx = self
            .hubs
            .iter()
            .position(|h| h.name == name)
            .ok_or_else(|| DotAgentError::HubNotFound {
                name: name.to_string(),
            })?;

        if self.hubs[idx].is_default {
            return Err(DotAgentError::CannotRemoveDefaultHub);
        }

        Ok(self.hubs.remove(idx))
    }

    /// Get a hub by name
    pub fn get(&self, name: &str) -> Option<&Hub> {
        self.hubs.iter().find(|h| h.name == name)
    }

    /// Get the default hub
    pub fn default_hub(&self) -> Option<&Hub> {
        self.hubs.iter().find(|h| h.is_default)
    }

    /// List all hubs
    pub fn list(&self) -> &[Hub] {
        &self.hubs
    }

    /// Get cache directory for a hub
    pub fn cache_dir(base_dir: &Path, hub_name: &str) -> PathBuf {
        base_dir.join("cache").join("hubs").join(hub_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn registry_with_official() {
        let registry = HubRegistry::with_official();
        assert_eq!(registry.hubs.len(), 1);
        assert!(registry.hubs[0].is_default);
        assert_eq!(registry.hubs[0].name, "official");
    }

    #[test]
    fn registry_add_remove() {
        let mut registry = HubRegistry::with_official();

        let hub = Hub::new("company", "https://github.com/company/hub");
        registry.add(hub).unwrap();
        assert_eq!(registry.hubs.len(), 2);

        // Duplicate should fail
        let hub2 = Hub::new("company", "https://github.com/other/hub");
        assert!(registry.add(hub2).is_err());

        // Remove
        let removed = registry.remove("company").unwrap();
        assert_eq!(removed.name, "company");
        assert_eq!(registry.hubs.len(), 1);

        // Cannot remove default
        assert!(registry.remove("official").is_err());
    }

    #[test]
    fn registry_save_load() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        let mut registry = HubRegistry::with_official();
        registry
            .add(Hub::new("test", "https://github.com/test/hub"))
            .unwrap();
        registry.save(base).unwrap();

        let loaded = HubRegistry::load(base).unwrap();
        assert_eq!(loaded.hubs.len(), 2);
        assert!(loaded.get("official").is_some());
        assert!(loaded.get("test").is_some());
    }
}
