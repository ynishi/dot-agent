//! # History Pack Format
//!
//! **⚠️ EXPERIMENTAL: This API and data format are unstable.**
//!
//! Breaking changes may occur without notice until stabilization.
//! Do not rely on this format for long-term storage yet.
//!
//! ## Overview
//!
//! Pack is a portable archive format for dot-agent operation history.
//! It bundles the operation graph and checkpoint data into a single file
//! for easy backup, transfer, and restore.
//!
//! ## Format Structure
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │ Header (magic + version)            │
//! ├─────────────────────────────────────┤
//! │ Graph (TOML, length-prefixed)       │
//! ├─────────────────────────────────────┤
//! │ Checkpoint Count (u32)              │
//! ├─────────────────────────────────────┤
//! │ Checkpoint 0                        │
//! │   ├─ ID (length-prefixed string)    │
//! │   ├─ Meta (TOML, length-prefixed)   │
//! │   ├─ File Count (u32)               │
//! │   └─ Files[]                        │
//! │       ├─ Path (length-prefixed)     │
//! │       └─ Content (length-prefixed)  │
//! ├─────────────────────────────────────┤
//! │ Checkpoint 1...N                    │
//! └─────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use dot_agent_core::history::{PackWriter, PackReader};
//!
//! // Export
//! let writer = PackWriter::new(&history_dir)?;
//! writer.write_to_file("backup.dotpack")?;
//!
//! // Import
//! let reader = PackReader::open("backup.dotpack")?;
//! reader.unpack_to(&history_dir)?;
//! ```

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::checkpoint::{Checkpoint, CheckpointManager};
use super::graph::OperationGraph;
use crate::error::{DotAgentError, Result};

/// Pack format magic bytes
const PACK_MAGIC: &[u8; 8] = b"DOTPACK\0";

/// Current pack format version
///
/// **EXPERIMENTAL**: This version number will change with breaking format changes.
const PACK_VERSION: u32 = 1;

/// File extension for pack files
pub const PACK_EXTENSION: &str = "dotpack";

/// A packed checkpoint with embedded file contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackedCheckpoint {
    /// Checkpoint ID
    pub id: String,
    /// Checkpoint metadata
    pub meta: Checkpoint,
    /// File contents: path -> raw bytes
    #[serde(skip)]
    pub files: HashMap<String, Vec<u8>>,
}

/// Complete pack structure
///
/// **⚠️ EXPERIMENTAL**: Format may change without notice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    /// Format version
    pub version: u32,
    /// Operation graph
    pub graph: OperationGraph,
    /// Packed checkpoints
    pub checkpoints: Vec<PackedCheckpoint>,
}

impl Pack {
    /// Create a new empty pack
    pub fn new() -> Self {
        Self {
            version: PACK_VERSION,
            graph: OperationGraph::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Get total file count across all checkpoints
    pub fn total_files(&self) -> usize {
        self.checkpoints.iter().map(|c| c.files.len()).sum()
    }

    /// Get total size in bytes
    pub fn total_size(&self) -> usize {
        self.checkpoints
            .iter()
            .flat_map(|c| c.files.values())
            .map(|v| v.len())
            .sum()
    }
}

impl Default for Pack {
    fn default() -> Self {
        Self::new()
    }
}

/// Writer for creating pack files
///
/// **⚠️ EXPERIMENTAL**: API may change without notice.
pub struct PackWriter {
    history_dir: PathBuf,
}

impl PackWriter {
    /// Create a new pack writer
    pub fn new(history_dir: impl Into<PathBuf>) -> Self {
        Self {
            history_dir: history_dir.into(),
        }
    }

    /// Build pack from history directory
    pub fn build_pack(&self) -> Result<Pack> {
        let mut pack = Pack::new();

        // Load graph
        let graph_path = self.history_dir.join("graph.toml");
        pack.graph = OperationGraph::load(&graph_path)?;

        // Load checkpoints
        let checkpoints_dir = self.history_dir.join("checkpoints");
        if checkpoints_dir.exists() {
            let checkpoint_manager = CheckpointManager::new(checkpoints_dir.clone())?;

            for entry in checkpoint_manager.list_checkpoints() {
                let checkpoint_dir = checkpoints_dir.join(&entry.id);

                if let Some(meta) = checkpoint_manager.load_checkpoint(&entry.id)? {
                    let mut packed = PackedCheckpoint {
                        id: entry.id.clone(),
                        meta,
                        files: HashMap::new(),
                    };

                    // Load delta files
                    let delta_dir = checkpoint_dir.join("delta");
                    if delta_dir.exists() {
                        Self::collect_files(&delta_dir, &delta_dir, &mut packed.files)?;
                    }

                    pack.checkpoints.push(packed);
                }
            }
        }

        Ok(pack)
    }

    /// Collect files recursively
    fn collect_files(
        root: &Path,
        current: &Path,
        files: &mut HashMap<String, Vec<u8>>,
    ) -> Result<()> {
        if !current.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                Self::collect_files(root, &path, files)?;
            } else if path.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|e| DotAgentError::Internal(e.to_string()))?;
                let content = fs::read(&path)?;
                files.insert(relative.to_string_lossy().to_string(), content);
            }
        }

        Ok(())
    }

    /// Write pack to file
    ///
    /// **⚠️ EXPERIMENTAL**: File format may change.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<PackStats> {
        let pack = self.build_pack()?;
        let file = File::create(path.as_ref())?;
        let mut writer = BufWriter::new(file);

        // Write header
        writer.write_all(PACK_MAGIC)?;
        writer.write_all(&PACK_VERSION.to_le_bytes())?;

        // Write graph as length-prefixed TOML
        let graph_toml = toml::to_string_pretty(&pack.graph)
            .map_err(|e| DotAgentError::Internal(e.to_string()))?;
        write_length_prefixed(&mut writer, graph_toml.as_bytes())?;

        // Write checkpoint count
        writer.write_all(&(pack.checkpoints.len() as u32).to_le_bytes())?;

        // Write each checkpoint
        for checkpoint in &pack.checkpoints {
            // ID
            write_length_prefixed(&mut writer, checkpoint.id.as_bytes())?;

            // Meta as TOML
            let meta_toml = toml::to_string_pretty(&checkpoint.meta)
                .map_err(|e| DotAgentError::Internal(e.to_string()))?;
            write_length_prefixed(&mut writer, meta_toml.as_bytes())?;

            // File count
            writer.write_all(&(checkpoint.files.len() as u32).to_le_bytes())?;

            // Files
            for (path, content) in &checkpoint.files {
                write_length_prefixed(&mut writer, path.as_bytes())?;
                write_length_prefixed(&mut writer, content)?;
            }
        }

        writer.flush()?;

        Ok(PackStats {
            operations: pack.graph.len(),
            checkpoints: pack.checkpoints.len(),
            files: pack.total_files(),
            size_bytes: pack.total_size(),
        })
    }
}

/// Reader for unpacking pack files
///
/// **⚠️ EXPERIMENTAL**: API may change without notice.
pub struct PackReader {
    pack: Pack,
}

impl PackReader {
    /// Open a pack file
    ///
    /// **⚠️ EXPERIMENTAL**: File format may change.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);

        // Read and verify header
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        if &magic != PACK_MAGIC {
            return Err(DotAgentError::Internal("Invalid pack file magic".into()));
        }

        let mut version_bytes = [0u8; 4];
        reader.read_exact(&mut version_bytes)?;
        let version = u32::from_le_bytes(version_bytes);

        if version > PACK_VERSION {
            return Err(DotAgentError::Internal(format!(
                "Pack version {} is newer than supported version {}",
                version, PACK_VERSION
            )));
        }

        // Read graph
        let graph_bytes = read_length_prefixed(&mut reader)?;
        let graph_toml =
            String::from_utf8(graph_bytes).map_err(|e| DotAgentError::Internal(e.to_string()))?;
        let graph: OperationGraph =
            toml::from_str(&graph_toml).map_err(|e| DotAgentError::Internal(e.to_string()))?;

        // Read checkpoint count
        let mut count_bytes = [0u8; 4];
        reader.read_exact(&mut count_bytes)?;
        let checkpoint_count = u32::from_le_bytes(count_bytes) as usize;

        // Read checkpoints
        let mut checkpoints = Vec::with_capacity(checkpoint_count);
        for _ in 0..checkpoint_count {
            // ID
            let id_bytes = read_length_prefixed(&mut reader)?;
            let id =
                String::from_utf8(id_bytes).map_err(|e| DotAgentError::Internal(e.to_string()))?;

            // Meta
            let meta_bytes = read_length_prefixed(&mut reader)?;
            let meta_toml = String::from_utf8(meta_bytes)
                .map_err(|e| DotAgentError::Internal(e.to_string()))?;
            let meta: Checkpoint =
                toml::from_str(&meta_toml).map_err(|e| DotAgentError::Internal(e.to_string()))?;

            // File count
            let mut file_count_bytes = [0u8; 4];
            reader.read_exact(&mut file_count_bytes)?;
            let file_count = u32::from_le_bytes(file_count_bytes) as usize;

            // Files
            let mut files = HashMap::with_capacity(file_count);
            for _ in 0..file_count {
                let path_bytes = read_length_prefixed(&mut reader)?;
                let path = String::from_utf8(path_bytes)
                    .map_err(|e| DotAgentError::Internal(e.to_string()))?;
                let content = read_length_prefixed(&mut reader)?;
                files.insert(path, content);
            }

            checkpoints.push(PackedCheckpoint { id, meta, files });
        }

        Ok(Self {
            pack: Pack {
                version,
                graph,
                checkpoints,
            },
        })
    }

    /// Get pack info
    pub fn info(&self) -> &Pack {
        &self.pack
    }

    /// Get statistics
    pub fn stats(&self) -> PackStats {
        PackStats {
            operations: self.pack.graph.len(),
            checkpoints: self.pack.checkpoints.len(),
            files: self.pack.total_files(),
            size_bytes: self.pack.total_size(),
        }
    }

    /// Unpack to history directory
    ///
    /// **⚠️ EXPERIMENTAL**: This will overwrite existing history.
    pub fn unpack_to(&self, history_dir: impl AsRef<Path>) -> Result<()> {
        let history_dir = history_dir.as_ref();
        fs::create_dir_all(history_dir)?;

        // Write graph
        let graph_path = history_dir.join("graph.toml");
        self.pack.graph.save(&graph_path)?;

        // Write checkpoints
        let checkpoints_dir = history_dir.join("checkpoints");
        fs::create_dir_all(&checkpoints_dir)?;

        // Write manifest
        let manifest = CheckpointManifest {
            version: 1,
            checkpoints: self
                .pack
                .checkpoints
                .iter()
                .map(|c| CheckpointEntry {
                    id: c.id.clone(),
                    operation_id: c.meta.operation_id.clone(),
                    created_at: c.meta.created_at,
                    is_full: c.meta.is_full,
                })
                .collect(),
            latest_full: self
                .pack
                .checkpoints
                .iter()
                .filter(|c| c.meta.is_full)
                .next_back()
                .map(|c| c.id.clone()),
        };

        let manifest_toml = toml::to_string_pretty(&manifest)
            .map_err(|e| DotAgentError::Internal(e.to_string()))?;
        fs::write(checkpoints_dir.join("manifest.toml"), manifest_toml)?;

        // Write checkpoint data
        for checkpoint in &self.pack.checkpoints {
            let checkpoint_dir = checkpoints_dir.join(&checkpoint.id);
            fs::create_dir_all(&checkpoint_dir)?;

            // Meta
            let meta_toml = toml::to_string_pretty(&checkpoint.meta)
                .map_err(|e| DotAgentError::Internal(e.to_string()))?;
            fs::write(checkpoint_dir.join("meta.toml"), meta_toml)?;

            // Delta manifest (reconstruct from meta)
            let delta = super::delta::Delta {
                entries: Vec::new(), // Will be reconstructed from files
                base_checkpoint: checkpoint.meta.base_checkpoint.clone(),
                is_full: checkpoint.meta.is_full,
            };
            let delta_toml = toml::to_string_pretty(&delta)
                .map_err(|e| DotAgentError::Internal(e.to_string()))?;
            fs::write(checkpoint_dir.join("delta.toml"), delta_toml)?;

            // Files
            let delta_dir = checkpoint_dir.join("delta");
            for (path, content) in &checkpoint.files {
                let file_path = delta_dir.join(path);
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(file_path, content)?;
            }
        }

        Ok(())
    }

    /// Merge into existing history (append non-duplicate operations)
    ///
    /// **⚠️ EXPERIMENTAL**: Merge logic may change.
    pub fn merge_into(&self, history_dir: impl AsRef<Path>) -> Result<MergeStats> {
        let history_dir = history_dir.as_ref();
        let mut stats = MergeStats::default();

        // Load existing graph
        let graph_path = history_dir.join("graph.toml");
        let mut existing_graph = if graph_path.exists() {
            OperationGraph::load(&graph_path)?
        } else {
            OperationGraph::new()
        };

        // Merge operations
        for (id, op) in &self.pack.graph.operations {
            if !existing_graph.operations.contains_key(id) {
                existing_graph.operations.insert(id.clone(), op.clone());
                stats.operations_added += 1;
            } else {
                stats.operations_skipped += 1;
            }
        }

        // Update roots and head
        for root in &self.pack.graph.roots {
            if !existing_graph.roots.contains(root) {
                existing_graph.roots.push(root.clone());
            }
        }
        if let Some(ref head) = self.pack.graph.head {
            existing_graph.head = Some(head.clone());
        }

        existing_graph.save(&graph_path)?;

        // Merge checkpoints
        let checkpoints_dir = history_dir.join("checkpoints");
        fs::create_dir_all(&checkpoints_dir)?;

        for checkpoint in &self.pack.checkpoints {
            let checkpoint_dir = checkpoints_dir.join(&checkpoint.id);
            if checkpoint_dir.exists() {
                stats.checkpoints_skipped += 1;
                continue;
            }

            fs::create_dir_all(&checkpoint_dir)?;

            // Write checkpoint data (same as unpack)
            let meta_toml = toml::to_string_pretty(&checkpoint.meta)
                .map_err(|e| DotAgentError::Internal(e.to_string()))?;
            fs::write(checkpoint_dir.join("meta.toml"), meta_toml)?;

            let delta_dir = checkpoint_dir.join("delta");
            for (path, content) in &checkpoint.files {
                let file_path = delta_dir.join(path);
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(file_path, content)?;
            }

            stats.checkpoints_added += 1;
        }

        Ok(stats)
    }
}

/// Statistics about a pack file
#[derive(Debug, Clone, Default)]
pub struct PackStats {
    pub operations: usize,
    pub checkpoints: usize,
    pub files: usize,
    pub size_bytes: usize,
}

/// Statistics from a merge operation
#[derive(Debug, Clone, Default)]
pub struct MergeStats {
    pub operations_added: usize,
    pub operations_skipped: usize,
    pub checkpoints_added: usize,
    pub checkpoints_skipped: usize,
}

// Re-use checkpoint types for manifest
use super::checkpoint::{CheckpointEntry, CheckpointManifest};

/// Write length-prefixed data
fn write_length_prefixed<W: Write>(writer: &mut W, data: &[u8]) -> io::Result<()> {
    writer.write_all(&(data.len() as u32).to_le_bytes())?;
    writer.write_all(data)?;
    Ok(())
}

/// Read length-prefixed data
fn read_length_prefixed<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    let mut data = vec![0u8; len];
    reader.read_exact(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn pack_roundtrip() -> Result<()> {
        let temp_history = TempDir::new()?;
        let temp_pack = TempDir::new()?;

        // Create minimal history
        let graph = OperationGraph::new();
        let graph_path = temp_history.path().join("graph.toml");
        graph.save(&graph_path)?;

        fs::create_dir_all(temp_history.path().join("checkpoints"))?;
        let manifest = CheckpointManifest {
            version: 1,
            checkpoints: Vec::new(),
            latest_full: None,
        };
        let manifest_toml = toml::to_string_pretty(&manifest).unwrap();
        fs::write(
            temp_history.path().join("checkpoints/manifest.toml"),
            manifest_toml,
        )?;

        // Pack
        let pack_path = temp_pack.path().join("test.dotpack");
        let writer = PackWriter::new(temp_history.path());
        let stats = writer.write_to_file(&pack_path)?;

        assert_eq!(stats.operations, 0);
        assert_eq!(stats.checkpoints, 0);

        // Unpack to new location
        let temp_restore = TempDir::new()?;
        let reader = PackReader::open(&pack_path)?;
        reader.unpack_to(temp_restore.path())?;

        // Verify graph exists
        assert!(temp_restore.path().join("graph.toml").exists());

        Ok(())
    }

    #[test]
    fn pack_stats() {
        let pack = Pack::new();
        assert_eq!(pack.total_files(), 0);
        assert_eq!(pack.total_size(), 0);
    }
}
