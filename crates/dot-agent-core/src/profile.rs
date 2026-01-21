use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{DotAgentError, Result};

const PROFILES_DIR: &str = "profiles";
const IGNORED_FILES: &[&str] = &[".DS_Store", ".gitignore", ".gitkeep"];
const IGNORED_EXTENSIONS: &[&str] = &[];

/// Default directories to exclude from profile operations
pub const DEFAULT_EXCLUDED_DIRS: &[&str] = &[".git"];

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

pub struct Profile {
    pub name: String,
    pub path: PathBuf,
}

impl Profile {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }

    /// List all files in the profile directory (relative paths) with default ignore config
    pub fn list_files(&self) -> Result<Vec<PathBuf>> {
        self.list_files_with_config(&IgnoreConfig::with_defaults())
    }

    /// List all files in the profile directory (relative paths) with custom ignore config
    pub fn list_files_with_config(&self, config: &IgnoreConfig) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(&self.path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Ok(relative) = path.strip_prefix(&self.path) {
                    if !config.should_ignore(relative) {
                        files.push(relative.to_path_buf());
                    }
                }
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

        Ok(Profile::new(name.to_string(), path))
    }

    /// Remove a profile
    pub fn remove_profile(&self, name: &str) -> Result<()> {
        let profile = self.get_profile(name)?;
        fs::remove_dir_all(&profile.path)?;
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

        Ok(Profile::new(dest_name.to_string(), dest_path))
    }

    /// Import a directory as a profile
    pub fn import_profile(&self, source: &Path, name: &str, force: bool) -> Result<Profile> {
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

        Ok(Profile::new(name.to_string(), dest))
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
}
