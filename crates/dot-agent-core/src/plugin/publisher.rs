//! Profile Publisher
//!
//! Converts dot-agent Profiles to Claude Code Plugin format

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};
use crate::plugin::registry::PluginRegistry;
use crate::plugin::types::{InstallScope, InstalledPlugin, PluginAuthor, PluginManifest};
use crate::profile::Profile;

/// Profile metadata for plugin publishing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfilePluginConfig {
    /// Profile version (semver)
    #[serde(default = "default_version")]
    pub version: String,
    /// Description
    pub description: Option<String>,
    /// Author name
    pub author: Option<String>,
    /// Author email
    pub author_email: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Keywords for searchability
    #[serde(default)]
    pub keywords: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Result of profile publishing
#[derive(Debug)]
pub struct PublishResult {
    /// Plugin name (same as profile name)
    pub plugin_name: String,
    /// Marketplace name
    pub marketplace_name: String,
    /// Full plugin ID (name@marketplace)
    pub full_id: String,
    /// Version published
    pub version: String,
    /// Installation path
    pub install_path: PathBuf,
    /// Number of files copied
    pub files_copied: usize,
    /// Whether rules were copied to .claude/rules/
    pub rules_copied: bool,
    /// Whether CLAUDE.md was copied
    pub claude_md_copied: bool,
}

/// Profile Publisher - converts Profiles to Plugins
pub struct ProfilePublisher {
    registry: PluginRegistry,
    /// Virtual marketplace name for published profiles
    marketplace_name: String,
}

impl ProfilePublisher {
    /// Marketplace name for dot-agent profiles
    pub const MARKETPLACE_NAME: &'static str = "dot-agent-profiles";

    /// Create a new ProfilePublisher
    pub fn new() -> Result<Self> {
        let registry = PluginRegistry::new()?;
        Ok(Self {
            registry,
            marketplace_name: Self::MARKETPLACE_NAME.to_string(),
        })
    }

    /// Create with custom plugins directory (for testing)
    pub fn with_dir(plugins_dir: PathBuf) -> Self {
        let registry = PluginRegistry::with_dir(plugins_dir);
        Self {
            registry,
            marketplace_name: Self::MARKETPLACE_NAME.to_string(),
        }
    }

    /// Publish a profile as a Claude Code plugin
    pub fn publish(
        &self,
        profile: &Profile,
        config: &ProfilePluginConfig,
        scope: InstallScope,
    ) -> Result<PublishResult> {
        let plugin_name = &profile.name;
        let version = &config.version;

        // Create target directory
        let install_path = self
            .registry
            .plugins_dir()
            .join("cache")
            .join(&self.marketplace_name)
            .join(plugin_name)
            .join(version);

        // Remove existing if present
        if install_path.exists() {
            fs::remove_dir_all(&install_path)?;
        }

        fs::create_dir_all(&install_path)?;

        // Generate plugin.json
        let manifest = self.generate_manifest(profile, config);
        let manifest_dir = install_path.join(".claude-plugin");
        fs::create_dir_all(&manifest_dir)?;
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(manifest_dir.join("plugin.json"), manifest_json)?;

        // Copy profile contents
        let files_copied = self.copy_profile_contents(&profile.path, &install_path)?;

        // Handle rules/ separately (copy to .claude/rules/ based on scope)
        let rules_copied = self.copy_rules_to_claude(profile, scope)?;

        // Handle CLAUDE.md separately (copy/append to .claude/CLAUDE.md based on scope)
        let claude_md_copied = self.copy_claude_md(profile, scope)?;

        // Ensure virtual marketplace exists
        self.ensure_virtual_marketplace()?;

        // Register in installed_plugins.json
        self.registry.add_installed_plugin(
            plugin_name,
            &self.marketplace_name,
            &install_path,
            version,
            scope,
        )?;

        let full_id = format!("{}@{}", plugin_name, self.marketplace_name);

        Ok(PublishResult {
            plugin_name: plugin_name.clone(),
            marketplace_name: self.marketplace_name.clone(),
            full_id,
            version: version.clone(),
            install_path,
            files_copied,
            rules_copied,
            claude_md_copied,
        })
    }

    /// Unpublish a profile (remove from plugin cache)
    pub fn unpublish(&self, profile_name: &str) -> Result<()> {
        // Remove from installed_plugins.json
        self.registry
            .remove_installed_plugin(profile_name, &self.marketplace_name)?;

        // Remove cache directory
        let cache_path = self
            .registry
            .plugins_dir()
            .join("cache")
            .join(&self.marketplace_name)
            .join(profile_name);

        if cache_path.exists() {
            fs::remove_dir_all(&cache_path)?;
        }

        Ok(())
    }

    /// List published profiles
    pub fn list_published(&self) -> Result<Vec<(String, InstalledPlugin)>> {
        let all_plugins = self.registry.list_installed_plugins()?;

        Ok(all_plugins
            .into_iter()
            .filter(|(full_id, _)| full_id.ends_with(&format!("@{}", self.marketplace_name)))
            .collect())
    }

    /// Check if a profile is published
    pub fn is_published(&self, profile_name: &str) -> Result<bool> {
        self.registry
            .is_plugin_installed(profile_name, &self.marketplace_name)
    }

    /// Get published profile info
    pub fn get_published(&self, profile_name: &str) -> Result<Option<InstalledPlugin>> {
        self.registry
            .get_installed_plugin(profile_name, &self.marketplace_name)
    }

    /// Bump version and republish
    pub fn republish(
        &self,
        profile: &Profile,
        config: &ProfilePluginConfig,
        scope: InstallScope,
        bump: VersionBump,
    ) -> Result<PublishResult> {
        // Get current version if published
        let current_version = if let Ok(Some(installed)) = self.get_published(&profile.name) {
            installed.version
        } else {
            "0.0.0".to_string()
        };

        // Bump version
        let new_version = bump_version(&current_version, bump);

        // Create new config with bumped version
        let mut new_config = config.clone();
        new_config.version = new_version;

        // Publish with new version
        self.publish(profile, &new_config, scope)
    }

    /// Generate plugin.json manifest
    fn generate_manifest(&self, profile: &Profile, config: &ProfilePluginConfig) -> PluginManifest {
        let author = config.author.as_ref().map(|name| PluginAuthor {
            name: name.clone(),
            email: config.author_email.clone(),
        });

        PluginManifest {
            name: profile.name.clone(),
            version: Some(config.version.clone()),
            description: config.description.clone(),
            author,
            homepage: None,
            repository: config.repository.clone(),
            license: None,
            keywords: if config.keywords.is_empty() {
                None
            } else {
                Some(config.keywords.clone())
            },
        }
    }

    /// Copy profile contents to plugin directory
    fn copy_profile_contents(&self, src: &Path, dst: &Path) -> Result<usize> {
        let mut count = 0;

        // Directories to copy as-is (plugin-compatible)
        let plugin_dirs = ["skills", "commands", "agents", "hooks"];

        // Files to copy as-is
        let plugin_files = [".mcp.json", ".lsp.json"];

        for dir_name in &plugin_dirs {
            let src_dir = src.join(dir_name);
            if src_dir.exists() && src_dir.is_dir() {
                let dst_dir = dst.join(dir_name);
                count += self.copy_dir_recursive(&src_dir, &dst_dir)?;
            }
        }

        for file_name in &plugin_files {
            let src_file = src.join(file_name);
            if src_file.exists() && src_file.is_file() {
                let dst_file = dst.join(file_name);
                fs::copy(&src_file, &dst_file)?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Copy rules/ to .claude/rules/ (global or project based on scope)
    fn copy_rules_to_claude(&self, profile: &Profile, scope: InstallScope) -> Result<bool> {
        let rules_dir = profile.path.join("rules");
        if !rules_dir.exists() || !rules_dir.is_dir() {
            return Ok(false);
        }

        // Determine target based on scope
        let claude_rules_dir = match scope {
            InstallScope::User => {
                // Global: ~/.claude/rules/
                dirs::home_dir()
                    .ok_or_else(|| DotAgentError::ConfigParseSimple {
                        message: "Cannot determine home directory".to_string(),
                    })?
                    .join(".claude")
                    .join("rules")
            }
            InstallScope::Project | InstallScope::Local => {
                // Project: ./.claude/rules/
                std::env::current_dir()?.join(".claude").join("rules")
            }
        };

        fs::create_dir_all(&claude_rules_dir)?;

        // Copy each rule file with profile prefix to avoid conflicts
        for entry in fs::read_dir(&rules_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(filename) = path.file_name() {
                    let prefixed_name = format!("{}_{}", profile.name, filename.to_string_lossy());
                    let dst_path = claude_rules_dir.join(prefixed_name);
                    fs::copy(&path, &dst_path)?;
                }
            }
        }

        Ok(true)
    }

    /// Copy CLAUDE.md to .claude/CLAUDE.md (global or project based on scope)
    fn copy_claude_md(&self, profile: &Profile, scope: InstallScope) -> Result<bool> {
        let claude_md = profile.path.join("CLAUDE.md");
        if !claude_md.exists() || !claude_md.is_file() {
            return Ok(false);
        }

        // Determine target based on scope
        let target_claude_md = match scope {
            InstallScope::User => {
                // Global: ~/.claude/CLAUDE.md
                dirs::home_dir()
                    .ok_or_else(|| DotAgentError::ConfigParseSimple {
                        message: "Cannot determine home directory".to_string(),
                    })?
                    .join(".claude")
                    .join("CLAUDE.md")
            }
            InstallScope::Project | InstallScope::Local => {
                // Project: ./.claude/CLAUDE.md
                std::env::current_dir()?.join(".claude").join("CLAUDE.md")
            }
        };

        if let Some(parent) = target_claude_md.parent() {
            fs::create_dir_all(parent)?;
        }

        let source_content = fs::read_to_string(&claude_md)?;

        // Add profile header
        let section = format!(
            "\n\n<!-- Profile: {} -->\n{}\n<!-- End Profile: {} -->\n",
            profile.name, source_content, profile.name
        );

        if target_claude_md.exists() {
            let existing = fs::read_to_string(&target_claude_md)?;

            // Check if section already exists
            let marker = format!("<!-- Profile: {} -->", profile.name);
            if existing.contains(&marker) {
                // Replace existing section
                let start_marker = format!("<!-- Profile: {} -->", profile.name);
                let end_marker = format!("<!-- End Profile: {} -->", profile.name);

                if let (Some(start), Some(end)) =
                    (existing.find(&start_marker), existing.find(&end_marker))
                {
                    let before = &existing[..start];
                    let after = &existing[end + end_marker.len()..];
                    let new_content = format!("{}{}{}", before.trim_end(), section, after);
                    fs::write(&target_claude_md, new_content)?;
                }
            } else {
                // Append section
                let mut content = existing;
                content.push_str(&section);
                fs::write(&target_claude_md, content)?;
            }
        } else {
            // Create new file
            fs::write(&target_claude_md, section.trim_start())?;
        }

        Ok(true)
    }

    /// Ensure virtual marketplace entry exists
    fn ensure_virtual_marketplace(&self) -> Result<()> {
        // Check if marketplace already exists
        if self
            .registry
            .get_marketplace(&self.marketplace_name)?
            .is_some()
        {
            return Ok(());
        }

        // Add virtual marketplace
        let profiles_dir = dirs::home_dir()
            .ok_or_else(|| DotAgentError::ConfigParseSimple {
                message: "Cannot determine home directory".to_string(),
            })?
            .join(".dot-agent")
            .join("profiles");

        self.registry.add_marketplace_local(
            &self.marketplace_name,
            profiles_dir.to_string_lossy().as_ref(),
        )?;

        Ok(())
    }

    /// Copy directory recursively
    #[allow(clippy::only_used_in_recursion)]
    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<usize> {
        fs::create_dir_all(dst)?;
        let mut count = 0;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                // Skip .git directory
                if entry.file_name() == ".git" {
                    continue;
                }
                count += self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
                count += 1;
            }
        }

        Ok(count)
    }
}

// Note: Default impl removed - use ProfilePublisher::new() which returns Result

/// Version bump type
#[derive(Debug, Clone, Copy)]
pub enum VersionBump {
    /// Patch version (1.0.0 -> 1.0.1)
    Patch,
    /// Minor version (1.0.0 -> 1.1.0)
    Minor,
    /// Major version (1.0.0 -> 2.0.0)
    Major,
}

/// Bump semver version
fn bump_version(version: &str, bump: VersionBump) -> String {
    let parts: Vec<u32> = version.split('.').filter_map(|s| s.parse().ok()).collect();

    let (major, minor, patch) = match parts.as_slice() {
        [major, minor, patch, ..] => (*major, *minor, *patch),
        [major, minor] => (*major, *minor, 0),
        [major] => (*major, 0, 0),
        [] => (0, 0, 0),
    };

    match bump {
        VersionBump::Patch => format!("{}.{}.{}", major, minor, patch + 1),
        VersionBump::Minor => format!("{}.{}.0", major, minor + 1),
        VersionBump::Major => format!("{}.0.0", major + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bump_version_patch() {
        assert_eq!(bump_version("1.0.0", VersionBump::Patch), "1.0.1");
        assert_eq!(bump_version("1.2.3", VersionBump::Patch), "1.2.4");
    }

    #[test]
    fn test_bump_version_minor() {
        assert_eq!(bump_version("1.0.0", VersionBump::Minor), "1.1.0");
        assert_eq!(bump_version("1.2.3", VersionBump::Minor), "1.3.0");
    }

    #[test]
    fn test_bump_version_major() {
        assert_eq!(bump_version("1.0.0", VersionBump::Major), "2.0.0");
        assert_eq!(bump_version("1.2.3", VersionBump::Major), "2.0.0");
    }

    #[test]
    fn test_bump_version_incomplete() {
        assert_eq!(bump_version("1.0", VersionBump::Patch), "1.0.1");
        assert_eq!(bump_version("1", VersionBump::Minor), "1.1.0");
    }

    fn create_test_publisher() -> (ProfilePublisher, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let publisher = ProfilePublisher::with_dir(temp_dir.path().to_path_buf());
        (publisher, temp_dir)
    }

    fn create_mock_profile(temp: &TempDir, name: &str) -> Profile {
        let profile_dir = temp.path().join("profiles").join(name);
        fs::create_dir_all(&profile_dir).unwrap();

        // Create skills directory
        let skills_dir = profile_dir.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(skills_dir.join("test-skill.md"), "# Test Skill").unwrap();

        // Create commands directory
        let commands_dir = profile_dir.join("commands");
        fs::create_dir_all(&commands_dir).unwrap();
        fs::write(commands_dir.join("test-cmd.md"), "# Test Command").unwrap();

        Profile {
            name: name.to_string(),
            path: profile_dir,
        }
    }

    #[test]
    fn test_publish_profile() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "test-profile");

        let config = ProfilePluginConfig {
            version: "1.0.0".to_string(),
            description: Some("Test profile".to_string()),
            author: Some("Test Author".to_string()),
            ..Default::default()
        };

        let result = publisher
            .publish(&profile, &config, InstallScope::User)
            .unwrap();

        assert_eq!(result.plugin_name, "test-profile");
        assert_eq!(result.marketplace_name, "dot-agent-profiles");
        assert_eq!(result.version, "1.0.0");
        assert!(result.install_path.exists());
        assert!(result.files_copied > 0);
    }

    #[test]
    fn test_unpublish_profile() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "test-profile");

        let config = ProfilePluginConfig::default();
        publisher
            .publish(&profile, &config, InstallScope::User)
            .unwrap();

        // Verify published
        assert!(publisher.is_published("test-profile").unwrap());

        // Unpublish
        publisher.unpublish("test-profile").unwrap();

        // Verify unpublished
        assert!(!publisher.is_published("test-profile").unwrap());
    }

    #[test]
    fn test_list_published() {
        let (publisher, temp) = create_test_publisher();
        let profile1 = create_mock_profile(&temp, "profile-1");
        let profile2 = create_mock_profile(&temp, "profile-2");

        let config = ProfilePluginConfig::default();
        publisher
            .publish(&profile1, &config, InstallScope::User)
            .unwrap();
        publisher
            .publish(&profile2, &config, InstallScope::User)
            .unwrap();

        let published = publisher.list_published().unwrap();
        assert_eq!(published.len(), 2);
    }

    #[test]
    fn test_generate_manifest() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "test-profile");

        let config = ProfilePluginConfig {
            version: "2.0.0".to_string(),
            description: Some("A test profile".to_string()),
            author: Some("Test Author".to_string()),
            author_email: Some("test@example.com".to_string()),
            repository: Some("https://github.com/test/repo".to_string()),
            keywords: vec!["rust".to_string(), "test".to_string()],
        };

        let manifest = publisher.generate_manifest(&profile, &config);

        assert_eq!(manifest.name, "test-profile");
        assert_eq!(manifest.version, Some("2.0.0".to_string()));
        assert_eq!(manifest.description, Some("A test profile".to_string()));
        assert!(manifest.author.is_some());
        assert_eq!(manifest.author.as_ref().unwrap().name, "Test Author");
    }

    fn create_mock_profile_with_extras(temp: &TempDir, name: &str) -> Profile {
        let profile_dir = temp.path().join("profiles").join(name);
        fs::create_dir_all(&profile_dir).unwrap();

        // Create skills directory
        let skills_dir = profile_dir.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(skills_dir.join("test-skill.md"), "# Test Skill").unwrap();

        // Create hooks directory
        let hooks_dir = profile_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        fs::write(hooks_dir.join("hooks.json"), r#"{"PostToolUse": []}"#).unwrap();

        // Create .mcp.json
        fs::write(profile_dir.join(".mcp.json"), r#"{"mcpServers": {}}"#).unwrap();

        // Create .lsp.json
        fs::write(profile_dir.join(".lsp.json"), r#"{"rust-analyzer": {}}"#).unwrap();

        // Create rules directory
        let rules_dir = profile_dir.join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("test-rule.md"), "# Test Rule").unwrap();

        // Create CLAUDE.md
        fs::write(profile_dir.join("CLAUDE.md"), "# Test Profile Instructions").unwrap();

        Profile {
            name: name.to_string(),
            path: profile_dir,
        }
    }

    #[test]
    fn test_publish_copies_hooks_and_config() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile_with_extras(&temp, "full-profile");

        let config = ProfilePluginConfig::default();
        let result = publisher
            .publish(&profile, &config, InstallScope::User)
            .unwrap();

        // Verify hooks/ was copied
        assert!(result.install_path.join("hooks/hooks.json").exists());

        // Verify .mcp.json was copied
        assert!(result.install_path.join(".mcp.json").exists());

        // Verify .lsp.json was copied
        assert!(result.install_path.join(".lsp.json").exists());

        // Verify plugin.json was generated
        assert!(result
            .install_path
            .join(".claude-plugin/plugin.json")
            .exists());
    }

    #[test]
    fn test_publish_with_project_scope() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "project-profile");

        let config = ProfilePluginConfig::default();
        let result = publisher
            .publish(&profile, &config, InstallScope::Project)
            .unwrap();

        assert_eq!(result.plugin_name, "project-profile");
        // Scope should be Project
        let installed = publisher.get_published("project-profile").unwrap().unwrap();
        assert_eq!(installed.scope, InstallScope::Project);
    }

    #[test]
    fn test_publish_with_local_scope() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "local-profile");

        let config = ProfilePluginConfig::default();
        let result = publisher
            .publish(&profile, &config, InstallScope::Local)
            .unwrap();

        assert_eq!(result.plugin_name, "local-profile");
        // Scope should be Local
        let installed = publisher.get_published("local-profile").unwrap().unwrap();
        assert_eq!(installed.scope, InstallScope::Local);
    }

    #[test]
    fn test_copy_profile_contents() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile_with_extras(&temp, "copy-test");

        let dst = temp.path().join("output");
        fs::create_dir_all(&dst).unwrap();

        let count = publisher
            .copy_profile_contents(&profile.path, &dst)
            .unwrap();

        // skills/test-skill.md, hooks/hooks.json, .mcp.json, .lsp.json = 4 files
        assert_eq!(count, 4);

        // Verify files exist
        assert!(dst.join("skills/test-skill.md").exists());
        assert!(dst.join("hooks/hooks.json").exists());
        assert!(dst.join(".mcp.json").exists());
        assert!(dst.join(".lsp.json").exists());

        // rules/ and CLAUDE.md should NOT be copied (handled separately)
        assert!(!dst.join("rules").exists());
        assert!(!dst.join("CLAUDE.md").exists());
    }

    #[test]
    fn test_republish_bumps_version() {
        let (publisher, temp) = create_test_publisher();
        let profile = create_mock_profile(&temp, "bump-test");

        let config = ProfilePluginConfig {
            version: "1.0.0".to_string(),
            ..Default::default()
        };

        // Initial publish
        let result1 = publisher
            .publish(&profile, &config, InstallScope::User)
            .unwrap();
        assert_eq!(result1.version, "1.0.0");

        // Republish with patch bump
        let result2 = publisher
            .republish(&profile, &config, InstallScope::User, VersionBump::Patch)
            .unwrap();
        assert_eq!(result2.version, "1.0.1");

        // Republish with minor bump
        let result3 = publisher
            .republish(&profile, &config, InstallScope::User, VersionBump::Minor)
            .unwrap();
        assert_eq!(result3.version, "1.1.0");
    }
}
