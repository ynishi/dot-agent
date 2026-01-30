use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::operation::Operation;
use crate::error::Result;

/// Persistent operation graph (DAG)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationGraph {
    /// Graph version for future migrations
    pub version: u32,
    /// All operations indexed by ID
    #[serde(default)]
    pub operations: HashMap<String, Operation>,
    /// Root operations (no parent)
    #[serde(default)]
    pub roots: Vec<String>,
    /// Latest operation ID (head)
    pub head: Option<String>,
}

impl OperationGraph {
    pub fn new() -> Self {
        Self {
            version: 1,
            operations: HashMap::new(),
            roots: Vec::new(),
            head: None,
        }
    }

    /// Load graph from file
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)?;
        let graph: Self = toml::from_str(&content)
            .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;

        Ok(graph)
    }

    /// Save graph to file
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::DotAgentError::Internal(e.to_string()))?;

        fs::write(path, content)?;
        Ok(())
    }

    /// Add an operation to the graph
    pub fn add_operation(&mut self, operation: Operation) {
        let id = operation.id.as_str().to_string();

        // Update roots if this is a root operation
        if operation.parent.is_none() {
            self.roots.push(id.clone());
        }

        // Update head
        self.head = Some(id.clone());

        // Store operation
        self.operations.insert(id, operation);
    }

    /// Get an operation by ID
    pub fn get_operation(&self, id: &str) -> Option<&Operation> {
        self.operations.get(id)
    }

    /// Get the latest operation
    pub fn get_head(&self) -> Option<&Operation> {
        self.head.as_ref().and_then(|id| self.operations.get(id))
    }

    /// Get all operations in chronological order
    pub fn operations_chronological(&self) -> Vec<&Operation> {
        let mut ops: Vec<_> = self.operations.values().collect();
        ops.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        ops
    }

    /// Get operations in reverse chronological order
    pub fn operations_reverse_chronological(&self) -> Vec<&Operation> {
        let mut ops: Vec<_> = self.operations.values().collect();
        ops.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        ops
    }

    /// Get the ancestry chain from an operation back to root
    pub fn get_ancestry(&self, id: &str) -> Vec<&Operation> {
        let mut chain = Vec::new();
        let mut current_id = Some(id.to_string());

        while let Some(ref op_id) = current_id {
            if let Some(op) = self.operations.get(op_id) {
                chain.push(op);
                current_id = op.parent.as_ref().map(|p| p.as_str().to_string());
            } else {
                break;
            }
        }

        chain
    }

    /// Get children of an operation
    pub fn get_children(&self, id: &str) -> Vec<&Operation> {
        self.operations
            .values()
            .filter(|op| op.parent.as_ref().map(|p| p.as_str()) == Some(id))
            .collect()
    }

    /// Find operation by checkpoint ID
    pub fn find_by_checkpoint(&self, checkpoint_id: &str) -> Option<&Operation> {
        self.operations
            .values()
            .find(|op| op.checkpoint_id == checkpoint_id)
    }

    /// Get operations count
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Check if graph is empty
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Remove an operation and its descendants
    pub fn remove_operation_tree(&mut self, id: &str) -> Vec<Operation> {
        let mut removed = Vec::new();
        let mut to_remove = vec![id.to_string()];

        while let Some(current_id) = to_remove.pop() {
            // Find children before removing
            let children: Vec<_> = self
                .operations
                .values()
                .filter(|op| op.parent.as_ref().map(|p| p.as_str()) == Some(&current_id))
                .map(|op| op.id.as_str().to_string())
                .collect();

            to_remove.extend(children);

            // Remove the operation
            if let Some(op) = self.operations.remove(&current_id) {
                removed.push(op);
            }

            // Update roots
            self.roots.retain(|r| r != &current_id);
        }

        // Update head if removed
        if self
            .head
            .as_ref()
            .map(|h| removed.iter().any(|op| op.id.as_str() == h))
            == Some(true)
        {
            self.head = self
                .operations_reverse_chronological()
                .first()
                .map(|op| op.id.as_str().to_string());
        }

        removed
    }
}

/// Manages the operation graph file
pub struct GraphManager {
    graph: OperationGraph,
    path: PathBuf,
}

impl GraphManager {
    pub fn new(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join("graph.toml");
        let graph = OperationGraph::load(&path)?;

        Ok(Self { graph, path })
    }

    pub fn graph(&self) -> &OperationGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut OperationGraph {
        &mut self.graph
    }

    pub fn add_operation(&mut self, operation: Operation) -> Result<()> {
        self.graph.add_operation(operation);
        self.save()
    }

    pub fn save(&self) -> Result<()> {
        self.graph.save(&self.path)
    }

    /// Get operations since a specific operation (exclusive)
    pub fn operations_since(&self, since_id: &str) -> Vec<&Operation> {
        let since_op = match self.graph.get_operation(since_id) {
            Some(op) => op,
            None => return Vec::new(),
        };

        self.graph
            .operations
            .values()
            .filter(|op| op.timestamp > since_op.timestamp)
            .collect()
    }

    /// Find the common ancestor of two operations (reserved for merge/rebase features)
    #[allow(dead_code)]
    pub fn common_ancestor(&self, id1: &str, id2: &str) -> Option<&Operation> {
        let ancestry1: std::collections::HashSet<_> = self
            .graph
            .get_ancestry(id1)
            .iter()
            .map(|op| op.id.as_str())
            .collect();

        self.graph
            .get_ancestry(id2)
            .into_iter()
            .find(|op| ancestry1.contains(op.id.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::operation::{InstallOperationOptions, OperationId, OperationType};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_operation(id: &str, parent: Option<&str>, checkpoint: &str) -> Operation {
        Operation {
            id: OperationId::from_string(id),
            operation_type: OperationType::Install {
                profile: "test".into(),
                source: None,
                target: PathBuf::from("/test"),
                options: InstallOperationOptions::default(),
            },
            parent: parent.map(|p| OperationId::from_string(p)),
            checkpoint_id: checkpoint.into(),
            timestamp: chrono::Utc::now(),
            description: None,
        }
    }

    #[test]
    fn graph_add_operation() {
        let mut graph = OperationGraph::new();

        let op1 = create_test_operation("op-001", None, "cp-001");
        graph.add_operation(op1);

        assert_eq!(graph.len(), 1);
        assert_eq!(graph.roots.len(), 1);
        assert!(graph.get_operation("op-001").is_some());
    }

    #[test]
    fn graph_ancestry() {
        let mut graph = OperationGraph::new();

        let op1 = create_test_operation("op-001", None, "cp-001");
        let op2 = create_test_operation("op-002", Some("op-001"), "cp-002");
        let op3 = create_test_operation("op-003", Some("op-002"), "cp-003");

        graph.add_operation(op1);
        graph.add_operation(op2);
        graph.add_operation(op3);

        let ancestry = graph.get_ancestry("op-003");
        assert_eq!(ancestry.len(), 3);
        assert_eq!(ancestry[0].id.as_str(), "op-003");
        assert_eq!(ancestry[1].id.as_str(), "op-002");
        assert_eq!(ancestry[2].id.as_str(), "op-001");
    }

    #[test]
    fn graph_persistence() -> Result<()> {
        let temp = TempDir::new()?;
        let path = temp.path().join("graph.toml");

        let mut graph = OperationGraph::new();
        let op = create_test_operation("op-001", None, "cp-001");
        graph.add_operation(op);
        graph.save(&path)?;

        let loaded = OperationGraph::load(&path)?;
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get_operation("op-001").is_some());

        Ok(())
    }

    #[test]
    fn graph_children() {
        let mut graph = OperationGraph::new();

        let op1 = create_test_operation("op-001", None, "cp-001");
        let op2 = create_test_operation("op-002", Some("op-001"), "cp-002");
        let op3 = create_test_operation("op-003", Some("op-001"), "cp-003");

        graph.add_operation(op1);
        graph.add_operation(op2);
        graph.add_operation(op3);

        let children = graph.get_children("op-001");
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn graph_remove_tree() {
        let mut graph = OperationGraph::new();

        let op1 = create_test_operation("op-001", None, "cp-001");
        let op2 = create_test_operation("op-002", Some("op-001"), "cp-002");
        let op3 = create_test_operation("op-003", Some("op-002"), "cp-003");

        graph.add_operation(op1);
        graph.add_operation(op2);
        graph.add_operation(op3);

        let removed = graph.remove_operation_tree("op-002");
        assert_eq!(removed.len(), 2);
        assert_eq!(graph.len(), 1);
    }
}
