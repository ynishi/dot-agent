//! Builtin Category Definitions
//!
//! コード内で定義されるビルトインカテゴリ。
//! すべてのProfileで利用可能。

use serde::{Deserialize, Serialize};

/// デフォルトの優先度
pub const DEFAULT_CATEGORY_PRIORITY: i32 = 100;

/// ビルトインカテゴリ定義
pub const BUILTIN_CATEGORIES: &[BuiltinCategory] = &[
    BuiltinCategory {
        name: "plan",
        description: "Planning, design, architecture, brainstorming. \
                      Requirements analysis, system design, implementation strategy. \
                      Used before writing any code.",
        patterns: &[
            "*plan*",
            "*architect*",
            "*design*",
            "*brainstorm*",
            "*spec*",
            "*requirement*",
        ],
        priority: DEFAULT_CATEGORY_PRIORITY,
    },
    BuiltinCategory {
        name: "execute",
        description: "Implementation, coding, building. \
                      TDD workflow, code generation, build processes. \
                      The actual code writing phase.",
        patterns: &[
            "*tdd*",
            "*coding*",
            "*build*",
            "*implement*",
            "*code-*",
            "*develop*",
        ],
        priority: DEFAULT_CATEGORY_PRIORITY,
    },
    BuiltinCategory {
        name: "review",
        description: "Code review, testing, verification, auditing. \
                      Quality assurance, security review, compliance checks.",
        patterns: &[
            "*review*",
            "*test*",
            "*verify*",
            "*audit*",
            "*check*",
            "*quality*",
        ],
        priority: DEFAULT_CATEGORY_PRIORITY,
    },
    BuiltinCategory {
        name: "debug",
        description: "Debugging, troubleshooting, error resolution. \
                      Build errors, runtime issues, performance problems.",
        patterns: &["*debug*", "*fix*", "*error*", "*troubleshoot*", "*resolve*"],
        priority: DEFAULT_CATEGORY_PRIORITY,
    },
];

/// ビルトインカテゴリの静的定義
#[derive(Debug, Clone)]
pub struct BuiltinCategory {
    /// カテゴリ名（一意識別子）
    pub name: &'static str,
    /// カテゴリの説明（LLM分類で使用）
    pub description: &'static str,
    /// Globパターン（ファイルパスマッチング用）
    pub patterns: &'static [&'static str],
    /// 優先度（複数マッチ時のソート用、高いほど優先）
    pub priority: i32,
}

/// ランタイムカテゴリ定義
///
/// ビルトインまたは`.dot-agent.toml`から構築される。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDef {
    /// カテゴリ名
    pub name: String,
    /// カテゴリの説明（LLM分類のプロンプトに使用）
    pub description: String,
    /// Globパターン（ファイルパスマッチング用）
    pub patterns: Vec<String>,
    /// 優先度
    #[serde(default = "default_priority")]
    pub priority: i32,
}

fn default_priority() -> i32 {
    DEFAULT_CATEGORY_PRIORITY
}

impl From<&BuiltinCategory> for CategoryDef {
    fn from(builtin: &BuiltinCategory) -> Self {
        Self {
            name: builtin.name.to_string(),
            description: builtin.description.to_string(),
            patterns: builtin.patterns.iter().map(|s| s.to_string()).collect(),
            priority: builtin.priority,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_categories_exist() {
        assert!(!BUILTIN_CATEGORIES.is_empty());
        assert!(BUILTIN_CATEGORIES.iter().any(|c| c.name == "plan"));
        assert!(BUILTIN_CATEGORIES.iter().any(|c| c.name == "execute"));
        assert!(BUILTIN_CATEGORIES.iter().any(|c| c.name == "review"));
        assert!(BUILTIN_CATEGORIES.iter().any(|c| c.name == "debug"));
    }

    #[test]
    fn test_category_def_from_builtin() {
        let builtin = &BUILTIN_CATEGORIES[0];
        let def = CategoryDef::from(builtin);
        assert_eq!(def.name, builtin.name);
        assert_eq!(def.priority, DEFAULT_CATEGORY_PRIORITY);
    }
}
