//! Plugin Installer
//!
//! High-level API for installing and managing Claude Code plugins

use std::path::PathBuf;

use crate::error::{DotAgentError, Result};
use crate::plugin::fetcher::PluginFetcher;
use crate::plugin::marketplace::{get_plugin, parse_marketplace};
use crate::plugin::registry::PluginRegistry;
use crate::plugin::types::{InstallScope, InstalledPlugin, Marketplace, ResolvedPlugin};

/// Plugin Installer - high-level API for plugin management
pub struct PluginInstaller {
    registry: PluginRegistry,
    fetcher: PluginFetcher,
}

impl PluginInstaller {
    /// Create a new PluginInstaller
    pub fn new() -> Result<Self> {
        let registry = PluginRegistry::new()?;
        let fetcher = PluginFetcher::new(registry.plugins_dir());

        Ok(Self { registry, fetcher })
    }

    /// Create with custom plugins directory (for testing)
    pub fn with_dir(plugins_dir: PathBuf) -> Self {
        let registry = PluginRegistry::with_dir(plugins_dir.clone());
        let fetcher = PluginFetcher::new(&plugins_dir);

        Self { registry, fetcher }
    }

    /// Get the registry
    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    /// Get the fetcher
    pub fn fetcher(&self) -> &PluginFetcher {
        &self.fetcher
    }

    // ========== Marketplace Management ==========

    /// Add a marketplace from GitHub
    pub fn add_marketplace_github(&self, name: &str, repo: &str) -> Result<PathBuf> {
        // Register in known_marketplaces.json
        self.registry.add_marketplace_github(name, repo)?;

        // Fetch/clone the marketplace
        let path = self.fetcher.fetch_marketplace_github(name, repo)?;

        Ok(path)
    }

    /// Add a marketplace from URL
    pub fn add_marketplace_url(&self, name: &str, url: &str) -> Result<PathBuf> {
        // Register in known_marketplaces.json
        self.registry.add_marketplace_url(name, url)?;

        // Fetch/clone the marketplace
        let path = self.fetcher.fetch_marketplace_url(name, url)?;

        Ok(path)
    }

    /// Add a marketplace from local path
    pub fn add_marketplace_local(&self, name: &str, path: &str) -> Result<PathBuf> {
        // Register in known_marketplaces.json
        self.registry.add_marketplace_local(name, path)?;

        // Link the marketplace
        let target = self.fetcher.fetch_marketplace_local(name, path)?;

        Ok(target)
    }

    /// Remove a marketplace
    pub fn remove_marketplace(&self, name: &str) -> Result<()> {
        self.registry.remove_marketplace(name)
    }

    /// Update a marketplace (re-fetch)
    pub fn update_marketplace(&self, name: &str) -> Result<PathBuf> {
        let marketplace = self.registry.get_marketplace(name)?.ok_or_else(|| {
            DotAgentError::MarketplaceNotFound {
                name: name.to_string(),
            }
        })?;

        let path = match marketplace.source.source.as_str() {
            "github" => {
                if let Some(repo) = &marketplace.source.repo {
                    self.fetcher.fetch_marketplace_github(name, repo)?
                } else {
                    return Err(DotAgentError::ConfigParseSimple {
                        message: "GitHub marketplace missing repo".to_string(),
                    });
                }
            }
            "url" => {
                if let Some(url) = &marketplace.source.url {
                    self.fetcher.fetch_marketplace_url(name, url)?
                } else {
                    return Err(DotAgentError::ConfigParseSimple {
                        message: "URL marketplace missing url".to_string(),
                    });
                }
            }
            "local" => {
                if let Some(local_path) = &marketplace.source.path {
                    self.fetcher.fetch_marketplace_local(name, local_path)?
                } else {
                    return Err(DotAgentError::ConfigParseSimple {
                        message: "Local marketplace missing path".to_string(),
                    });
                }
            }
            _ => {
                return Err(DotAgentError::ConfigParseSimple {
                    message: format!("Unknown marketplace source: {}", marketplace.source.source),
                });
            }
        };

        Ok(path)
    }

    /// List all marketplaces
    pub fn list_marketplaces(&self) -> Result<Vec<(String, PathBuf)>> {
        let marketplaces = self.registry.list_marketplaces()?;

        Ok(marketplaces
            .into_iter()
            .map(|(name, km)| (name, PathBuf::from(km.install_location)))
            .collect())
    }

    /// Load a marketplace
    pub fn load_marketplace(&self, name: &str) -> Result<Marketplace> {
        let km = self.registry.get_marketplace(name)?.ok_or_else(|| {
            DotAgentError::MarketplaceNotFound {
                name: name.to_string(),
            }
        })?;

        let marketplace_dir = PathBuf::from(&km.install_location);
        parse_marketplace(&marketplace_dir)
    }

    // ========== Plugin Management ==========

    /// Install a plugin
    pub fn install_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
        scope: InstallScope,
    ) -> Result<InstalledPlugin> {
        // Load marketplace
        let marketplace = self.load_marketplace(marketplace_name)?;
        let marketplace_dir = self.fetcher.marketplaces_dir().join(marketplace_name);

        // Find plugin
        let plugin_entry = marketplace
            .plugins
            .iter()
            .find(|p| p.name == plugin_name)
            .ok_or_else(|| DotAgentError::PluginNotFound {
                name: plugin_name.to_string(),
            })?;

        // Determine version
        let version = plugin_entry.version.clone().unwrap_or_else(|| {
            // Generate version from timestamp or use "latest"
            chrono::Utc::now().format("%Y%m%d%H%M%S").to_string()
        });

        // Fetch plugin
        let install_path =
            self.fetcher
                .fetch_plugin(&marketplace, &marketplace_dir, plugin_entry, &version)?;

        // Register in installed_plugins.json
        let installed = self.registry.add_installed_plugin(
            plugin_name,
            marketplace_name,
            &install_path,
            &version,
            scope,
        )?;

        Ok(installed)
    }

    /// Uninstall a plugin
    pub fn uninstall_plugin(&self, plugin_name: &str, marketplace_name: &str) -> Result<()> {
        self.registry
            .remove_installed_plugin(plugin_name, marketplace_name)
    }

    /// Check if a plugin is installed
    pub fn is_plugin_installed(&self, plugin_name: &str, marketplace_name: &str) -> Result<bool> {
        self.registry
            .is_plugin_installed(plugin_name, marketplace_name)
    }

    /// Get installed plugin info
    pub fn get_installed_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
    ) -> Result<Option<InstalledPlugin>> {
        self.registry
            .get_installed_plugin(plugin_name, marketplace_name)
    }

    /// List all installed plugins
    pub fn list_installed_plugins(&self) -> Result<Vec<(String, InstalledPlugin)>> {
        self.registry.list_installed_plugins()
    }

    // ========== Search ==========

    /// Search plugins across all marketplaces
    pub fn search_plugins(&self, query: &str) -> Result<Vec<ResolvedPlugin>> {
        let marketplaces = self.registry.list_marketplaces()?;
        let mut results = Vec::new();

        for (name, km) in marketplaces {
            let marketplace_dir = PathBuf::from(&km.install_location);

            // Skip if marketplace dir doesn't exist
            if !marketplace_dir.exists() {
                continue;
            }

            // Try to load marketplace
            if let Ok(marketplace) = parse_marketplace(&marketplace_dir) {
                let plugins = crate::plugin::marketplace::search_plugins(&marketplace, query);

                // Enrich with installation status
                for mut plugin in plugins {
                    if let Ok(Some(installed)) =
                        self.registry.get_installed_plugin(&plugin.name, &name)
                    {
                        plugin.installed = Some(installed);
                        plugin.cache_path =
                            Some(self.fetcher.cache_dir().join(&name).join(&plugin.name));
                    }
                    results.push(plugin);
                }
            }
        }

        Ok(results)
    }

    /// List all plugins from a marketplace
    pub fn list_marketplace_plugins(&self, marketplace_name: &str) -> Result<Vec<ResolvedPlugin>> {
        let marketplace = self.load_marketplace(marketplace_name)?;
        let plugins = crate::plugin::marketplace::list_plugins(&marketplace);

        // Enrich with installation status
        let mut results = Vec::new();
        for mut plugin in plugins {
            if let Ok(Some(installed)) = self
                .registry
                .get_installed_plugin(&plugin.name, marketplace_name)
            {
                plugin.installed = Some(installed);
                plugin.cache_path = Some(
                    self.fetcher
                        .cache_dir()
                        .join(marketplace_name)
                        .join(&plugin.name),
                );
            }
            results.push(plugin);
        }

        Ok(results)
    }

    /// Get a specific plugin from a marketplace
    pub fn get_plugin(
        &self,
        plugin_name: &str,
        marketplace_name: &str,
    ) -> Result<Option<ResolvedPlugin>> {
        let marketplace = self.load_marketplace(marketplace_name)?;

        if let Some(mut plugin) = get_plugin(&marketplace, plugin_name) {
            if let Ok(Some(installed)) = self
                .registry
                .get_installed_plugin(plugin_name, marketplace_name)
            {
                plugin.installed = Some(installed);
                plugin.cache_path = Some(
                    self.fetcher
                        .cache_dir()
                        .join(marketplace_name)
                        .join(plugin_name),
                );
            }
            Ok(Some(plugin))
        } else {
            Ok(None)
        }
    }
}

impl Default for PluginInstaller {
    fn default() -> Self {
        Self::new().expect("Failed to create PluginInstaller")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_installer() -> (PluginInstaller, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let installer = PluginInstaller::with_dir(temp_dir.path().to_path_buf());
        (installer, temp_dir)
    }

    fn create_mock_marketplace(temp: &TempDir, name: &str) -> PathBuf {
        // Use a separate "sources" directory to avoid conflict with symlinks
        let marketplace_dir = temp.path().join("sources").join(name);
        fs::create_dir_all(marketplace_dir.join(".claude-plugin")).unwrap();

        let marketplace_json = format!(
            r#"{{
            "name": "{}",
            "owner": {{ "name": "Test Owner" }},
            "plugins": [
                {{
                    "name": "test-plugin",
                    "source": "./plugins/test-plugin",
                    "description": "A test plugin",
                    "version": "1.0.0"
                }}
            ]
        }}"#,
            name
        );

        fs::write(
            marketplace_dir.join(".claude-plugin/marketplace.json"),
            marketplace_json,
        )
        .unwrap();

        // Create plugin directory
        let plugin_dir = marketplace_dir.join("plugins/test-plugin/.claude-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join("plugin.json"),
            r#"{"name": "test-plugin", "version": "1.0.0"}"#,
        )
        .unwrap();

        marketplace_dir
    }

    #[test]
    fn test_add_marketplace_local() {
        let (installer, temp) = create_test_installer();
        let source_dir = create_mock_marketplace(&temp, "source-market");

        // Manually add to known_marketplaces
        installer
            .registry
            .add_marketplace_local("test-market", source_dir.to_str().unwrap())
            .unwrap();

        let marketplaces = installer.list_marketplaces().unwrap();
        assert_eq!(marketplaces.len(), 1);
        assert_eq!(marketplaces[0].0, "test-market");
    }

    #[test]
    fn test_load_marketplace() {
        let (installer, temp) = create_test_installer();
        let source_dir = create_mock_marketplace(&temp, "test-market");

        // Add marketplace
        installer
            .registry
            .add_marketplace_local("test-market", source_dir.to_str().unwrap())
            .unwrap();

        // Create symlink manually for test
        let target = temp.path().join("marketplaces/test-market");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_dir, &target).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&source_dir, &target).unwrap();

        // Load marketplace
        let marketplace = installer.load_marketplace("test-market").unwrap();
        assert_eq!(marketplace.name, "test-market");
        assert_eq!(marketplace.plugins.len(), 1);
    }

    #[test]
    fn test_list_marketplace_plugins() {
        let (installer, temp) = create_test_installer();
        let source_dir = create_mock_marketplace(&temp, "test-market");

        // Add marketplace
        installer
            .registry
            .add_marketplace_local("test-market", source_dir.to_str().unwrap())
            .unwrap();

        // Create symlink
        let target = temp.path().join("marketplaces/test-market");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_dir, &target).unwrap();

        // List plugins
        let plugins = installer.list_marketplace_plugins("test-market").unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "test-plugin");
        assert_eq!(plugins[0].full_id, "test-plugin@test-market");
    }

    #[test]
    fn test_search_plugins() {
        let (installer, temp) = create_test_installer();
        let source_dir = create_mock_marketplace(&temp, "test-market");

        // Add marketplace
        installer
            .registry
            .add_marketplace_local("test-market", source_dir.to_str().unwrap())
            .unwrap();

        // Create symlink
        let target = temp.path().join("marketplaces/test-market");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source_dir, &target).unwrap();

        // Search
        let results = installer.search_plugins("test").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test-plugin");
    }
}
