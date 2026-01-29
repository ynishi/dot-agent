//! # Category Module
//!
//! Profile内のファイルを意味的なカテゴリに分類する機能を提供する。
//!
//! ## 設計目的
//!
//! 従来のdot-agentは物理的なディレクトリ構造（skills/, agents/, rules/等）で
//! ファイルを管理していたが、ユーザーが求めるのは意味的な分類である：
//!
//! - **Plan**: 設計、アーキテクチャ、ブレインストーミング
//! - **Execute**: 実装、コーディング、TDD、ビルド
//! - **Review**: コードレビュー、テスト、検証、監査
//! - **Debug**: デバッグ、トラブルシューティング、エラー解決
//!
//! ## モジュール構成
//!
//! - `builtin`: ビルトインカテゴリ定義
//! - `store`: カテゴリ定義のランタイムストア
//! - `classifier`: 分類器
//!
//! ## 使用例
//!
//! ### CategoryStoreの基本操作
//!
//! ```rust
//! use dot_agent_core::category::{CategoryStore, BUILTIN_CATEGORIES};
//!
//! // ビルトインカテゴリを取得
//! let store = CategoryStore::builtin();
//! assert!(store.get("plan").is_some());
//! assert!(store.get("execute").is_some());
//! assert!(store.get("review").is_some());
//! assert!(store.get("debug").is_some());
//!
//! // カテゴリ一覧を取得
//! let names = store.names();
//! assert!(names.len() >= 4);
//! ```
//!
//! ### 完全な使用例（外部依存あり）
//!
//! ```rust,ignore
//! use dot_agent_core::category::{CategoryClassifier, ClassificationMode};
//! use dot_agent_core::Profile;
//!
//! // Profileをロード
//! let profile = Profile::load("my-profile")?;
//!
//! // カテゴリ分類（Globモード）
//! let classifier = CategoryClassifier::from_profile(&profile, ClassificationMode::Glob)?;
//! let classified = classifier.classify(&profile)?;
//!
//! // 特定カテゴリのファイルを取得
//! let plan_files = classified.files_in_category("plan");
//! ```

mod builtin;
mod classifier;
mod store;

// Re-exports
pub use builtin::{BuiltinCategory, CategoryDef, BUILTIN_CATEGORIES, DEFAULT_CATEGORY_PRIORITY};
pub use classifier::{
    CategoryClassifier, ClassificationMode, ClassifiedProfile, FileClassification,
};
pub use store::{CategoriesConfig, CategoryConfigEntry, CategoryStore};
