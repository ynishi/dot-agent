//! Plugin Fetcher
//!
//! Downloads plugins from GitHub, URLs, or local paths

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{DotAgentError, Result};
use crate::plugin::types::{Marketplace, PluginEntry, PluginSource, StructuredSource};

/// Plugin Fetcher - downloads plugins to cache
pub struct PluginFetcher {
    /// Cache directory (~/.claude/plugins/cache)
    cache_dir: PathBuf,
    /// Marketplaces directory (~/.claude/plugins/marketplaces)
    marketplaces_dir: PathBuf,
}

impl PluginFetcher {
    /// Create a new PluginFetcher
    pub fn new(plugins_dir: &Path) -> Self {
        Self {
            cache_dir: plugins_dir.join("cache"),
            marketplaces_dir: plugins_dir.join("marketplaces"),
        }
    }

    /// Get cache directory
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get marketplaces directory
    pub fn marketplaces_dir(&self) -> &Path {
        &self.marketplaces_dir
    }

    // ========== Marketplace Fetching ==========

    /// Fetch/clone a marketplace from GitHub
    pub fn fetch_marketplace_github(&self, name: &str, repo: &str) -> Result<PathBuf> {
        let target_dir = self.marketplaces_dir.join(name);

        // Remove existing if present
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        // Clone repository
        let url = format!("https://github.com/{}.git", repo);
        self.git_clone(&url, &target_dir)?;

        Ok(target_dir)
    }

    /// Fetch/clone a marketplace from URL
    pub fn fetch_marketplace_url(&self, name: &str, url: &str) -> Result<PathBuf> {
        let target_dir = self.marketplaces_dir.join(name);

        // Remove existing if present
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        // Clone repository
        self.git_clone(url, &target_dir)?;

        Ok(target_dir)
    }

    /// Link a local marketplace (symlink or copy)
    pub fn fetch_marketplace_local(&self, name: &str, path: &str) -> Result<PathBuf> {
        let source = PathBuf::from(path);
        let target_dir = self.marketplaces_dir.join(name);

        if !source.exists() {
            return Err(DotAgentError::FileNotFound { path: source });
        }

        // Remove existing if present
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        // Create parent directory
        fs::create_dir_all(&self.marketplaces_dir)?;

        // Create symlink
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &target_dir)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&source, &target_dir)?;

        Ok(target_dir)
    }

    // ========== Plugin Fetching ==========

    /// Fetch a plugin from a marketplace
    pub fn fetch_plugin(
        &self,
        marketplace: &Marketplace,
        marketplace_dir: &Path,
        plugin: &PluginEntry,
        version: &str,
    ) -> Result<PathBuf> {
        let target_dir = self
            .cache_dir
            .join(&marketplace.name)
            .join(&plugin.name)
            .join(version);

        // Remove existing if present
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        // Create parent directories
        fs::create_dir_all(&target_dir)?;

        match &plugin.source {
            PluginSource::Relative(rel_path) => {
                // Resolve relative path from marketplace directory
                let plugin_root = self.resolve_plugin_root(marketplace);
                let source_dir = if let Some(root) = plugin_root {
                    marketplace_dir
                        .join(root)
                        .join(rel_path.trim_start_matches("./"))
                } else {
                    marketplace_dir.join(rel_path.trim_start_matches("./"))
                };

                if !source_dir.exists() {
                    return Err(DotAgentError::FileNotFound { path: source_dir });
                }

                // Validate path doesn't escape marketplace directory (path traversal protection)
                let canonical_source = source_dir.canonicalize()?;
                let canonical_marketplace = marketplace_dir.canonicalize()?;
                if !canonical_source.starts_with(&canonical_marketplace) {
                    return Err(DotAgentError::PathTraversal {
                        path: source_dir,
                    });
                }

                // Copy plugin directory
                self.copy_dir_recursive(&source_dir, &target_dir)?;
            }
            PluginSource::Structured(structured) => {
                self.fetch_plugin_structured(structured, &target_dir)?;
            }
        }

        Ok(target_dir)
    }

    /// Resolve plugin root from marketplace metadata
    fn resolve_plugin_root(&self, marketplace: &Marketplace) -> Option<String> {
        marketplace
            .metadata
            .as_ref()
            .and_then(|m| m.plugin_root.clone())
    }

    /// Fetch plugin from structured source
    fn fetch_plugin_structured(&self, source: &StructuredSource, target_dir: &Path) -> Result<()> {
        match source.source.as_str() {
            "github" => {
                if let Some(repo) = &source.repo {
                    let url = format!("https://github.com/{}.git", repo);
                    let temp_dir = target_dir.with_extension("tmp");

                    // Clone to temp
                    self.git_clone_with_ref(&url, &temp_dir, source.r#ref.as_deref())?;

                    // Move contents to target
                    self.move_dir_contents(&temp_dir, target_dir)?;

                    // Remove temp
                    let _ = fs::remove_dir_all(&temp_dir);
                }
            }
            "url" => {
                if let Some(url) = &source.url {
                    let temp_dir = target_dir.with_extension("tmp");

                    // Clone to temp
                    self.git_clone_with_ref(url, &temp_dir, source.r#ref.as_deref())?;

                    // Move contents to target
                    self.move_dir_contents(&temp_dir, target_dir)?;

                    // Remove temp
                    let _ = fs::remove_dir_all(&temp_dir);
                }
            }
            _ => {
                return Err(DotAgentError::ConfigParseSimple {
                    message: format!("Unknown source type: {}", source.source),
                });
            }
        }

        Ok(())
    }

    // ========== Git Operations ==========

    /// Clone a git repository
    fn git_clone(&self, url: &str, target: &Path) -> Result<()> {
        fs::create_dir_all(target.parent().unwrap_or(target))?;

        let output = Command::new("git")
            .args(["clone", "--depth", "1", url])
            .arg(target)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DotAgentError::Git(format!("git clone failed: {}", stderr)));
        }

        Ok(())
    }

    /// Clone a git repository with optional ref
    fn git_clone_with_ref(&self, url: &str, target: &Path, r#ref: Option<&str>) -> Result<()> {
        fs::create_dir_all(target.parent().unwrap_or(target))?;

        let mut args = vec!["clone", "--depth", "1"];

        if let Some(git_ref) = r#ref {
            args.push("--branch");
            args.push(git_ref);
        }

        args.push(url);

        let output = Command::new("git").args(&args).arg(target).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DotAgentError::Git(format!("git clone failed: {}", stderr)));
        }

        Ok(())
    }

    // ========== File Operations ==========

    /// Copy directory recursively
    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<()> {
        #![allow(clippy::only_used_in_recursion)]
        fs::create_dir_all(dst)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                // Skip .git directory
                if entry.file_name() == ".git" {
                    continue;
                }
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }

    /// Move directory contents
    fn move_dir_contents(&self, src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst)?;

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            // Skip .git directory
            if entry.file_name() == ".git" {
                continue;
            }

            if src_path.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_fetcher() -> (PluginFetcher, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let fetcher = PluginFetcher::new(temp_dir.path());
        (fetcher, temp_dir)
    }

    #[test]
    fn test_cache_dir() {
        let (fetcher, temp) = create_test_fetcher();
        assert_eq!(fetcher.cache_dir(), temp.path().join("cache"));
    }

    #[test]
    fn test_marketplaces_dir() {
        let (fetcher, temp) = create_test_fetcher();
        assert_eq!(fetcher.marketplaces_dir(), temp.path().join("marketplaces"));
    }

    #[test]
    fn test_copy_dir_recursive() {
        let (fetcher, temp) = create_test_fetcher();

        // Create source structure
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("subdir")).unwrap();
        fs::write(src.join("file1.txt"), "content1").unwrap();
        fs::write(src.join("subdir/file2.txt"), "content2").unwrap();

        // Copy
        let dst = temp.path().join("dst");
        fetcher.copy_dir_recursive(&src, &dst).unwrap();

        // Verify
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());
        assert_eq!(
            fs::read_to_string(dst.join("file1.txt")).unwrap(),
            "content1"
        );
    }

    #[test]
    fn test_copy_dir_skips_git() {
        let (fetcher, temp) = create_test_fetcher();

        // Create source with .git
        let src = temp.path().join("src");
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git/config"), "git config").unwrap();
        fs::write(src.join("file.txt"), "content").unwrap();

        // Copy
        let dst = temp.path().join("dst");
        fetcher.copy_dir_recursive(&src, &dst).unwrap();

        // Verify .git is skipped
        assert!(!dst.join(".git").exists());
        assert!(dst.join("file.txt").exists());
    }

    #[test]
    fn test_fetch_marketplace_local() {
        let (fetcher, temp) = create_test_fetcher();

        // Create source marketplace
        let src = temp.path().join("my-marketplace");
        fs::create_dir_all(src.join(".claude-plugin")).unwrap();
        fs::write(
            src.join(".claude-plugin/marketplace.json"),
            r#"{"name": "test", "owner": {"name": "Test"}}"#,
        )
        .unwrap();

        // Create marketplaces directory
        fs::create_dir_all(fetcher.marketplaces_dir()).unwrap();

        // Fetch
        let result = fetcher.fetch_marketplace_local("test-market", src.to_str().unwrap());
        assert!(result.is_ok());

        let target = result.unwrap();
        // On unix, it should be a symlink
        #[cfg(unix)]
        assert!(target.is_symlink() || target.exists());
    }
}
