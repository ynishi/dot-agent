use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::error::Result;

/// Type of change in a delta entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaType {
    /// File was added
    Added,
    /// File was modified
    Modified,
    /// File was deleted (tombstone)
    Deleted,
}

/// A single file change in the delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaEntry {
    /// Relative path from target root
    pub path: String,
    /// Type of change
    pub delta_type: DeltaType,
    /// SHA256 hash of the new content (None for deleted)
    pub hash: Option<String>,
    /// Size in bytes (None for deleted)
    pub size: Option<u64>,
}

impl DeltaEntry {
    pub fn added(path: impl Into<String>, hash: String, size: u64) -> Self {
        Self {
            path: path.into(),
            delta_type: DeltaType::Added,
            hash: Some(hash),
            size: Some(size),
        }
    }

    pub fn modified(path: impl Into<String>, hash: String, size: u64) -> Self {
        Self {
            path: path.into(),
            delta_type: DeltaType::Modified,
            hash: Some(hash),
            size: Some(size),
        }
    }

    pub fn deleted(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            delta_type: DeltaType::Deleted,
            hash: None,
            size: None,
        }
    }
}

/// Delta between two states (incremental snapshot)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Delta {
    /// Entries in this delta
    pub entries: Vec<DeltaEntry>,
    /// Reference to base checkpoint (None for full snapshot)
    pub base_checkpoint: Option<String>,
    /// Whether this is a full snapshot
    pub is_full: bool,
}

impl Delta {
    pub fn new_incremental(base_checkpoint: String) -> Self {
        Self {
            entries: Vec::new(),
            base_checkpoint: Some(base_checkpoint),
            is_full: false,
        }
    }

    pub fn new_full() -> Self {
        Self {
            entries: Vec::new(),
            base_checkpoint: None,
            is_full: true,
        }
    }

    pub fn add_entry(&mut self, entry: DeltaEntry) {
        self.entries.push(entry);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn added_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.delta_type == DeltaType::Added)
            .count()
    }

    pub fn modified_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.delta_type == DeltaType::Modified)
            .count()
    }

    pub fn deleted_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.delta_type == DeltaType::Deleted)
            .count()
    }
}

/// Compute SHA256 hash of a file
pub fn compute_file_hash(path: &Path) -> io::Result<String> {
    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute delta between two directory states
pub fn compute_delta(base_files: &HashMap<String, String>, current_dir: &Path) -> Result<Delta> {
    let mut delta = if base_files.is_empty() {
        Delta::new_full()
    } else {
        Delta::new_incremental(String::new())
    };

    let mut current_files = HashMap::new();

    // Walk current directory
    if current_dir.exists() {
        walk_directory(current_dir, current_dir, &mut current_files)?;
    }

    // Find added and modified files
    for (path, hash) in &current_files {
        match base_files.get(path) {
            None => {
                // Added
                let full_path = current_dir.join(path);
                let size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
                delta.add_entry(DeltaEntry::added(path, hash.clone(), size));
            }
            Some(base_hash) if base_hash != hash => {
                // Modified
                let full_path = current_dir.join(path);
                let size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
                delta.add_entry(DeltaEntry::modified(path, hash.clone(), size));
            }
            _ => {
                // Unchanged
            }
        }
    }

    // Find deleted files
    for path in base_files.keys() {
        if !current_files.contains_key(path) {
            delta.add_entry(DeltaEntry::deleted(path));
        }
    }

    Ok(delta)
}

/// Walk directory and collect file hashes
fn walk_directory(root: &Path, current: &Path, files: &mut HashMap<String, String>) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_directory(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;
            let relative_str = relative.to_string_lossy().to_string();

            if let Ok(hash) = compute_file_hash(&path) {
                files.insert(relative_str, hash);
            }
        }
    }

    Ok(())
}

/// Save delta files to a checkpoint directory
pub fn save_delta_files(delta: &Delta, source_dir: &Path, checkpoint_dir: &Path) -> Result<()> {
    let delta_dir = checkpoint_dir.join("delta");
    fs::create_dir_all(&delta_dir)?;

    for entry in &delta.entries {
        match entry.delta_type {
            DeltaType::Added | DeltaType::Modified => {
                let source_path = source_dir.join(&entry.path);
                let dest_path = delta_dir.join(&entry.path);

                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                if source_path.exists() {
                    fs::copy(&source_path, &dest_path)?;
                }
            }
            DeltaType::Deleted => {
                // Just record in manifest, no file to copy
            }
        }
    }

    // Save delta manifest
    let manifest_path = checkpoint_dir.join("delta.toml");
    let manifest_content = toml::to_string_pretty(delta)
        .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;
    fs::write(manifest_path, manifest_content)?;

    Ok(())
}

/// Load delta from checkpoint directory
pub fn load_delta(checkpoint_dir: &Path) -> Result<Delta> {
    let manifest_path = checkpoint_dir.join("delta.toml");
    let content = fs::read_to_string(manifest_path)?;
    let delta: Delta = toml::from_str(&content)
        .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;
    Ok(delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn delta_entry_creation() {
        let added = DeltaEntry::added("file.txt", "abc123".into(), 100);
        assert_eq!(added.delta_type, DeltaType::Added);
        assert!(added.hash.is_some());

        let deleted = DeltaEntry::deleted("old.txt");
        assert_eq!(deleted.delta_type, DeltaType::Deleted);
        assert!(deleted.hash.is_none());
    }

    #[test]
    fn delta_counts() {
        let mut delta = Delta::new_incremental("cp-001".into());
        delta.add_entry(DeltaEntry::added("a.txt", "hash1".into(), 10));
        delta.add_entry(DeltaEntry::added("b.txt", "hash2".into(), 20));
        delta.add_entry(DeltaEntry::modified("c.txt", "hash3".into(), 30));
        delta.add_entry(DeltaEntry::deleted("d.txt"));

        assert_eq!(delta.added_count(), 2);
        assert_eq!(delta.modified_count(), 1);
        assert_eq!(delta.deleted_count(), 1);
    }

    #[test]
    fn compute_delta_empty_base() -> Result<()> {
        let temp = TempDir::new()?;
        let dir = temp.path();

        fs::write(dir.join("file.txt"), "content")?;

        let base_files = HashMap::new();
        let delta = compute_delta(&base_files, dir)?;

        assert!(delta.is_full);
        assert_eq!(delta.added_count(), 1);

        Ok(())
    }

    #[test]
    fn compute_delta_with_changes() -> Result<()> {
        let temp = TempDir::new()?;
        let dir = temp.path();

        // Current state
        fs::write(dir.join("new.txt"), "new content")?;
        fs::write(dir.join("modified.txt"), "modified content")?;

        // Base state (simulated)
        let mut base_files = HashMap::new();
        base_files.insert("modified.txt".into(), "old_hash".into());
        base_files.insert("deleted.txt".into(), "deleted_hash".into());

        let delta = compute_delta(&base_files, dir)?;

        assert_eq!(delta.added_count(), 1);
        assert_eq!(delta.modified_count(), 1);
        assert_eq!(delta.deleted_count(), 1);

        Ok(())
    }
}
