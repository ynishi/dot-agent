use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{DotAgentError, Result};

use super::metadata::compute_hash;

const TARGET_SNAPSHOTS_DIR: &str = "target-snapshots";
const PROFILE_SNAPSHOTS_DIR: &str = "profile-snapshots";
const MANIFEST_FILE: &str = "manifest.toml";

/// Directories to exclude from target snapshots (Claude Code system directories)
const EXCLUDED_DIRS: &[&str] = &[
    "debug",
    "file-history",
    "paste-cache",
    "cache",
    "backups",
    "plans",
    "ide",
    "data",
    "config",
    "projects",
    "todos",
    ".claude",
    "_bk",
];

/// Files to exclude from target snapshots
const EXCLUDED_FILES: &[&str] = &[
    ".DS_Store",
    "history.jsonl",
    "__store.db",
    "config.json",
    ".dot-agent-meta.toml",
];

/// Trigger that caused the snapshot to be created
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotTrigger {
    /// Automatically created before install operation
    PreInstall,
    /// Automatically created before uninstall operation
    PreUninstall,
    /// Automatically created before update operation
    PreUpdate,
    /// Manually created by user
    Manual,
}

impl SnapshotTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreInstall => "pre-install",
            Self::PreUninstall => "pre-uninstall",
            Self::PreUpdate => "pre-update",
            Self::Manual => "manual",
        }
    }
}

impl std::fmt::Display for SnapshotTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Snapshot Target Trait
// ============================================================================

/// Trait for types that can be snapshot targets
pub trait SnapshotTarget {
    /// Get the storage directory for snapshots of this target
    fn storage_dir(&self, base_dir: &Path) -> PathBuf;

    /// Get the content path to snapshot
    fn content_path(&self) -> &Path;

    /// Get identifier for display/messages
    fn identifier(&self) -> String;

    /// Check if a file should be excluded from snapshot
    fn should_exclude(&self, path: &Path, base: &Path) -> bool;

    /// Error to return when source doesn't exist
    fn not_found_error(&self) -> DotAgentError;
}

/// Target directory snapshot (e.g., ~/.claude or project/.claude)
#[derive(Debug, Clone)]
pub struct TargetDir {
    path: PathBuf,
}

impl TargetDir {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SnapshotTarget for TargetDir {
    fn storage_dir(&self, base_dir: &Path) -> PathBuf {
        let hash = compute_hash(self.path.to_string_lossy().as_bytes());
        let short_hash = &hash[..12];
        base_dir.join(TARGET_SNAPSHOTS_DIR).join(short_hash)
    }

    fn content_path(&self) -> &Path {
        &self.path
    }

    fn identifier(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    fn should_exclude(&self, path: &Path, base: &Path) -> bool {
        should_exclude_target(path, base)
    }

    fn not_found_error(&self) -> DotAgentError {
        DotAgentError::TargetNotFound {
            path: self.path.clone(),
        }
    }
}

/// Profile source directory snapshot
#[derive(Debug, Clone)]
pub struct ProfileDir {
    name: String,
    path: PathBuf,
}

impl ProfileDir {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }
}

impl SnapshotTarget for ProfileDir {
    fn storage_dir(&self, base_dir: &Path) -> PathBuf {
        base_dir.join(PROFILE_SNAPSHOTS_DIR).join(&self.name)
    }

    fn content_path(&self) -> &Path {
        &self.path
    }

    fn identifier(&self) -> String {
        self.name.clone()
    }

    fn should_exclude(&self, path: &Path, _base: &Path) -> bool {
        should_exclude_profile(path)
    }

    fn not_found_error(&self) -> DotAgentError {
        DotAgentError::ProfileNotFound {
            name: self.name.clone(),
        }
    }
}

// ============================================================================
// Exclusion Filters
// ============================================================================

fn should_exclude_target(path: &Path, base: &Path) -> bool {
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if EXCLUDED_FILES.iter().any(|&f| name_str == f) {
            return true;
        }
        if name_str.starts_with(".dot-agent") {
            return true;
        }
    }

    if let Ok(relative) = path.strip_prefix(base) {
        if let Some(first_component) = relative.components().next() {
            let first = first_component.as_os_str().to_string_lossy();
            if EXCLUDED_DIRS.iter().any(|&d| first == d) {
                return true;
            }
        }
    }

    false
}

fn should_exclude_profile(path: &Path) -> bool {
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || name_str == ".DS_Store" {
            return true;
        }
    }
    false
}

// ============================================================================
// Snapshot Data Structures
// ============================================================================

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub trigger: SnapshotTrigger,
    pub message: Option<String>,
    pub profiles_affected: Vec<String>,
    pub file_count: usize,
}

impl Snapshot {
    /// Format timestamp for display in local timezone
    pub fn display_time(&self) -> String {
        self.timestamp
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
}

/// Manifest storing list of snapshots for a target
#[derive(Debug, Serialize, Deserialize)]
struct SnapshotManifest {
    target_path: String,
    created_at: DateTime<Utc>,
    snapshots: Vec<Snapshot>,
}

impl Default for SnapshotManifest {
    fn default() -> Self {
        Self {
            target_path: String::new(),
            created_at: Utc::now(),
            snapshots: Vec::new(),
        }
    }
}

/// Differences between a snapshot and current state
#[derive(Debug, Default)]
pub struct SnapshotDiff {
    pub unchanged: Vec<String>,
    pub modified: Vec<String>,
    pub added: Vec<String>,
    pub deleted: Vec<String>,
}

impl SnapshotDiff {
    pub fn has_changes(&self) -> bool {
        !self.modified.is_empty() || !self.added.is_empty() || !self.deleted.is_empty()
    }
}

// ============================================================================
// Generic Snapshot Manager
// ============================================================================

/// Generic snapshot manager that works with any SnapshotTarget
pub struct GenericSnapshotManager<T: SnapshotTarget> {
    base_dir: PathBuf,
    _marker: PhantomData<T>,
}

impl<T: SnapshotTarget> GenericSnapshotManager<T> {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            _marker: PhantomData,
        }
    }

    fn load_manifest(&self, target: &T) -> Result<SnapshotManifest> {
        let manifest_path = target.storage_dir(&self.base_dir).join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(SnapshotManifest::default());
        }
        let content = fs::read_to_string(&manifest_path)?;
        toml::from_str(&content).map_err(DotAgentError::TomlDe)
    }

    fn save_manifest(&self, target: &T, manifest: &SnapshotManifest) -> Result<()> {
        let storage_dir = target.storage_dir(&self.base_dir);
        fs::create_dir_all(&storage_dir)?;
        let manifest_path = storage_dir.join(MANIFEST_FILE);
        let content = toml::to_string_pretty(manifest)?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    /// Save a snapshot of the target
    pub fn save(
        &self,
        target: &T,
        trigger: SnapshotTrigger,
        message: Option<&str>,
        profiles_affected: &[String],
    ) -> Result<Snapshot> {
        let content_path = target.content_path();
        if !content_path.exists() {
            return Err(target.not_found_error());
        }

        let timestamp = Utc::now();
        let id = timestamp.format("%Y%m%d_%H%M%S").to_string();

        let snapshot_dir = target.storage_dir(&self.base_dir).join(&id);
        fs::create_dir_all(&snapshot_dir)?;

        // Copy files from content to snapshot
        let mut file_count = 0;
        for entry in WalkDir::new(content_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();

            if target.should_exclude(src, content_path) {
                continue;
            }

            if let Ok(relative) = src.strip_prefix(content_path) {
                let dst = snapshot_dir.join(relative);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(src, dst)?;
                file_count += 1;
            }
        }

        let snapshot = Snapshot {
            id,
            timestamp,
            trigger,
            message: message.map(String::from),
            profiles_affected: profiles_affected.to_vec(),
            file_count,
        };

        // Update manifest
        let mut manifest = self.load_manifest(target)?;
        if manifest.target_path.is_empty() {
            manifest.target_path = target.identifier();
        }
        manifest.snapshots.push(snapshot.clone());
        self.save_manifest(target, &manifest)?;

        Ok(snapshot)
    }

    /// List all snapshots for a target
    pub fn list(&self, target: &T) -> Result<Vec<Snapshot>> {
        let manifest = self.load_manifest(target)?;
        Ok(manifest.snapshots)
    }

    /// Get a specific snapshot
    pub fn get(&self, target: &T, id: &str) -> Result<Snapshot> {
        let manifest = self.load_manifest(target)?;
        manifest
            .snapshots
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| DotAgentError::SnapshotNotFound { id: id.to_string() })
    }

    /// Restore a snapshot
    pub fn restore(&self, target: &T, id: &str) -> Result<(usize, usize)> {
        let _ = self.get(target, id)?;

        let snapshot_dir = target.storage_dir(&self.base_dir).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        let content_path = target.content_path();

        // Clear content directory
        let mut removed = 0;
        if content_path.exists() {
            for entry in WalkDir::new(content_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let path = entry.path();
                if target.should_exclude(path, content_path) {
                    continue;
                }
                fs::remove_file(path)?;
                removed += 1;
            }
            clean_empty_dirs(content_path)?;
        }

        // Copy files from snapshot to content
        fs::create_dir_all(content_path)?;
        let mut restored = 0;
        for entry in WalkDir::new(&snapshot_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();
            if let Ok(relative) = src.strip_prefix(&snapshot_dir) {
                let dst = content_path.join(relative);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(src, dst)?;
                restored += 1;
            }
        }

        Ok((removed, restored))
    }

    /// Compare snapshot with current state
    pub fn diff(&self, target: &T, id: &str) -> Result<SnapshotDiff> {
        let _ = self.get(target, id)?;

        let snapshot_dir = target.storage_dir(&self.base_dir).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        let content_path = target.content_path();
        let mut diff = SnapshotDiff::default();

        let snapshot_files = collect_files(&snapshot_dir)?;
        let current_files = if content_path.exists() {
            collect_files_filtered(content_path, |p| target.should_exclude(p, content_path))?
        } else {
            HashMap::new()
        };

        for (path, snap_hash) in &snapshot_files {
            match current_files.get(path) {
                Some(curr_hash) if curr_hash == snap_hash => {
                    diff.unchanged.push(path.clone());
                }
                Some(_) => {
                    diff.modified.push(path.clone());
                }
                None => {
                    diff.deleted.push(path.clone());
                }
            }
        }

        for path in current_files.keys() {
            if !snapshot_files.contains_key(path) {
                diff.added.push(path.clone());
            }
        }

        diff.unchanged.sort();
        diff.modified.sort();
        diff.added.sort();
        diff.deleted.sort();

        Ok(diff)
    }

    /// Delete a snapshot
    pub fn delete(&self, target: &T, id: &str) -> Result<()> {
        let _ = self.get(target, id)?;

        let snapshot_dir = target.storage_dir(&self.base_dir).join(id);
        if snapshot_dir.exists() {
            fs::remove_dir_all(&snapshot_dir)?;
        }

        let mut manifest = self.load_manifest(target)?;
        manifest.snapshots.retain(|s| s.id != id);
        self.save_manifest(target, &manifest)?;

        Ok(())
    }

    /// Prune old snapshots, keeping only the most recent `keep` snapshots
    pub fn prune(&self, target: &T, keep: usize) -> Result<Vec<String>> {
        let mut manifest = self.load_manifest(target)?;

        if manifest.snapshots.len() <= keep {
            return Ok(vec![]);
        }

        manifest
            .snapshots
            .sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let to_remove = manifest.snapshots.len() - keep;
        let removed: Vec<Snapshot> = manifest.snapshots.drain(..to_remove).collect();

        let mut deleted_ids = Vec::new();
        for snap in &removed {
            let snapshot_dir = target.storage_dir(&self.base_dir).join(&snap.id);
            if snapshot_dir.exists() {
                fs::remove_dir_all(&snapshot_dir)?;
            }
            deleted_ids.push(snap.id.clone());
        }

        self.save_manifest(target, &manifest)?;

        Ok(deleted_ids)
    }

    /// Get the latest snapshot
    pub fn latest(&self, target: &T) -> Result<Option<Snapshot>> {
        let manifest = self.load_manifest(target)?;
        Ok(manifest
            .snapshots
            .into_iter()
            .max_by(|a, b| a.timestamp.cmp(&b.timestamp)))
    }
}

// ============================================================================
// Type Aliases for Backward Compatibility
// ============================================================================

/// Snapshot manager for target directories (e.g., installed .claude folders)
pub type SnapshotManager = GenericSnapshotManager<TargetDir>;

/// Snapshot manager for profile source directories
pub type ProfileSnapshotManager = GenericSnapshotManager<ProfileDir>;

// ============================================================================
// Convenience Methods for SnapshotManager
// ============================================================================

impl SnapshotManager {
    /// Save a snapshot of a target directory
    pub fn save_target(
        &self,
        target: &Path,
        trigger: SnapshotTrigger,
        message: Option<&str>,
        profiles_affected: &[String],
    ) -> Result<Snapshot> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.save(&target_dir, trigger, message, profiles_affected)
    }

    /// List snapshots for a target directory
    pub fn list_target(&self, target: &Path) -> Result<Vec<Snapshot>> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.list(&target_dir)
    }

    /// Get a specific snapshot for a target directory
    pub fn get_target(&self, target: &Path, id: &str) -> Result<Snapshot> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.get(&target_dir, id)
    }

    /// Restore a snapshot to a target directory
    pub fn restore_target(&self, target: &Path, id: &str) -> Result<(usize, usize)> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.restore(&target_dir, id)
    }

    /// Compare snapshot with current target state
    pub fn diff_target(&self, target: &Path, id: &str) -> Result<SnapshotDiff> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.diff(&target_dir, id)
    }

    /// Delete a snapshot for a target directory
    pub fn delete_target(&self, target: &Path, id: &str) -> Result<()> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.delete(&target_dir, id)
    }

    /// Prune old snapshots for a target directory
    pub fn prune_target(&self, target: &Path, keep: usize) -> Result<Vec<String>> {
        let target_dir = TargetDir::new(target.to_path_buf());
        self.prune(&target_dir, keep)
    }
}

// ============================================================================
// Convenience Methods for ProfileSnapshotManager
// ============================================================================

impl ProfileSnapshotManager {
    /// Save a snapshot of a profile
    pub fn save_profile(
        &self,
        profile_name: &str,
        profile_path: &Path,
        message: Option<&str>,
    ) -> Result<Snapshot> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), profile_path.to_path_buf());
        self.save(
            &profile_dir,
            SnapshotTrigger::Manual,
            message,
            &[profile_name.to_string()],
        )
    }

    /// List snapshots for a profile
    pub fn list_profile(&self, profile_name: &str) -> Result<Vec<Snapshot>> {
        // Use empty path for listing - we only need the name for storage_dir
        let profile_dir = ProfileDir::new(profile_name.to_string(), PathBuf::new());
        self.list(&profile_dir)
    }

    /// Get a specific snapshot for a profile
    pub fn get_profile(&self, profile_name: &str, id: &str) -> Result<Snapshot> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), PathBuf::new());
        self.get(&profile_dir, id)
    }

    /// Restore a snapshot to a profile directory
    pub fn restore_profile(
        &self,
        profile_name: &str,
        profile_path: &Path,
        id: &str,
    ) -> Result<(usize, usize)> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), profile_path.to_path_buf());
        self.restore(&profile_dir, id)
    }

    /// Compare snapshot with current profile state
    pub fn diff_profile(
        &self,
        profile_name: &str,
        profile_path: &Path,
        id: &str,
    ) -> Result<SnapshotDiff> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), profile_path.to_path_buf());
        self.diff(&profile_dir, id)
    }

    /// Delete a snapshot for a profile
    pub fn delete_profile(&self, profile_name: &str, id: &str) -> Result<()> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), PathBuf::new());
        self.delete(&profile_dir, id)
    }

    /// Prune old snapshots for a profile
    pub fn prune_profile(&self, profile_name: &str, keep: usize) -> Result<Vec<String>> {
        let profile_dir = ProfileDir::new(profile_name.to_string(), PathBuf::new());
        self.prune(&profile_dir, keep)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn collect_files(dir: &Path) -> Result<HashMap<String, String>> {
    let mut files = HashMap::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        if let Ok(relative) = path.strip_prefix(dir) {
            let content = fs::read(path)?;
            let hash = compute_hash(&content);
            files.insert(relative.to_string_lossy().to_string(), hash);
        }
    }
    Ok(files)
}

fn collect_files_filtered<F>(dir: &Path, should_exclude: F) -> Result<HashMap<String, String>>
where
    F: Fn(&Path) -> bool,
{
    let mut files = HashMap::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        if should_exclude(path) {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(dir) {
            let content = fs::read(path)?;
            let hash = compute_hash(&content);
            files.insert(relative.to_string_lossy().to_string(), hash);
        }
    }
    Ok(files)
}

fn clean_empty_dirs(dir: &Path) -> Result<()> {
    for entry in WalkDir::new(dir)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
    {
        let path = entry.path();
        if path != dir && fs::read_dir(path)?.next().is_none() {
            fs::remove_dir(path)?;
        }
    }
    Ok(())
}
