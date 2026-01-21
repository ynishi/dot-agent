use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{DotAgentError, Result};
use crate::metadata::{compute_file_hash, compute_hash, Metadata};
use crate::profile::{IgnoreConfig, Profile};

const CLAUDE_MD: &str = "CLAUDE.md";
const CLAUDE_DIR: &str = ".claude";

/// Callback type for file operation progress reporting
pub type FileCallback<'a> = Option<&'a dyn Fn(&str, &str)>;

// Directories where files should be prefixed with profile name
const PREFIXED_DIRS: &[&str] = &["agents", "commands", "rules"];
// Directories where subdirectories should be prefixed (skills has SKILL.md inside)
const PREFIXED_SUBDIRS: &[&str] = &["skills"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Unchanged,
    Modified,
    Added,
    Missing,
}

#[derive(Debug)]
pub struct FileInfo {
    pub relative_path: PathBuf,
    pub status: FileStatus,
}

#[derive(Debug, Default)]
pub struct InstallResult {
    pub installed: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

#[derive(Debug, Default)]
pub struct DiffResult {
    pub unchanged: usize,
    pub modified: usize,
    pub added: usize,
    pub missing: usize,
    pub files: Vec<FileInfo>,
}

pub struct Installer {
    base_dir: PathBuf,
}

impl Installer {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get target directory (either project/.claude or global ~/.claude)
    pub fn resolve_target(&self, target: Option<&Path>, global: bool) -> Result<PathBuf> {
        if global {
            let home = dirs::home_dir().ok_or(DotAgentError::HomeNotFound)?;
            Ok(home.join(".claude"))
        } else {
            let base = target
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            if !base.exists() {
                return Err(DotAgentError::TargetNotFound { path: base });
            }

            Ok(base.join(CLAUDE_DIR))
        }
    }

    /// Install a profile to target
    #[allow(clippy::too_many_arguments)]
    pub fn install(
        &self,
        profile: &Profile,
        target: &Path,
        force: bool,
        dry_run: bool,
        no_prefix: bool,
        ignore_config: &IgnoreConfig,
        on_file: FileCallback<'_>,
    ) -> Result<InstallResult> {
        let mut result = InstallResult::default();
        let mut metadata = Metadata::load(target)?.unwrap_or_else(|| Metadata::new(&self.base_dir));

        // Ensure target directory exists
        if !dry_run && !target.exists() {
            fs::create_dir_all(target)?;
        }

        let files = profile.list_files_with_config(ignore_config)?;

        for relative_path in files {
            let src = profile.path.join(&relative_path);
            let prefixed_path = if no_prefix {
                relative_path.clone()
            } else {
                prefix_path(&relative_path, &profile.name)
            };
            let dst = target.join(&prefixed_path);
            let relative_str = prefixed_path.to_string_lossy().to_string();

            let is_claude_md = relative_path.to_string_lossy() == CLAUDE_MD;
            let src_content = fs::read(&src)?;
            let src_hash = compute_hash(&src_content);

            if dst.exists() {
                let dst_hash = compute_file_hash(&dst)?;

                if src_hash == dst_hash {
                    // Same content - skip
                    if let Some(f) = on_file {
                        f("SKIP", &relative_str);
                    }
                    result.skipped += 1;
                    continue;
                }

                // CLAUDE.md is never overwritten
                if is_claude_md {
                    if let Some(f) = on_file {
                        f("WARN", &relative_str);
                    }
                    result.skipped += 1;
                    continue;
                }

                // Different content
                if !force {
                    if let Some(f) = on_file {
                        f("CONFLICT", &relative_str);
                    }
                    result.conflicts += 1;
                    continue;
                }
            }

            // Copy file
            if !dry_run {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&dst, &src_content)?;
                metadata.add_file(&relative_str, &src_hash);
            }

            if let Some(f) = on_file {
                f("OK", &relative_str);
            }
            result.installed += 1;
        }

        if !dry_run && result.conflicts == 0 {
            metadata.add_profile(&profile.name);
            metadata.save(target)?;
        }

        Ok(result)
    }

    /// Compare profile with installed files
    pub fn diff(
        &self,
        profile: &Profile,
        target: &Path,
        ignore_config: &IgnoreConfig,
    ) -> Result<DiffResult> {
        let mut result = DiffResult::default();

        if !target.exists() {
            // All files are missing
            for relative_path in profile.list_files_with_config(ignore_config)? {
                let prefixed_path = prefix_path(&relative_path, &profile.name);
                result.files.push(FileInfo {
                    relative_path: prefixed_path,
                    status: FileStatus::Missing,
                });
                result.missing += 1;
            }
            return Ok(result);
        }

        let metadata = Metadata::load(target)?;
        let profile_files = profile.list_files_with_config(ignore_config)?;

        // Build set of prefixed paths for comparison
        let prefixed_files: Vec<_> = profile_files
            .iter()
            .map(|p| prefix_path(p, &profile.name))
            .collect();

        // Check profile files against target
        for (idx, relative_path) in profile_files.iter().enumerate() {
            let src = profile.path.join(relative_path);
            let prefixed_path = &prefixed_files[idx];
            let dst = target.join(prefixed_path);

            if !dst.exists() {
                result.files.push(FileInfo {
                    relative_path: prefixed_path.clone(),
                    status: FileStatus::Missing,
                });
                result.missing += 1;
                continue;
            }

            let src_hash = compute_file_hash(&src)?;
            let dst_hash = compute_file_hash(&dst)?;

            if src_hash == dst_hash {
                result.files.push(FileInfo {
                    relative_path: prefixed_path.clone(),
                    status: FileStatus::Unchanged,
                });
                result.unchanged += 1;
            } else {
                result.files.push(FileInfo {
                    relative_path: prefixed_path.clone(),
                    status: FileStatus::Modified,
                });
                result.modified += 1;
            }
        }

        // Check for files in metadata that aren't in profile (user added)
        if let Some(meta) = &metadata {
            for file_path in meta.files.keys() {
                let path = PathBuf::from(file_path);
                if !prefixed_files.contains(&path) {
                    let full_path = target.join(&path);
                    if full_path.exists() {
                        result.files.push(FileInfo {
                            relative_path: path,
                            status: FileStatus::Added,
                        });
                        result.added += 1;
                    }
                }
            }
        }

        result
            .files
            .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(result)
    }

    /// Remove installed profile files
    pub fn remove(
        &self,
        profile: &Profile,
        target: &Path,
        force: bool,
        dry_run: bool,
        ignore_config: &IgnoreConfig,
        on_file: FileCallback<'_>,
    ) -> Result<(usize, usize)> {
        if !target.exists() {
            return Ok((0, 0));
        }

        let mut metadata = Metadata::load(target)?.unwrap_or_else(|| Metadata::new(&self.base_dir));
        let diff = self.diff(profile, target, ignore_config)?;

        // Check for local modifications
        if !force {
            let modified: Vec<_> = diff
                .files
                .iter()
                .filter(|f| f.status == FileStatus::Modified)
                .map(|f| f.relative_path.clone())
                .collect();

            if !modified.is_empty() {
                return Err(DotAgentError::LocalModifications { paths: modified });
            }
        }

        let mut removed = 0;
        let mut kept = 0;

        for file_info in &diff.files {
            let dst = target.join(&file_info.relative_path);
            let relative_str = file_info.relative_path.to_string_lossy().to_string();

            // Never remove CLAUDE.md
            if relative_str == CLAUDE_MD {
                if let Some(f) = on_file {
                    f("KEEP", &relative_str);
                }
                kept += 1;
                continue;
            }

            // Skip user-added files
            if file_info.status == FileStatus::Added {
                if let Some(f) = on_file {
                    f("KEEP", &relative_str);
                }
                kept += 1;
                continue;
            }

            // Skip missing files
            if file_info.status == FileStatus::Missing {
                continue;
            }

            // Remove file
            if !dry_run && dst.exists() {
                fs::remove_file(&dst)?;
                metadata.remove_file(&relative_str);

                // Remove empty parent directories
                if let Some(parent) = dst.parent() {
                    let _ = remove_empty_dirs(parent, target);
                }
            }

            if let Some(f) = on_file {
                f("DEL", &relative_str);
            }
            removed += 1;
        }

        if !dry_run {
            metadata.remove_profile(&profile.name);
            if metadata.installed.profiles.is_empty() && metadata.files.is_empty() {
                // Remove metadata file if no profiles left
                let meta_path = target.join(".dot-agent-meta.toml");
                let _ = fs::remove_file(meta_path);
            } else {
                metadata.save(target)?;
            }
        }

        Ok((removed, kept))
    }

    /// Upgrade profile files
    #[allow(clippy::too_many_arguments)]
    pub fn upgrade(
        &self,
        profile: &Profile,
        target: &Path,
        force: bool,
        dry_run: bool,
        no_prefix: bool,
        ignore_config: &IgnoreConfig,
        on_file: FileCallback<'_>,
    ) -> Result<(usize, usize, usize, usize)> {
        // updated, new, skipped, unchanged
        if !target.exists() {
            // Just install everything
            let result =
                self.install(profile, target, force, dry_run, no_prefix, ignore_config, on_file)?;
            return Ok((0, result.installed, 0, 0));
        }

        let mut metadata = Metadata::load(target)?.unwrap_or_else(|| Metadata::new(&self.base_dir));
        let mut updated = 0;
        let mut new = 0;
        let mut skipped = 0;
        let mut unchanged = 0;

        let files = profile.list_files_with_config(ignore_config)?;

        for relative_path in files {
            let src = profile.path.join(&relative_path);
            let prefixed_path = if no_prefix {
                relative_path.clone()
            } else {
                prefix_path(&relative_path, &profile.name)
            };
            let dst = target.join(&prefixed_path);
            let relative_str = prefixed_path.to_string_lossy().to_string();
            let is_claude_md = relative_path.to_string_lossy() == CLAUDE_MD;

            let src_content = fs::read(&src)?;
            let src_hash = compute_hash(&src_content);

            if !dst.exists() {
                // New file
                if !dry_run {
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&dst, &src_content)?;
                    metadata.add_file(&relative_str, &src_hash);
                }
                if let Some(f) = on_file {
                    f("NEW", &relative_str);
                }
                new += 1;
                continue;
            }

            let dst_hash = compute_file_hash(&dst)?;

            if src_hash == dst_hash {
                if let Some(f) = on_file {
                    f("OK", &relative_str);
                }
                unchanged += 1;
                continue;
            }

            // CLAUDE.md is never overwritten
            if is_claude_md {
                if let Some(f) = on_file {
                    f("WARN", &relative_str);
                }
                skipped += 1;
                continue;
            }

            // Check if file was modified locally
            let original_hash = metadata.get_file_hash(&relative_str);
            let locally_modified = original_hash.map(|h| h != &dst_hash).unwrap_or(false);

            if locally_modified && !force {
                if let Some(f) = on_file {
                    f("SKIP", &relative_str);
                }
                skipped += 1;
                continue;
            }

            // Update file
            if !dry_run {
                fs::write(&dst, &src_content)?;
                metadata.add_file(&relative_str, &src_hash);
            }
            if let Some(f) = on_file {
                f("UPDATE", &relative_str);
            }
            updated += 1;
        }

        if !dry_run {
            metadata.add_profile(&profile.name);
            metadata.save(target)?;
        }

        Ok((updated, new, skipped, unchanged))
    }
}

fn remove_empty_dirs(dir: &Path, root: &Path) -> std::io::Result<()> {
    if dir == root {
        return Ok(());
    }

    if dir.is_dir() && fs::read_dir(dir)?.next().is_none() {
        fs::remove_dir(dir)?;
        if let Some(parent) = dir.parent() {
            remove_empty_dirs(parent, root)?;
        }
    }

    Ok(())
}

/// Transform relative path to add profile prefix where needed
/// Examples:
///   agents/code-reviewer.md → agents/{profile}-code-reviewer.md
///   skills/my-skill/SKILL.md → skills/{profile}-my-skill/SKILL.md
///   rules/testing.md → rules/{profile}-testing.md
///   commands/profile:cmd.md → commands/profile:cmd.md (already prefixed)
///   CLAUDE.md → CLAUDE.md (no change)
fn prefix_path(relative_path: &Path, profile_name: &str) -> PathBuf {
    let components: Vec<_> = relative_path.components().collect();

    if components.is_empty() {
        return relative_path.to_path_buf();
    }

    // Get first component (top-level directory)
    let first = components[0].as_os_str().to_string_lossy();

    // Check if this is a directory where we prefix files directly
    if PREFIXED_DIRS.contains(&first.as_ref()) && components.len() >= 2 {
        let filename = components[1].as_os_str().to_string_lossy();

        // Skip if already prefixed (contains ':' or starts with profile name)
        if filename.contains(':') || filename.starts_with(&format!("{}-", profile_name)) {
            return relative_path.to_path_buf();
        }

        // agents/code-reviewer.md → agents/{profile}-code-reviewer.md
        let mut result = PathBuf::from(components[0].as_os_str());
        result.push(format!("{}-{}", profile_name, filename));

        // Add remaining components if any
        for comp in &components[2..] {
            result.push(comp.as_os_str());
        }
        return result;
    }

    // Check if this is a directory where we prefix subdirectories
    if PREFIXED_SUBDIRS.contains(&first.as_ref()) && components.len() >= 2 {
        let subdir = components[1].as_os_str().to_string_lossy();

        // Skip if already prefixed
        if subdir.contains(':') || subdir.starts_with(&format!("{}-", profile_name)) {
            return relative_path.to_path_buf();
        }

        // skills/my-skill/SKILL.md → skills/{profile}-my-skill/SKILL.md
        let mut result = PathBuf::from(components[0].as_os_str());
        result.push(format!("{}-{}", profile_name, subdir));

        // Add remaining components
        for comp in &components[2..] {
            result.push(comp.as_os_str());
        }
        return result;
    }

    // No transformation needed
    relative_path.to_path_buf()
}
