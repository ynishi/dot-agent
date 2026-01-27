use std::fs;
use std::path::{Path, PathBuf};

use once_cell::unsync::OnceCell;

use walkdir::WalkDir;

use crate::error::{DotAgentError, Result};
use crate::plugin_manifest::{FilterConfig, PluginManifest, DEFAULT_COMPONENT_DIRS};
use crate::profile_metadata::{
    PluginScope, ProfileIndexEntry, ProfileMetadata, ProfileSource, ProfilesIndex,
};

const PROFILES_DIR: &str = "profiles";
const IGNORED_FILES: &[&str] = &[".DS_Store", ".gitignore", ".gitkeep"];
const IGNORED_EXTENSIONS: &[&str] = &[];

/// Default directories to exclude from profile operations
pub const DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    // Test directories
    "tests",
    "test",
    "__tests__",
    // Build/cache directories
    "__pycache__",
    ".pytest_cache",
    "node_modules",
    "target",
    // IDE/editor directories
    ".vscode",
    ".idea",
    // CI/CD directories
    ".github",
    ".gitlab",
];

/// Configuration for file ignore/include behavior
#[derive(Debug, Clone, Default)]
pub struct IgnoreConfig {
    /// Directories to exclude (checked against path components)
    pub excluded_dirs: Vec<String>,
    /// Directories to explicitly include (overrides default exclusions)
    pub included_dirs: Vec<String>,
}

impl IgnoreConfig {
    /// Create with default exclusions (.git)
    pub fn with_defaults() -> Self {
        Self {
            excluded_dirs: DEFAULT_EXCLUDED_DIRS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            included_dirs: Vec::new(),
        }
    }

    /// Add a directory to exclude
    pub fn exclude(mut self, dir: impl Into<String>) -> Self {
        self.excluded_dirs.push(dir.into());
        self
    }

    /// Add a directory to include (overrides default exclusion)
    pub fn include(mut self, dir: impl Into<String>) -> Self {
        self.included_dirs.push(dir.into());
        self
    }

    /// Check if a path should be ignored based on this config
    pub fn should_ignore(&self, path: &Path) -> bool {
        // Check static file ignores first
        if should_ignore_file(path) {
            return true;
        }

        // Check path components against excluded directories
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                let name_str = name.to_string_lossy();

                // Check if explicitly included (overrides exclusion)
                if self.included_dirs.iter().any(|d| d == name_str.as_ref()) {
                    continue;
                }

                // Check if excluded
                if self.excluded_dirs.iter().any(|d| d == name_str.as_ref()) {
                    return true;
                }
            }
        }

        false
    }
}

/// Check if a file should be ignored (static rules, not directory-based)
fn should_ignore_file(path: &Path) -> bool {
    if let Some(name) = path.file_name() {
        let name = name.to_string_lossy();
        if IGNORED_FILES.contains(&name.as_ref()) {
            return true;
        }
    }

    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy();
        if IGNORED_EXTENSIONS.contains(&ext.as_ref()) {
            return true;
        }
    }

    false
}

/// Root file always allowed (CLAUDE.md)
const ALLOWED_ROOT_FILE: &str = "CLAUDE.md";

/// Filter that determines which files are allowed for sync
/// Built once from: (DEFAULT_DIRS or plugin paths) + CLAUDE.md + include - exclude
struct AllowedFilter {
    /// Base directories (from plugin.json or DEFAULT_COMPONENT_DIRS)
    base_dirs: Vec<PathBuf>,
    /// Include patterns from .dot-agent.toml
    include_patterns: Vec<glob::Pattern>,
    /// Exclude patterns from .dot-agent.toml
    exclude_patterns: Vec<glob::Pattern>,
}

impl AllowedFilter {
    fn new(manifest: &Option<PluginManifest>, filter: &Option<FilterConfig>) -> Self {
        // Determine base directories
        let base_dirs: Vec<PathBuf> = if let Some(ref m) = manifest {
            if m.has_explicit_paths() {
                m.get_component_paths()
            } else {
                DEFAULT_COMPONENT_DIRS.iter().map(PathBuf::from).collect()
            }
        } else {
            DEFAULT_COMPONENT_DIRS.iter().map(PathBuf::from).collect()
        };

        // Parse include/exclude patterns
        let (include_patterns, exclude_patterns) = if let Some(ref f) = filter {
            let includes = f
                .include
                .iter()
                .filter_map(|p| glob::Pattern::new(p).ok())
                .collect();
            let excludes = f
                .exclude
                .iter()
                .filter_map(|p| glob::Pattern::new(p).ok())
                .collect();
            (includes, excludes)
        } else {
            (Vec::new(), Vec::new())
        };

        Self {
            base_dirs,
            include_patterns,
            exclude_patterns,
        }
    }

    /// Check if a relative path is allowed
    fn matches(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // 1. Check exclude first - if excluded, not allowed
        for pattern in &self.exclude_patterns {
            if pattern.matches(&path_str) {
                return false;
            }
        }

        // 2. Check if it's the always-allowed root file (CLAUDE.md)
        if path.components().count() == 1 {
            if let Some(name) = path.file_name() {
                if name == ALLOWED_ROOT_FILE {
                    return true;
                }
            }
        }

        // 3. Check if path starts with any base directory
        for base_dir in &self.base_dirs {
            if path.starts_with(base_dir) {
                return true;
            }
        }

        // 4. Check include patterns
        for pattern in &self.include_patterns {
            if pattern.matches(&path_str) {
                return true;
            }
        }

        false
    }
}

/// A profile containing configuration files for Claude Code
///
/// Profile aggregates related metadata with lazy loading:
/// - `PluginManifest` from `.claude-plugin/plugin.json`
/// - `ProfileMetadata` from `.dot-agent.toml`
/// - `FilterConfig` from `.dot-agent.toml` filter section
pub struct Profile {
    pub name: String,
    pub path: PathBuf,

    // Lazy-loaded cached data
    manifest: OnceCell<Option<PluginManifest>>,
    metadata: OnceCell<Option<ProfileMetadata>>,
    filter: OnceCell<Option<FilterConfig>>,
}

impl Profile {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            manifest: OnceCell::new(),
            metadata: OnceCell::new(),
            filter: OnceCell::new(),
        }
    }

    // =========================================================================
    // Lazy-loaded accessors
    // =========================================================================

    /// Get plugin manifest (lazy loaded from `.claude-plugin/plugin.json`)
    pub fn manifest(&self) -> Result<Option<&PluginManifest>> {
        self.manifest
            .get_or_try_init(|| PluginManifest::load(&self.path))
            .map(|opt| opt.as_ref())
    }

    /// Get profile metadata (lazy loaded from `.dot-agent.toml`)
    pub fn metadata(&self) -> Result<Option<&ProfileMetadata>> {
        self.metadata
            .get_or_try_init(|| ProfileMetadata::load(&self.path))
            .map(|opt| opt.as_ref())
    }

    /// Get filter config (lazy loaded from `.dot-agent.toml`)
    pub fn filter_config(&self) -> Result<Option<&FilterConfig>> {
        self.filter
            .get_or_try_init(|| FilterConfig::load(&self.path))
            .map(|opt| opt.as_ref())
    }

    // =========================================================================
    // Convenience methods
    // =========================================================================

    /// Get profile source (Local, Git, or Marketplace)
    pub fn source(&self) -> Result<ProfileSource> {
        Ok(self
            .metadata()?
            .map(|m| m.source.clone())
            .unwrap_or_default())
    }

    /// Get profile version
    pub fn version(&self) -> Result<Option<String>> {
        Ok(self.metadata()?.and_then(|m| m.profile.version.clone()))
    }

    /// Get profile description
    pub fn description(&self) -> Result<Option<String>> {
        Ok(self.metadata()?.and_then(|m| m.profile.description.clone()))
    }

    /// Check if profile has plugin features (hooks, MCP, LSP)
    pub fn has_plugin_features(&self) -> bool {
        ProfileMetadata::has_plugin_features(&self.path)
    }

    /// Get plugin scope (User, Project, or Local)
    pub fn plugin_scope(&self) -> Result<PluginScope> {
        Ok(self.metadata()?.map(|m| m.plugin.scope).unwrap_or_default())
    }

    /// Check if plugin is enabled
    pub fn plugin_enabled(&self) -> Result<bool> {
        Ok(self.metadata()?.map(|m| m.plugin.enabled).unwrap_or(true))
    }

    // =========================================================================
    // File listing
    // =========================================================================

    /// List all files in the profile directory (relative paths) with default ignore config
    pub fn list_files(&self) -> Result<Vec<PathBuf>> {
        self.list_files_with_config(&IgnoreConfig::with_defaults())
    }

    /// List all files in the profile directory (relative paths) with custom ignore config
    ///
    /// File collection logic:
    /// 1. Build allowed filter: (DEFAULT_DIRS or plugin paths) + CLAUDE.md + include - exclude
    /// 2. Walk all files and collect only those matching the filter
    pub fn list_files_with_config(&self, config: &IgnoreConfig) -> Result<Vec<PathBuf>> {
        // Use cached manifest and filter
        let manifest = self.manifest()?;
        let filter = self.filter_config()?;

        // Build allowed filter upfront
        let allowed = AllowedFilter::new(&manifest.cloned(), &filter.cloned());

        // Walk all files and collect only those matching the filter
        let mut files: Vec<PathBuf> = Vec::new();

        for entry in WalkDir::new(&self.path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let relative = match path.strip_prefix(&self.path) {
                Ok(r) => r,
                Err(_) => continue,
            };

            // Skip system-ignored files (.DS_Store, .git, etc.)
            if config.should_ignore(relative) {
                continue;
            }

            // Check against allowed filter
            if allowed.matches(relative) {
                files.push(relative.to_path_buf());
            }
        }

        files.sort();
        Ok(files)
    }

    /// Get contents summary (e.g., "skills (5), commands (3)")
    pub fn contents_summary(&self) -> String {
        self.contents_summary_with_config(&IgnoreConfig::with_defaults())
    }

    /// Get contents summary with custom ignore config
    pub fn contents_summary_with_config(&self, config: &IgnoreConfig) -> String {
        let mut summary = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    let count = WalkDir::new(&path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            if !e.path().is_file() {
                                return false;
                            }
                            if let Ok(relative) = e.path().strip_prefix(&self.path) {
                                !config.should_ignore(relative)
                            } else {
                                false
                            }
                        })
                        .count();
                    if count > 0 {
                        summary.push(format!("{} ({})", name, count));
                    }
                } else if path.is_file() {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    if let Ok(relative) = path.strip_prefix(&self.path) {
                        if !config.should_ignore(relative) {
                            summary.push(name);
                        }
                    }
                }
            }
        }

        if summary.is_empty() {
            "(empty)".to_string()
        } else {
            summary.join(", ")
        }
    }
}

pub struct ProfileManager {
    base_dir: PathBuf,
}

impl ProfileManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.base_dir.join(PROFILES_DIR)
    }

    /// Discover all profiles
    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let profiles_dir = self.profiles_dir();
        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                profiles.push(Profile::new(name, path));
            }
        }

        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    /// Get a specific profile
    pub fn get_profile(&self, name: &str) -> Result<Profile> {
        let path = self.profiles_dir().join(name);
        if !path.exists() {
            return Err(DotAgentError::ProfileNotFound {
                name: name.to_string(),
            });
        }
        Ok(Profile::new(name.to_string(), path))
    }

    /// Create a new profile with scaffolding
    pub fn create_profile(&self, name: &str) -> Result<Profile> {
        validate_profile_name(name)?;

        let path = self.profiles_dir().join(name);
        if path.exists() {
            return Err(DotAgentError::ProfileAlreadyExists {
                name: name.to_string(),
            });
        }

        // Create directory structure
        fs::create_dir_all(&path)?;
        fs::create_dir_all(path.join("agents"))?;
        fs::create_dir_all(path.join("commands"))?;
        fs::create_dir_all(path.join("hooks"))?;
        fs::create_dir_all(path.join("plugins"))?;
        fs::create_dir_all(path.join("rules"))?;
        fs::create_dir_all(path.join("skills"))?;

        // Create CLAUDE.md template
        let claude_md = format!(
            r#"# {} Profile

## Overview

<!-- Describe what this profile is for -->

## Usage

```bash
dot-agent install {}
```

## Customization

<!-- Add project-specific instructions here -->
"#,
            name, name
        );
        fs::write(path.join("CLAUDE.md"), claude_md)?;

        // Create profile metadata
        let metadata = ProfileMetadata::new_local(name);
        metadata.save(&path)?;

        // Update profiles index
        let mut index = ProfilesIndex::load(&self.base_dir)?;
        index.upsert(name, ProfileIndexEntry::new_local(name));
        index.save(&self.base_dir)?;

        Ok(Profile::new(name.to_string(), path))
    }

    /// Remove a profile
    pub fn remove_profile(&self, name: &str) -> Result<()> {
        let profile = self.get_profile(name)?;
        fs::remove_dir_all(&profile.path)?;

        // Update profiles index
        let mut index = ProfilesIndex::load(&self.base_dir)?;
        index.remove(name);
        index.save(&self.base_dir)?;

        Ok(())
    }

    /// Copy an existing profile to a new name
    pub fn copy_profile(&self, source_name: &str, dest_name: &str, force: bool) -> Result<Profile> {
        let source = self.get_profile(source_name)?;
        validate_profile_name(dest_name)?;

        let dest_path = self.profiles_dir().join(dest_name);

        if dest_path.exists() {
            if !force {
                return Err(DotAgentError::ProfileAlreadyExists {
                    name: dest_name.to_string(),
                });
            }
            fs::remove_dir_all(&dest_path)?;
        }

        copy_dir_recursive(&source.path, &dest_path)?;

        // Update metadata with new name
        if let Some(mut metadata) = ProfileMetadata::load(&dest_path)? {
            metadata.profile.name = dest_name.to_string();
            metadata.save(&dest_path)?;
        } else {
            let metadata = ProfileMetadata::new_local(dest_name);
            metadata.save(&dest_path)?;
        }

        // Update profiles index
        let mut index = ProfilesIndex::load(&self.base_dir)?;
        index.upsert(dest_name, ProfileIndexEntry::new_local(dest_name));
        index.save(&self.base_dir)?;

        Ok(Profile::new(dest_name.to_string(), dest_path))
    }

    /// Import a directory as a profile (local source)
    pub fn import_profile(&self, source: &Path, name: &str, force: bool) -> Result<Profile> {
        self.import_profile_with_source(source, name, force, ProfileSource::Local)
    }

    /// Import a directory as a profile from git
    #[allow(clippy::too_many_arguments)]
    pub fn import_profile_from_git(
        &self,
        source: &Path,
        name: &str,
        force: bool,
        url: &str,
        branch: Option<&str>,
        commit: Option<&str>,
        subpath: Option<&str>,
    ) -> Result<Profile> {
        let source_info = ProfileSource::Git {
            url: url.to_string(),
            branch: branch.map(|s| s.to_string()),
            commit: commit.map(|s| s.to_string()),
            path: subpath.map(|s| s.to_string()),
        };
        self.import_profile_with_source(source, name, force, source_info)
    }

    /// Import a directory as a profile from marketplace
    pub fn import_profile_from_marketplace(
        &self,
        source: &Path,
        name: &str,
        force: bool,
        channel: &str,
        plugin: &str,
        version: &str,
    ) -> Result<Profile> {
        let source_info = ProfileSource::Marketplace {
            channel: channel.to_string(),
            plugin: plugin.to_string(),
            version: version.to_string(),
        };
        self.import_profile_with_source(source, name, force, source_info)
    }

    /// Import a directory as a profile with source information
    fn import_profile_with_source(
        &self,
        source: &Path,
        name: &str,
        force: bool,
        source_info: ProfileSource,
    ) -> Result<Profile> {
        validate_profile_name(name)?;

        if !source.exists() {
            return Err(DotAgentError::TargetNotFound {
                path: source.to_path_buf(),
            });
        }

        let dest = self.profiles_dir().join(name);

        if dest.exists() {
            if !force {
                return Err(DotAgentError::ProfileAlreadyExists {
                    name: name.to_string(),
                });
            }
            fs::remove_dir_all(&dest)?;
        }

        // Ensure profiles directory exists
        fs::create_dir_all(self.profiles_dir())?;

        // Copy directory recursively
        copy_dir_recursive(source, &dest)?;

        // Create or update profile metadata
        let metadata = match &source_info {
            ProfileSource::Local => ProfileMetadata::new_local(name),
            ProfileSource::Git {
                url,
                branch,
                commit,
                path,
            } => ProfileMetadata::new_git(
                name,
                url,
                branch.as_deref(),
                commit.as_deref(),
                path.as_deref(),
            ),
            ProfileSource::Marketplace {
                channel,
                plugin,
                version,
            } => ProfileMetadata::new_marketplace(name, channel, plugin, version),
        };
        metadata.save(&dest)?;

        // Update profiles index
        let mut index = ProfilesIndex::load(&self.base_dir)?;
        let entry = match &source_info {
            ProfileSource::Local => ProfileIndexEntry::new_local(name),
            ProfileSource::Git {
                url,
                branch,
                commit,
                path,
            } => ProfileIndexEntry::new_git(
                name,
                url,
                branch.as_deref(),
                commit.as_deref(),
                path.as_deref(),
            ),
            ProfileSource::Marketplace {
                channel,
                plugin,
                version,
            } => ProfileIndexEntry::new_marketplace(name, channel, plugin, version),
        };
        index.upsert(name, entry);
        index.save(&self.base_dir)?;

        Ok(Profile::new(name.to_string(), dest))
    }

    /// Get metadata for a profile
    pub fn get_profile_metadata(&self, name: &str) -> Result<Option<ProfileMetadata>> {
        let profile = self.get_profile(name)?;
        ProfileMetadata::load(&profile.path)
    }

    /// Get profile source from index
    pub fn get_profile_source(&self, name: &str) -> Result<Option<ProfileSource>> {
        let index = ProfilesIndex::load(&self.base_dir)?;
        Ok(index.get(name).map(|e| e.source.clone()))
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    copy_dir_recursive_with_config(src, dst, &IgnoreConfig::with_defaults())
}

fn copy_dir_recursive_with_config(src: &Path, dst: &Path, config: &IgnoreConfig) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();
        let relative = src_path.strip_prefix(src).unwrap();
        let dst_path = dst.join(relative);

        // Skip ignored directories/files
        if config.should_ignore(relative) {
            continue;
        }

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)?;
        } else if src_path.is_file() {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(DotAgentError::InvalidProfileName {
            name: name.to_string(),
        });
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() {
        return Err(DotAgentError::InvalidProfileName {
            name: name.to_string(),
        });
    }

    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(DotAgentError::InvalidProfileName {
                name: name.to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_config_default_excludes_git() {
        let config = IgnoreConfig::with_defaults();
        assert!(config.excluded_dirs.contains(&".git".to_string()));
    }

    #[test]
    fn ignore_config_default_excludes_tests() {
        let config = IgnoreConfig::with_defaults();
        assert!(config.excluded_dirs.contains(&"tests".to_string()));
        assert!(config.excluded_dirs.contains(&"test".to_string()));
        assert!(config.excluded_dirs.contains(&"__tests__".to_string()));
    }

    #[test]
    fn ignore_config_default_excludes_build_dirs() {
        let config = IgnoreConfig::with_defaults();
        assert!(config.excluded_dirs.contains(&"__pycache__".to_string()));
        assert!(config.excluded_dirs.contains(&".pytest_cache".to_string()));
        assert!(config.excluded_dirs.contains(&"node_modules".to_string()));
        assert!(config.excluded_dirs.contains(&"target".to_string()));
    }

    #[test]
    fn ignore_config_should_ignore_tests_dir() {
        let config = IgnoreConfig::with_defaults();

        assert!(config.should_ignore(Path::new("tests")));
        assert!(config.should_ignore(Path::new("tests/unit/test_parser.py")));
        assert!(config.should_ignore(Path::new("tests/claude-code/analyze-token-usage.py")));
    }

    #[test]
    fn ignore_config_should_ignore_git_files() {
        let config = IgnoreConfig::with_defaults();

        // .git directory itself
        assert!(config.should_ignore(Path::new(".git")));

        // Files inside .git
        assert!(config.should_ignore(Path::new(".git/HEAD")));
        assert!(config.should_ignore(Path::new(".git/config")));
        assert!(config.should_ignore(Path::new(".git/objects/pack/something.pack")));
    }

    #[test]
    fn ignore_config_should_not_ignore_regular_files() {
        let config = IgnoreConfig::with_defaults();

        assert!(!config.should_ignore(Path::new("README.md")));
        assert!(!config.should_ignore(Path::new("src/main.rs")));
        assert!(!config.should_ignore(Path::new("skills/my-skill/SKILL.md")));
    }

    #[test]
    fn ignore_config_include_overrides_exclude() {
        let config = IgnoreConfig::with_defaults().include(".git");

        // .git should no longer be ignored because it's included
        assert!(!config.should_ignore(Path::new(".git")));
        assert!(!config.should_ignore(Path::new(".git/HEAD")));
    }

    #[test]
    fn ignore_config_additional_exclusions() {
        let config = IgnoreConfig::with_defaults().exclude("node_modules");

        // Both .git and node_modules should be ignored
        assert!(config.should_ignore(Path::new(".git/HEAD")));
        assert!(config.should_ignore(Path::new("node_modules/package/index.js")));
    }

    #[test]
    fn ignore_config_static_file_ignores() {
        let config = IgnoreConfig::with_defaults();

        // Static file ignores should still work
        assert!(config.should_ignore(Path::new(".DS_Store")));
        assert!(config.should_ignore(Path::new(".gitignore")));
        assert!(config.should_ignore(Path::new(".gitkeep")));
        assert!(config.should_ignore(Path::new("some/path/.DS_Store")));
    }

    #[test]
    fn ignore_config_empty_allows_all() {
        let config = IgnoreConfig::default();

        // With no exclusions, nothing directory-related is ignored
        // (but static file ignores still apply)
        assert!(!config.should_ignore(Path::new(".git/HEAD")));
        assert!(!config.should_ignore(Path::new("node_modules/index.js")));

        // Static ignores still work
        assert!(config.should_ignore(Path::new(".DS_Store")));
    }

    #[test]
    fn profile_lazy_load_returns_none_for_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let profile = Profile::new("test".to_string(), tmp.path().to_path_buf());

        // No manifest file exists
        assert!(profile.manifest().unwrap().is_none());

        // No metadata file exists
        assert!(profile.metadata().unwrap().is_none());

        // No filter config exists
        assert!(profile.filter_config().unwrap().is_none());
    }

    #[test]
    fn profile_source_defaults_to_local() {
        let tmp = tempfile::TempDir::new().unwrap();
        let profile = Profile::new("test".to_string(), tmp.path().to_path_buf());

        // Without metadata, source defaults to Local
        let source = profile.source().unwrap();
        assert!(matches!(source, ProfileSource::Local));
    }

    #[test]
    fn profile_plugin_defaults() {
        let tmp = tempfile::TempDir::new().unwrap();
        let profile = Profile::new("test".to_string(), tmp.path().to_path_buf());

        // Without metadata, plugin_enabled defaults to true
        assert!(profile.plugin_enabled().unwrap());

        // Without metadata, plugin_scope defaults to User
        assert!(matches!(profile.plugin_scope().unwrap(), PluginScope::User));
    }
}
