use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::checkpoint::{Checkpoint, CheckpointManager};
use super::delta::{compute_delta, compute_file_hash};
use super::graph::{GraphManager, OperationGraph};
use super::operation::{Operation, OperationId, OperationType};
use crate::error::Result;

/// Main API for history management
pub struct HistoryManager {
    /// Base directory for history storage
    base_dir: PathBuf,
    /// Graph manager
    graph_manager: GraphManager,
    /// Checkpoint manager
    checkpoint_manager: CheckpointManager,
    /// Cached file hashes for change detection
    cached_hashes: HashMap<PathBuf, FileHashCache>,
}

/// Cache for file hashes at a specific point
#[derive(Debug, Clone, Default)]
struct FileHashCache {
    hashes: HashMap<String, String>,
    captured_at: Option<DateTime<Utc>>,
}

impl HistoryManager {
    /// Create a new history manager
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        let history_dir = base_dir.join("history");
        fs::create_dir_all(&history_dir)?;

        let graph_manager = GraphManager::new(&history_dir)?;
        let checkpoint_manager = CheckpointManager::new(history_dir.join("checkpoints"))?;

        Ok(Self {
            base_dir,
            graph_manager,
            checkpoint_manager,
            cached_hashes: HashMap::new(),
        })
    }

    /// Get the operation graph
    pub fn graph(&self) -> &OperationGraph {
        self.graph_manager.graph()
    }

    /// Record an operation and create a checkpoint
    pub fn record_operation(
        &mut self,
        operation_type: OperationType,
        target_dir: &Path,
    ) -> Result<Operation> {
        // Get parent (current head)
        let parent = self
            .graph_manager
            .graph()
            .head
            .clone()
            .map(OperationId::from_string);

        // Create checkpoint
        let temp_op_id = OperationId::new();
        let checkpoint = self
            .checkpoint_manager
            .create_checkpoint(temp_op_id.as_str(), target_dir)?;

        // Create operation
        let operation = Operation::new(operation_type, parent, checkpoint.id.clone());

        // Add to graph
        self.graph_manager.add_operation(operation.clone())?;

        // Update cached hashes
        self.cached_hashes.insert(
            target_dir.to_path_buf(),
            FileHashCache {
                hashes: checkpoint.file_hashes,
                captured_at: Some(Utc::now()),
            },
        );

        Ok(operation)
    }

    /// Rollback to a specific operation (undo operations after it)
    pub fn rollback(&mut self, operation_id: &str, target_dir: &Path) -> Result<RollbackResult> {
        let operation = self
            .graph_manager
            .graph()
            .get_operation(operation_id)
            .ok_or_else(|| crate::error::DotAgentError::NotFound(operation_id.to_string()))?;

        let checkpoint_id = operation.checkpoint_id.clone();

        // Get operations that will be undone (collect IDs to avoid borrow conflict)
        let op_ids_to_undo: Vec<String> = self
            .graph_manager
            .operations_since(operation_id)
            .iter()
            .map(|op| op.id.as_str().to_string())
            .collect();
        let undo_count = op_ids_to_undo.len();

        // Restore checkpoint
        self.checkpoint_manager
            .restore_checkpoint(&checkpoint_id, target_dir)?;

        // Remove undone operations from graph
        for op_id in &op_ids_to_undo {
            self.graph_manager.graph_mut().remove_operation_tree(op_id);
        }
        self.graph_manager.save()?;

        // Update cached hashes
        if let Some(checkpoint) = self.checkpoint_manager.load_checkpoint(&checkpoint_id)? {
            self.cached_hashes.insert(
                target_dir.to_path_buf(),
                FileHashCache {
                    hashes: checkpoint.file_hashes,
                    captured_at: Some(Utc::now()),
                },
            );
        }

        Ok(RollbackResult {
            restored_to: operation_id.to_string(),
            operations_undone: undo_count,
        })
    }

    /// Restore to a specific checkpoint (without modifying graph)
    pub fn restore(&self, checkpoint_id: &str, target_dir: &Path) -> Result<()> {
        self.checkpoint_manager
            .restore_checkpoint(checkpoint_id, target_dir)
    }

    /// Detect user edits since last recorded operation
    pub fn detect_changes(&self, target_dir: &Path) -> Result<ChangeDetectionResult> {
        let cached = self
            .cached_hashes
            .get(target_dir)
            .cloned()
            .unwrap_or_default();

        let delta = compute_delta(&cached.hashes, target_dir)?;

        Ok(ChangeDetectionResult {
            has_changes: !delta.is_empty(),
            added: delta.added_count(),
            modified: delta.modified_count(),
            deleted: delta.deleted_count(),
            files_changed: delta.entries.iter().map(|e| e.path.clone()).collect(),
        })
    }

    /// Sync: detect and record user edits as an operation
    pub fn sync(&mut self, target_dir: &Path) -> Result<Option<Operation>> {
        let changes = self.detect_changes(target_dir)?;

        if !changes.has_changes {
            return Ok(None);
        }

        let operation = self.record_operation(
            OperationType::UserEdit {
                target: target_dir.to_path_buf(),
                files_changed: changes.files_changed,
                auto_detected: true,
            },
            target_dir,
        )?;

        Ok(Some(operation))
    }

    /// List recent operations
    pub fn list_history(&self, limit: Option<usize>) -> Vec<HistoryEntry> {
        let ops = self
            .graph_manager
            .graph()
            .operations_reverse_chronological();
        let iter = ops.iter();

        let entries: Vec<_> = if let Some(limit) = limit {
            iter.take(limit)
                .map(|op| HistoryEntry::from_operation(op))
                .collect()
        } else {
            iter.map(|op| HistoryEntry::from_operation(op)).collect()
        };

        entries
    }

    /// Get operation details
    pub fn get_operation(&self, operation_id: &str) -> Option<&Operation> {
        self.graph_manager.graph().get_operation(operation_id)
    }

    /// Get checkpoint for an operation
    pub fn get_checkpoint(&self, operation_id: &str) -> Result<Option<Checkpoint>> {
        let operation = match self.graph_manager.graph().get_operation(operation_id) {
            Some(op) => op,
            None => return Ok(None),
        };

        self.checkpoint_manager
            .load_checkpoint(&operation.checkpoint_id)
    }

    /// Prune old checkpoints
    pub fn prune_checkpoints(&mut self, keep_count: usize) -> Result<usize> {
        self.checkpoint_manager.prune(keep_count)
    }

    /// Get ancestry of an operation
    pub fn get_ancestry(&self, operation_id: &str) -> Vec<&Operation> {
        self.graph_manager.graph().get_ancestry(operation_id)
    }

    /// Initialize cache for a target directory
    pub fn init_cache(&mut self, target_dir: &Path) -> Result<()> {
        let mut hashes = HashMap::new();
        self.walk_and_hash(target_dir, target_dir, &mut hashes)?;

        self.cached_hashes.insert(
            target_dir.to_path_buf(),
            FileHashCache {
                hashes,
                captured_at: Some(Utc::now()),
            },
        );

        Ok(())
    }

    fn walk_and_hash(
        &self,
        root: &Path,
        current: &Path,
        hashes: &mut HashMap<String, String>,
    ) -> Result<()> {
        if !current.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();

            // Skip history directory
            if path.file_name().and_then(|n| n.to_str()) == Some(".dot-agent-history") {
                continue;
            }

            if path.is_dir() {
                self.walk_and_hash(root, &path, hashes)?;
            } else if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;

                if let Ok(hash) = compute_file_hash(&path) {
                    hashes.insert(relative.to_string_lossy().to_string(), hash);
                }
            }
        }

        Ok(())
    }
}

/// Result of a rollback operation
#[derive(Debug, Clone)]
pub struct RollbackResult {
    pub restored_to: String,
    pub operations_undone: usize,
}

/// Result of change detection
#[derive(Debug, Clone)]
pub struct ChangeDetectionResult {
    pub has_changes: bool,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub files_changed: Vec<String>,
}

/// History entry for display
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: String,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
    pub checkpoint_id: String,
    pub is_auto_detected: bool,
}

impl HistoryEntry {
    fn from_operation(op: &Operation) -> Self {
        Self {
            id: op.id.as_str().to_string(),
            summary: op.summary(),
            timestamp: op.timestamp,
            checkpoint_id: op.checkpoint_id.clone(),
            is_auto_detected: op.is_auto_detected(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::operation::InstallOperationOptions;
    use tempfile::TempDir;

    #[test]
    fn history_manager_init() -> Result<()> {
        let temp = TempDir::new()?;
        let manager = HistoryManager::new(temp.path().to_path_buf())?;

        assert!(manager.graph().is_empty());
        Ok(())
    }

    #[test]
    fn record_and_list_operations() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_target = TempDir::new()?;

        // Create target files
        fs::write(temp_target.path().join("file.txt"), "content")?;

        let mut manager = HistoryManager::new(temp_base.path().to_path_buf())?;

        // Record operation
        let op = manager.record_operation(
            OperationType::Install {
                profile: "test".into(),
                source: None,
                target: temp_target.path().to_path_buf(),
                options: InstallOperationOptions::default(),
            },
            temp_target.path(),
        )?;

        // List history
        let history = manager.list_history(None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, op.id.as_str());

        Ok(())
    }

    #[test]
    fn detect_changes() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_target = TempDir::new()?;

        fs::write(temp_target.path().join("file.txt"), "content")?;

        let mut manager = HistoryManager::new(temp_base.path().to_path_buf())?;
        manager.init_cache(temp_target.path())?;

        // No changes initially
        let result = manager.detect_changes(temp_target.path())?;
        assert!(!result.has_changes);

        // Make a change
        fs::write(temp_target.path().join("new.txt"), "new content")?;

        // Now there should be changes
        let result = manager.detect_changes(temp_target.path())?;
        assert!(result.has_changes);
        assert_eq!(result.added, 1);

        Ok(())
    }

    #[test]
    fn sync_user_edits() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_target = TempDir::new()?;

        fs::write(temp_target.path().join("file.txt"), "content")?;

        let mut manager = HistoryManager::new(temp_base.path().to_path_buf())?;

        // Initial record
        manager.record_operation(
            OperationType::Install {
                profile: "test".into(),
                source: None,
                target: temp_target.path().to_path_buf(),
                options: InstallOperationOptions::default(),
            },
            temp_target.path(),
        )?;

        // Make user edit
        fs::write(temp_target.path().join("user.txt"), "user content")?;

        // Sync
        let op = manager.sync(temp_target.path())?;
        assert!(op.is_some());
        assert!(op.unwrap().is_auto_detected());

        Ok(())
    }

    #[test]
    fn rollback_operation() -> Result<()> {
        let temp_base = TempDir::new()?;
        let temp_target = TempDir::new()?;

        // Initial state
        fs::write(temp_target.path().join("file.txt"), "original")?;

        let mut manager = HistoryManager::new(temp_base.path().to_path_buf())?;

        // First operation
        let op1 = manager.record_operation(
            OperationType::Install {
                profile: "test".into(),
                source: None,
                target: temp_target.path().to_path_buf(),
                options: InstallOperationOptions::default(),
            },
            temp_target.path(),
        )?;

        // Modify file
        fs::write(temp_target.path().join("file.txt"), "modified")?;

        // Second operation
        manager.record_operation(
            OperationType::UserEdit {
                target: temp_target.path().to_path_buf(),
                files_changed: vec!["file.txt".into()],
                auto_detected: false,
            },
            temp_target.path(),
        )?;

        // Rollback to first operation
        let result = manager.rollback(op1.id.as_str(), temp_target.path())?;
        assert_eq!(result.operations_undone, 1);

        // Verify content restored
        let content = fs::read_to_string(temp_target.path().join("file.txt"))?;
        assert_eq!(content, "original");

        Ok(())
    }
}
