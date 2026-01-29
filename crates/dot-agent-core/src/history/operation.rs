use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unique identifier for an operation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId(String);

impl OperationId {
    pub fn new() -> Self {
        let id = format!("op-{}", uuid::Uuid::new_v4().simple());
        Self(id)
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Source information for tracking where a profile came from
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceInfo {
    Local {
        path: PathBuf,
    },
    Git {
        url: String,
        branch: Option<String>,
        commit: Option<String>,
    },
    Marketplace {
        id: String,
        version: String,
    },
}

/// Type of operation performed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationType {
    /// Install a profile to a target
    Install {
        profile: String,
        source: Option<SourceInfo>,
        target: PathBuf,
        options: InstallOperationOptions,
    },
    /// Remove a profile from a target
    Remove { profile: String, target: PathBuf },
    /// Upgrade a profile at a target
    Upgrade {
        profile: String,
        source: Option<SourceInfo>,
        target: PathBuf,
        from_checkpoint: Option<String>,
    },
    /// Fusion of multiple profiles into a new one
    Fusion {
        inputs: Vec<FusionInput>,
        output_profile: String,
    },
    /// Apply a rule to create a new profile
    RuleApply {
        rule_name: String,
        source_profile: String,
        output_profile: String,
    },
    /// User manual edit (auto-detected)
    UserEdit {
        target: PathBuf,
        files_changed: Vec<String>,
        auto_detected: bool,
    },
    /// Manual snapshot (user-triggered)
    ManualSnapshot {
        target: PathBuf,
        description: Option<String>,
    },
}

/// Options used during install operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallOperationOptions {
    pub force: bool,
    pub dry_run: bool,
    pub no_prefix: bool,
    pub no_merge: bool,
}

/// Input specification for fusion operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionInput {
    pub profile: String,
    pub category: Option<String>,
}

/// A single operation in the history graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Unique identifier
    pub id: OperationId,
    /// Type of operation with its specific data
    pub operation_type: OperationType,
    /// Parent operation ID (None for root)
    pub parent: Option<OperationId>,
    /// Checkpoint ID created after this operation
    pub checkpoint_id: String,
    /// When the operation was performed
    pub timestamp: DateTime<Utc>,
    /// Optional description
    pub description: Option<String>,
}

impl Operation {
    pub fn new(
        operation_type: OperationType,
        parent: Option<OperationId>,
        checkpoint_id: String,
    ) -> Self {
        Self {
            id: OperationId::new(),
            operation_type,
            parent,
            checkpoint_id,
            timestamp: Utc::now(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Get a human-readable summary of this operation
    pub fn summary(&self) -> String {
        match &self.operation_type {
            OperationType::Install {
                profile, target, ..
            } => {
                format!("install {} -> {}", profile, target.display())
            }
            OperationType::Remove { profile, target } => {
                format!("remove {} from {}", profile, target.display())
            }
            OperationType::Upgrade {
                profile, target, ..
            } => {
                format!("upgrade {} at {}", profile, target.display())
            }
            OperationType::Fusion {
                inputs,
                output_profile,
            } => {
                let input_strs: Vec<_> = inputs
                    .iter()
                    .map(|i| {
                        if let Some(cat) = &i.category {
                            format!("{}:{}", i.profile, cat)
                        } else {
                            i.profile.clone()
                        }
                    })
                    .collect();
                format!("fusion [{}] -> {}", input_strs.join(", "), output_profile)
            }
            OperationType::RuleApply {
                rule_name,
                source_profile,
                output_profile,
            } => {
                format!(
                    "rule-apply {} to {} -> {}",
                    rule_name, source_profile, output_profile
                )
            }
            OperationType::UserEdit {
                target,
                files_changed,
                ..
            } => {
                format!(
                    "user-edit at {} ({} files)",
                    target.display(),
                    files_changed.len()
                )
            }
            OperationType::ManualSnapshot { target, .. } => {
                format!("snapshot {}", target.display())
            }
        }
    }

    /// Check if this operation is auto-detected (user edit)
    pub fn is_auto_detected(&self) -> bool {
        matches!(
            &self.operation_type,
            OperationType::UserEdit {
                auto_detected: true,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_id_generation() {
        let id1 = OperationId::new();
        let id2 = OperationId::new();
        assert_ne!(id1, id2);
        assert!(id1.as_str().starts_with("op-"));
    }

    #[test]
    fn operation_summary() {
        let op = Operation::new(
            OperationType::Install {
                profile: "test-profile".into(),
                source: None,
                target: PathBuf::from("/home/user/.claude"),
                options: Default::default(),
            },
            None,
            "cp-001".into(),
        );
        assert!(op.summary().contains("install"));
        assert!(op.summary().contains("test-profile"));
    }

    #[test]
    fn fusion_input_summary() {
        let op = Operation::new(
            OperationType::Fusion {
                inputs: vec![
                    FusionInput {
                        profile: "base".into(),
                        category: Some("agents".into()),
                    },
                    FusionInput {
                        profile: "rust".into(),
                        category: Some("rules".into()),
                    },
                ],
                output_profile: "mixed".into(),
            },
            None,
            "cp-002".into(),
        );
        let summary = op.summary();
        assert!(summary.contains("base:agents"));
        assert!(summary.contains("rust:rules"));
        assert!(summary.contains("mixed"));
    }
}
