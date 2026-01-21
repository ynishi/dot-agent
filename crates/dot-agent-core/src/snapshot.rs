use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{DotAgentError, Result};
use crate::metadata::compute_hash;

const SNAPSHOTS_DIR: &str = "target-snapshots";
const MANIFEST_FILE: &str = "manifest.toml";

/// Directories to exclude from snapshots (Claude Code system directories)
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

/// Files to exclude from snapshots
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

fn should_exclude(path: &Path, base: &Path) -> bool {
    // Check filename
    if let Some(name) = path.file_name() {
        let name_str = name.to_string_lossy();
        if EXCLUDED_FILES.iter().any(|&f| name_str == f) {
            return true;
        }
        if name_str.starts_with(".dot-agent") {
            return true;
        }
    }

    // Check if path is inside excluded directory
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
    pub fn display_time(&self) -> String {
        self.timestamp
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotManifest {
    /// Original target path (for reference)
    target_path: String,
    /// When manifest was first created
    created_at: DateTime<Utc>,
    /// List of snapshots
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

pub struct SnapshotManager {
    base_dir: PathBuf,
}

impl SnapshotManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get snapshots directory for a specific target
    fn snapshots_dir(&self, target: &Path) -> PathBuf {
        let target_hash = compute_hash(target.to_string_lossy().as_bytes());
        let short_hash = &target_hash[..12];
        self.base_dir.join(SNAPSHOTS_DIR).join(short_hash)
    }

    /// Load manifest for a target
    fn load_manifest(&self, target: &Path) -> Result<SnapshotManifest> {
        let manifest_path = self.snapshots_dir(target).join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(SnapshotManifest::default());
        }
        let content = fs::read_to_string(&manifest_path)?;
        toml::from_str(&content).map_err(DotAgentError::TomlDe)
    }

    /// Save manifest for a target
    fn save_manifest(&self, target: &Path, manifest: &SnapshotManifest) -> Result<()> {
        let snapshots_dir = self.snapshots_dir(target);
        fs::create_dir_all(&snapshots_dir)?;
        let manifest_path = snapshots_dir.join(MANIFEST_FILE);
        let content = toml::to_string_pretty(manifest)?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    /// Save a snapshot of the target directory
    ///
    /// # Arguments
    /// * `target` - Target directory to snapshot
    /// * `trigger` - What triggered this snapshot
    /// * `message` - Optional description
    /// * `profiles_affected` - Profiles involved in this operation
    pub fn save(
        &self,
        target: &Path,
        trigger: SnapshotTrigger,
        message: Option<&str>,
        profiles_affected: &[String],
    ) -> Result<Snapshot> {
        if !target.exists() {
            return Err(DotAgentError::TargetNotFound {
                path: target.to_path_buf(),
            });
        }

        let timestamp = Utc::now();
        let id = timestamp.format("%Y%m%d_%H%M%S").to_string();

        let snapshot_dir = self.snapshots_dir(target).join(&id);
        fs::create_dir_all(&snapshot_dir)?;

        // Copy all files from target to snapshot (excluding system files)
        let mut file_count = 0;
        for entry in WalkDir::new(target)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();

            // Use should_exclude for comprehensive filtering
            if should_exclude(src, target) {
                continue;
            }

            if let Ok(relative) = src.strip_prefix(target) {
                let dst = snapshot_dir.join(relative);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(src, dst)?;
                file_count += 1;
            }
        }

        let snapshot = Snapshot {
            id: id.clone(),
            timestamp,
            trigger,
            message: message.map(String::from),
            profiles_affected: profiles_affected.to_vec(),
            file_count,
        };

        // Update manifest
        let mut manifest = self.load_manifest(target)?;
        // Set target_path if this is a new manifest
        if manifest.target_path.is_empty() {
            manifest.target_path = target.to_string_lossy().to_string();
        }
        manifest.snapshots.push(snapshot.clone());
        self.save_manifest(target, &manifest)?;

        Ok(snapshot)
    }

    /// Convenience method for manual snapshots (backward compatible)
    pub fn save_manual(&self, target: &Path, message: Option<&str>) -> Result<Snapshot> {
        self.save(target, SnapshotTrigger::Manual, message, &[])
    }

    /// List all snapshots for a target
    pub fn list(&self, target: &Path) -> Result<Vec<Snapshot>> {
        let manifest = self.load_manifest(target)?;
        Ok(manifest.snapshots)
    }

    /// Get a specific snapshot
    pub fn get(&self, target: &Path, id: &str) -> Result<Snapshot> {
        let manifest = self.load_manifest(target)?;
        manifest
            .snapshots
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| DotAgentError::SnapshotNotFound { id: id.to_string() })
    }

    /// Restore a snapshot
    pub fn restore(&self, target: &Path, id: &str) -> Result<(usize, usize)> {
        let _ = self.get(target, id)?; // Verify snapshot exists

        let snapshot_dir = self.snapshots_dir(target).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        // Clear target directory (except metadata)
        let mut removed = 0;
        if target.exists() {
            for entry in WalkDir::new(target)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let path = entry.path();
                if path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with(".dot-agent"))
                    .unwrap_or(false)
                {
                    continue;
                }
                fs::remove_file(path)?;
                removed += 1;
            }
            // Clean empty directories
            clean_empty_dirs(target)?;
        }

        // Copy files from snapshot to target
        fs::create_dir_all(target)?;
        let mut restored = 0;
        for entry in WalkDir::new(&snapshot_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();
            if let Ok(relative) = src.strip_prefix(&snapshot_dir) {
                let dst = target.join(relative);
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
    pub fn diff(&self, target: &Path, id: &str) -> Result<SnapshotDiff> {
        let _ = self.get(target, id)?;

        let snapshot_dir = self.snapshots_dir(target).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        let mut diff = SnapshotDiff::default();

        // Build file maps
        let snapshot_files = collect_files(&snapshot_dir)?;
        let current_files = if target.exists() {
            collect_files_filtered(target)?
        } else {
            HashMap::new()
        };

        // Check snapshot files against current
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

        // Check for new files
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
    pub fn delete(&self, target: &Path, id: &str) -> Result<()> {
        let _ = self.get(target, id)?;

        let snapshot_dir = self.snapshots_dir(target).join(id);
        if snapshot_dir.exists() {
            fs::remove_dir_all(&snapshot_dir)?;
        }

        // Update manifest
        let mut manifest = self.load_manifest(target)?;
        manifest.snapshots.retain(|s| s.id != id);
        self.save_manifest(target, &manifest)?;

        Ok(())
    }

    /// Prune old snapshots, keeping only the most recent `keep` snapshots
    ///
    /// Returns the number of deleted snapshots and their IDs
    pub fn prune(&self, target: &Path, keep: usize) -> Result<Vec<String>> {
        let mut manifest = self.load_manifest(target)?;

        if manifest.snapshots.len() <= keep {
            return Ok(vec![]);
        }

        // Sort by timestamp (oldest first)
        manifest.snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        // Calculate how many to remove
        let to_remove = manifest.snapshots.len() - keep;
        let removed: Vec<Snapshot> = manifest.snapshots.drain(..to_remove).collect();

        // Delete snapshot directories
        let mut deleted_ids = Vec::new();
        for snap in &removed {
            let snapshot_dir = self.snapshots_dir(target).join(&snap.id);
            if snapshot_dir.exists() {
                fs::remove_dir_all(&snapshot_dir)?;
            }
            deleted_ids.push(snap.id.clone());
        }

        // Save updated manifest
        self.save_manifest(target, &manifest)?;

        Ok(deleted_ids)
    }

    /// Get the latest snapshot for a target
    pub fn latest(&self, target: &Path) -> Result<Option<Snapshot>> {
        let manifest = self.load_manifest(target)?;
        Ok(manifest
            .snapshots
            .into_iter()
            .max_by(|a, b| a.timestamp.cmp(&b.timestamp)))
    }
}

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

fn collect_files_filtered(dir: &Path) -> Result<HashMap<String, String>> {
    let mut files = HashMap::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path();
        // Skip metadata files
        if path
            .file_name()
            .map(|n| n.to_string_lossy().starts_with(".dot-agent"))
            .unwrap_or(false)
        {
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

// ============================================================================
// Profile Snapshot Manager
// ============================================================================

const PROFILE_SNAPSHOTS_DIR: &str = "profile-snapshots";

/// Manages snapshots for profile source directories
pub struct ProfileSnapshotManager {
    base_dir: PathBuf,
}

impl ProfileSnapshotManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get snapshots directory for a specific profile
    fn snapshots_dir(&self, profile_name: &str) -> PathBuf {
        self.base_dir.join(PROFILE_SNAPSHOTS_DIR).join(profile_name)
    }

    /// Load manifest for a profile
    fn load_manifest(&self, profile_name: &str) -> Result<SnapshotManifest> {
        let manifest_path = self.snapshots_dir(profile_name).join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(SnapshotManifest::default());
        }
        let content = fs::read_to_string(&manifest_path)?;
        toml::from_str(&content).map_err(DotAgentError::TomlDe)
    }

    /// Save manifest for a profile
    fn save_manifest(&self, profile_name: &str, manifest: &SnapshotManifest) -> Result<()> {
        let snapshots_dir = self.snapshots_dir(profile_name);
        fs::create_dir_all(&snapshots_dir)?;
        let manifest_path = snapshots_dir.join(MANIFEST_FILE);
        let content = toml::to_string_pretty(manifest)?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    /// Save a snapshot of a profile
    pub fn save(
        &self,
        profile_name: &str,
        profile_path: &Path,
        message: Option<&str>,
    ) -> Result<Snapshot> {
        if !profile_path.exists() {
            return Err(DotAgentError::ProfileNotFound {
                name: profile_name.to_string(),
            });
        }

        let timestamp = Utc::now();
        let id = timestamp.format("%Y%m%d_%H%M%S").to_string();

        let snapshot_dir = self.snapshots_dir(profile_name).join(&id);
        fs::create_dir_all(&snapshot_dir)?;

        // Copy all files from profile to snapshot
        let mut file_count = 0;
        for entry in WalkDir::new(profile_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();

            // Skip hidden files and common excludes
            if let Some(name) = src.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str == ".DS_Store" {
                    continue;
                }
            }

            if let Ok(relative) = src.strip_prefix(profile_path) {
                let dst = snapshot_dir.join(relative);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(src, dst)?;
                file_count += 1;
            }
        }

        let snapshot = Snapshot {
            id: id.clone(),
            timestamp,
            trigger: SnapshotTrigger::Manual,
            message: message.map(String::from),
            profiles_affected: vec![profile_name.to_string()],
            file_count,
        };

        // Update manifest
        let mut manifest = self.load_manifest(profile_name)?;
        if manifest.target_path.is_empty() {
            manifest.target_path = profile_path.to_string_lossy().to_string();
        }
        manifest.snapshots.push(snapshot.clone());
        self.save_manifest(profile_name, &manifest)?;

        Ok(snapshot)
    }

    /// List all snapshots for a profile
    pub fn list(&self, profile_name: &str) -> Result<Vec<Snapshot>> {
        let manifest = self.load_manifest(profile_name)?;
        Ok(manifest.snapshots)
    }

    /// Get a specific snapshot
    pub fn get(&self, profile_name: &str, id: &str) -> Result<Snapshot> {
        let manifest = self.load_manifest(profile_name)?;
        manifest
            .snapshots
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| DotAgentError::SnapshotNotFound { id: id.to_string() })
    }

    /// Restore a snapshot to the profile directory
    pub fn restore(
        &self,
        profile_name: &str,
        profile_path: &Path,
        id: &str,
    ) -> Result<(usize, usize)> {
        let _ = self.get(profile_name, id)?;

        let snapshot_dir = self.snapshots_dir(profile_name).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        // Clear profile directory
        let mut removed = 0;
        if profile_path.exists() {
            for entry in WalkDir::new(profile_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
            {
                let path = entry.path();
                // Skip hidden files
                if path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
                {
                    continue;
                }
                fs::remove_file(path)?;
                removed += 1;
            }
            clean_empty_dirs(profile_path)?;
        }

        // Copy files from snapshot to profile
        fs::create_dir_all(profile_path)?;
        let mut restored = 0;
        for entry in WalkDir::new(&snapshot_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let src = entry.path();
            if let Ok(relative) = src.strip_prefix(&snapshot_dir) {
                let dst = profile_path.join(relative);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(src, dst)?;
                restored += 1;
            }
        }

        Ok((removed, restored))
    }

    /// Compare snapshot with current profile state
    pub fn diff(&self, profile_name: &str, profile_path: &Path, id: &str) -> Result<SnapshotDiff> {
        let _ = self.get(profile_name, id)?;

        let snapshot_dir = self.snapshots_dir(profile_name).join(id);
        if !snapshot_dir.exists() {
            return Err(DotAgentError::SnapshotNotFound { id: id.to_string() });
        }

        let mut diff = SnapshotDiff::default();

        let snapshot_files = collect_files(&snapshot_dir)?;
        let current_files = if profile_path.exists() {
            collect_files(profile_path)?
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
    pub fn delete(&self, profile_name: &str, id: &str) -> Result<()> {
        let _ = self.get(profile_name, id)?;

        let snapshot_dir = self.snapshots_dir(profile_name).join(id);
        if snapshot_dir.exists() {
            fs::remove_dir_all(&snapshot_dir)?;
        }

        let mut manifest = self.load_manifest(profile_name)?;
        manifest.snapshots.retain(|s| s.id != id);
        self.save_manifest(profile_name, &manifest)?;

        Ok(())
    }

    /// Prune old snapshots
    pub fn prune(&self, profile_name: &str, keep: usize) -> Result<Vec<String>> {
        let mut manifest = self.load_manifest(profile_name)?;

        if manifest.snapshots.len() <= keep {
            return Ok(vec![]);
        }

        manifest.snapshots.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let to_remove = manifest.snapshots.len() - keep;
        let removed: Vec<Snapshot> = manifest.snapshots.drain(..to_remove).collect();

        let mut deleted_ids = Vec::new();
        for snap in &removed {
            let snapshot_dir = self.snapshots_dir(profile_name).join(&snap.id);
            if snapshot_dir.exists() {
                fs::remove_dir_all(&snapshot_dir)?;
            }
            deleted_ids.push(snap.id.clone());
        }

        self.save_manifest(profile_name, &manifest)?;

        Ok(deleted_ids)
    }
}
