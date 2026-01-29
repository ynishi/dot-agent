//! LLM Integration Module
//!
//! Claude CLIを使用したLLM操作の共通機能を提供する。
//!
//! ## 使用方法
//!
//! ### Claude CLI可用性チェック
//!
//! ```rust
//! use dot_agent_core::check_claude_cli;
//!
//! let available = check_claude_cli();
//! println!("Claude CLI available: {}", available);
//! ```
//!
//! ### LlmConfig
//!
//! ```rust
//! use dot_agent_core::LlmConfig;
//!
//! let config = LlmConfig::default();
//! assert!(!config.enabled);
//! ```
//!
//! ### 完全な使用例（外部依存あり）
//!
//! ```rust,ignore
//! use dot_agent_core::{check_claude_cli, execute_claude};
//! use std::path::Path;
//!
//! if check_claude_cli() {
//!     let working_dir = Path::new(".");
//!     let result = execute_claude(working_dir, "Your prompt here")?;
//!     println!("{}", result);
//! }
//! ```

use std::io::Write as IoWrite;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};

// ============================================================================
// Configuration
// ============================================================================

/// LLM機能の設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmConfig {
    /// LLM機能を有効にするか（デフォルト: false）
    #[serde(default)]
    pub enabled: bool,
}

// ============================================================================
// CLI Operations
// ============================================================================

/// Claude CLIが利用可能かチェック
///
/// `claude --version` を実行して成功すればtrue
pub fn check_claude_cli() -> bool {
    Command::new("claude")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Claude CLIを実行してプロンプトを処理
///
/// # Arguments
/// * `working_dir` - 作業ディレクトリ
/// * `prompt` - 送信するプロンプト
///
/// # Returns
/// Claude CLIの出力（stdout）
///
/// # Errors
/// * `ClaudeNotFound` - Claude CLIが見つからない場合
/// * `ClaudeExecutionFailed` - 実行に失敗した場合
pub fn execute_claude(working_dir: &Path, prompt: &str) -> Result<String> {
    let mut cmd = Command::new("claude");
    cmd.arg("--print");
    cmd.arg("--dangerously-skip-permissions");
    cmd.current_dir(working_dir);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| DotAgentError::ClaudeExecutionFailed {
            message: format!("Failed to spawn claude: {}", e),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| DotAgentError::ClaudeExecutionFailed {
                message: format!("Failed to write prompt: {}", e),
            })?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| DotAgentError::ClaudeExecutionFailed {
            message: format!("Execution failed: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DotAgentError::ClaudeExecutionFailed {
            message: format!("Claude exited with error: {}", stderr),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Claude CLIの存在を確認し、なければエラーを返す
pub fn require_claude_cli() -> Result<()> {
    if !check_claude_cli() {
        return Err(DotAgentError::ClaudeNotFound);
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_llm_config_deserialize() {
        let toml_str = r#"
            enabled = true
        "#;
        let config: LlmConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
    }

    #[test]
    fn test_llm_config_deserialize_empty() {
        let toml_str = "";
        let config: LlmConfig = toml::from_str(toml_str).unwrap();
        assert!(!config.enabled);
    }
}
