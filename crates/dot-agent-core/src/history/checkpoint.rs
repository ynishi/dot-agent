use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::delta::{compute_delta, load_delta, save_delta_files, Delta};
use crate::error::Result;

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID
    pub id: String,
    /// Associated operation ID
    pub operation_id: String,
    /// When the checkpoint was created
    pub created_at: DateTime<Utc>,
    /// Whether this is a full snapshot
    pub is_full: bool,
    /// Base checkpoint ID for incremental (None for full)
    pub base_checkpoint: Option<String>,
    /// File hashes at this checkpoint (path -> sha256)
    pub file_hashes: HashMap<String, String>,
    /// Total files count
    pub file_count: usize,
    /// Total size in bytes
    pub total_size: u64,
}

impl Checkpoint {
    pub fn new(id: String, operation_id: String, is_full: bool) -> Self {
        Self {
            id,
            operation_id,
            created_at: Utc::now(),
            is_full,
            base_checkpoint: None,
            file_hashes: HashMap::new(),
            file_count: 0,
            total_size: 0,
        }
    }

    pub fn with_base(mut self, base_id: String) -> Self {
        self.base_checkpoint = Some(base_id);
        self
    }

    pub fn with_hashes(mut self, hashes: HashMap<String, String>) -> Self {
        self.file_count = hashes.len();
        self.file_hashes = hashes;
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.total_size = size;
        self
    }
}

/// Manages checkpoints storage
pub struct CheckpointManager {
    /// Base directory for checkpoints
    base_dir: PathBuf,
    /// Manifest of all checkpoints
    manifest: CheckpointManifest,
    /// Number of operations between full snapshots
    full_snapshot_interval: usize,
}

/// Manifest tracking all checkpoints
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub version: u32,
    pub checkpoints: Vec<CheckpointEntry>,
    pub latest_full: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointEntry {
    pub id: String,
    pub operation_id: String,
    pub created_at: DateTime<Utc>,
    pub is_full: bool,
}

impl CheckpointManager {
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir)?;

        let manifest_path = base_dir.join("manifest.toml");
        let manifest = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path)?;
            toml::from_str(&content)
                .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?
        } else {
            CheckpointManifest {
                version: 1,
                checkpoints: Vec::new(),
                latest_full: None,
            }
        };

        Ok(Self {
            base_dir,
            manifest,
            full_snapshot_interval: 10,
        })
    }

    /// Set the interval for full snapshots
    pub fn with_full_snapshot_interval(mut self, interval: usize) -> Self {
        self.full_snapshot_interval = interval;
        self
    }

    /// Generate a new checkpoint ID
    fn generate_checkpoint_id() -> String {
        let now = Utc::now();
        format!("cp-{}", now.format("%Y%m%d_%H%M%S_%3f"))
    }

    /// Determine if we should create a full snapshot
    fn should_create_full(&self) -> bool {
        if self.manifest.latest_full.is_none() {
            return true;
        }

        let since_last_full = self
            .manifest
            .checkpoints
            .iter()
            .rev()
            .take_while(|c| !c.is_full)
            .count();

        since_last_full >= self.full_snapshot_interval
    }

    /// Create a checkpoint for the given source directory
    pub fn create_checkpoint(
        &mut self,
        operation_id: &str,
        source_dir: &Path,
    ) -> Result<Checkpoint> {
        let checkpoint_id = Self::generate_checkpoint_id();
        let is_full = self.should_create_full();

        let checkpoint_dir = self.base_dir.join(&checkpoint_id);
        fs::create_dir_all(&checkpoint_dir)?;

        // Get base file hashes for incremental
        let base_hashes = if is_full {
            HashMap::new()
        } else if let Some(ref latest_full_id) = self.manifest.latest_full {
            self.load_checkpoint(latest_full_id)?
                .map(|c| c.file_hashes)
                .unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Compute delta
        let delta = compute_delta(&base_hashes, source_dir)?;

        // Save delta files
        save_delta_files(&delta, source_dir, &checkpoint_dir)?;

        // Collect current file hashes
        let mut current_hashes = base_hashes.clone();
        let mut total_size = 0u64;

        for entry in &delta.entries {
            match entry.delta_type {
                super::delta::DeltaType::Added | super::delta::DeltaType::Modified => {
                    if let Some(ref hash) = entry.hash {
                        current_hashes.insert(entry.path.clone(), hash.clone());
                    }
                    if let Some(size) = entry.size {
                        total_size = total_size.saturating_add(size);
                    }
                }
                super::delta::DeltaType::Deleted => {
                    current_hashes.remove(&entry.path);
                }
            }
        }

        // Create checkpoint
        let checkpoint = Checkpoint::new(checkpoint_id.clone(), operation_id.to_string(), is_full)
            .with_hashes(current_hashes)
            .with_size(total_size);

        if !is_full {
            let checkpoint = checkpoint.with_base(
                self.manifest
                    .latest_full
                    .clone()
                    .unwrap_or_else(|| checkpoint_id.clone()),
            );
            self.save_checkpoint_meta(&checkpoint, &checkpoint_dir)?;

            // Update manifest
            self.manifest.checkpoints.push(CheckpointEntry {
                id: checkpoint_id,
                operation_id: operation_id.to_string(),
                created_at: checkpoint.created_at,
                is_full,
            });
            self.save_manifest()?;

            return Ok(checkpoint);
        }

        // Save checkpoint metadata
        self.save_checkpoint_meta(&checkpoint, &checkpoint_dir)?;

        // Update manifest
        self.manifest.checkpoints.push(CheckpointEntry {
            id: checkpoint_id.clone(),
            operation_id: operation_id.to_string(),
            created_at: checkpoint.created_at,
            is_full,
        });

        if is_full {
            self.manifest.latest_full = Some(checkpoint_id);
        }

        self.save_manifest()?;

        Ok(checkpoint)
    }

    /// Save checkpoint metadata to disk
    fn save_checkpoint_meta(&self, checkpoint: &Checkpoint, checkpoint_dir: &Path) -> Result<()> {
        let meta_path = checkpoint_dir.join("meta.toml");
        let content = toml::to_string_pretty(checkpoint)
            .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;
        fs::write(meta_path, content)?;
        Ok(())
    }

    /// Save manifest to disk
    fn save_manifest(&self) -> Result<()> {
        let manifest_path = self.base_dir.join("manifest.toml");
        let content = toml::to_string_pretty(&self.manifest)
            .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    /// Load a checkpoint by ID
    pub fn load_checkpoint(&self, checkpoint_id: &str) -> Result<Option<Checkpoint>> {
        let checkpoint_dir = self.base_dir.join(checkpoint_id);
        let meta_path = checkpoint_dir.join("meta.toml");

        if !meta_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(meta_path)?;
        let checkpoint: Checkpoint = toml::from_str(&content)
            .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;

        Ok(Some(checkpoint))
    }

    /// Restore a checkpoint to the target directory
    pub fn restore_checkpoint(&self, checkpoint_id: &str, target_dir: &Path) -> Result<()> {
        // Find the chain of checkpoints to apply
        let chain = self.build_checkpoint_chain(checkpoint_id)?;

        // Clear target directory (but keep .dot-agent-meta.toml)
        self.clear_target_preserving_meta(target_dir)?;

        // Apply checkpoints in order (oldest first)
        for cp_id in chain {
            let checkpoint_dir = self.base_dir.join(&cp_id);
            let delta = load_delta(&checkpoint_dir)?;
            self.apply_delta(&delta, &checkpoint_dir, target_dir)?;
        }

        Ok(())
    }

    /// Build the chain of checkpoints from base to target
    fn build_checkpoint_chain(&self, checkpoint_id: &str) -> Result<Vec<String>> {
        let mut chain = Vec::new();
        let mut current_id = checkpoint_id.to_string();

        loop {
            chain.push(current_id.clone());

            let checkpoint = self
                .load_checkpoint(&current_id)?
                .ok_or_else(|| crate::error::DotAgentError::NotFound(current_id.clone()))?;

            if checkpoint.is_full {
                break;
            }

            match checkpoint.base_checkpoint {
                Some(base_id) => current_id = base_id,
                None => break,
            }
        }

        chain.reverse();
        Ok(chain)
    }

    /// Clear target directory while preserving metadata files
    fn clear_target_preserving_meta(&self, target_dir: &Path) -> Result<()> {
        if !target_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(target_dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Preserve dot-agent metadata
            if name == ".dot-agent-meta.toml" || name == ".dot-agent-history" {
                continue;
            }

            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }

        Ok(())
    }

    /// Apply a delta to the target directory
    fn apply_delta(&self, delta: &Delta, checkpoint_dir: &Path, target_dir: &Path) -> Result<()> {
        let delta_dir = checkpoint_dir.join("delta");

        for entry in &delta.entries {
            let target_path = target_dir.join(&entry.path);

            match entry.delta_type {
                super::delta::DeltaType::Added | super::delta::DeltaType::Modified => {
                    let source_path = delta_dir.join(&entry.path);
                    if source_path.exists() {
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::copy(&source_path, &target_path)?;
                    }
                }
                super::delta::DeltaType::Deleted => {
                    if target_path.exists() {
                        fs::remove_file(&target_path)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// List all checkpoints
    pub fn list_checkpoints(&self) -> &[CheckpointEntry] {
        &self.manifest.checkpoints
    }

    /// Prune old checkpoints, keeping at least `keep_count` recent ones
    pub fn prune(&mut self, keep_count: usize) -> Result<usize> {
        if self.manifest.checkpoints.len() <= keep_count {
            return Ok(0);
        }

        let to_remove = self.manifest.checkpoints.len() - keep_count;
        let mut removed = 0;

        // Remove oldest checkpoints (but never remove latest_full)
        let mut new_checkpoints = Vec::new();
        for (i, entry) in self.manifest.checkpoints.iter().enumerate() {
            if i < to_remove && Some(&entry.id) != self.manifest.latest_full.as_ref() {
                let checkpoint_dir = self.base_dir.join(&entry.id);
                if checkpoint_dir.exists() {
                    fs::remove_dir_all(&checkpoint_dir)?;
                    removed += 1;
                }
            } else {
                new_checkpoints.push(entry.clone());
            }
        }

        self.manifest.checkpoints = new_checkpoints;
        self.save_manifest()?;

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn checkpoint_creation() {
        let cp = Checkpoint::new("cp-001".into(), "op-001".into(), true);
        assert!(cp.is_full);
        assert!(cp.base_checkpoint.is_none());
    }

    #[test]
    fn checkpoint_manager_init() -> Result<()> {
        let temp = TempDir::new()?;
        let manager = CheckpointManager::new(temp.path().to_path_buf())?;

        assert!(manager.manifest.checkpoints.is_empty());
        assert!(manager.manifest.latest_full.is_none());

        Ok(())
    }

    #[test]
    fn create_full_checkpoint() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_source = TempDir::new()?;

        // Create source files
        fs::write(temp_source.path().join("file1.txt"), "content1")?;
        fs::write(temp_source.path().join("file2.txt"), "content2")?;

        let mut manager = CheckpointManager::new(temp_base.path().to_path_buf())?;
        let checkpoint = manager.create_checkpoint("op-001", temp_source.path())?;

        assert!(checkpoint.is_full);
        assert_eq!(checkpoint.file_count, 2);

        Ok(())
    }

    #[test]
    fn restore_checkpoint() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_source = TempDir::new()?;
        let temp_target = TempDir::new()?;

        // Create source files
        fs::write(temp_source.path().join("file1.txt"), "content1")?;
        fs::write(temp_source.path().join("file2.txt"), "content2")?;

        let mut manager = CheckpointManager::new(temp_base.path().to_path_buf())?;
        let checkpoint = manager.create_checkpoint("op-001", temp_source.path())?;

        // Restore to target
        manager.restore_checkpoint(&checkpoint.id, temp_target.path())?;

        assert!(temp_target.path().join("file1.txt").exists());
        assert!(temp_target.path().join("file2.txt").exists());

        Ok(())
    }
}
