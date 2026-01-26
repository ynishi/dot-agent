# Claude Code Plugin Integration Design

## 概要

dot-agentにClaude Code Plugin Marketplace統合機能を追加する設計書。

**方針**: Claude Codeネイティブのプラグインシステムをそのまま利用し、settings.jsonを触らない。

## 1. アーキテクチャ

### 1.1 Claude Code プラグインディレクトリ構造

```
~/.claude/plugins/
├── known_marketplaces.json    # Marketplace一覧
├── installed_plugins.json     # インストール済みプラグイン
├── marketplaces/              # Marketplaceデータキャッシュ
│   └── <marketplace-name>/
│       └── .claude-plugin/marketplace.json
└── cache/                     # プラグイン本体
    └── <marketplace-name>/
        └── <plugin-name>/
            └── <version>/
                ├── .claude-plugin/plugin.json
                ├── skills/
                ├── commands/
                ├── agents/
                ├── .mcp.json
                ├── .lsp.json
                └── hooks/hooks.json
```

### 1.2 dot-agentの役割

**dot-agentがやること:**
1. `known_marketplaces.json` にMarketplace追加
2. `marketplaces/` にMarketplaceデータをクローン
3. プラグインを `cache/` にダウンロード
4. `installed_plugins.json` を更新

**Claude Codeが自動でやること:**
- mcpServers/lspServers/hooksを読み込み
- skills/commands/agentsを有効化
- 全て自動

**settings.json**: 一切触らない

### 1.3 新規ChannelType: `ClaudePlugin`

```rust
pub enum ChannelType {
    GitHubGlobal,
    AwesomeList,
    Hub,
    Direct,
    ClaudePlugin,  // NEW: Claude Code Plugin Marketplace
}
```

### 1.4 データフロー

```
┌─────────────────────────────────────────────────────────────────┐
│                     dot-agent                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐  │
│  │ Channel      │    │ Plugin       │    │ Claude Plugin    │  │
│  │ Registry     │───▶│ Fetcher      │───▶│ Installer        │  │
│  └──────────────┘    └──────────────┘    └──────────────────┘  │
│         │                   │                     │             │
│         │                   │                     ▼             │
│         │                   │            ┌──────────────────┐  │
│         │                   │            │ ~/.claude/       │  │
│         │                   │            │  plugins/        │  │
│         │                   │            │  ├── cache/      │  │
│         │                   │            │  ├── marketplaces│  │
│         │                   │            │  └── *.json      │  │
│         │                   │            └──────────────────┘  │
│         ▼                   ▼                     │             │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │              marketplace.json Parser                      │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Claude Code (自動)                           │
│  - プラグイン検出                                                │
│  - mcpServers/lspServers/hooks 読み込み                          │
│  - skills/commands/agents 有効化                                 │
└─────────────────────────────────────────────────────────────────┘
```

## 2. JSON ファイル構造

### 2.1 known_marketplaces.json

```json
{
  "claude-plugins-official": {
    "source": {
      "source": "github",
      "repo": "anthropics/claude-plugins-official"
    },
    "installLocation": "/Users/xxx/.claude/plugins/marketplaces/claude-plugins-official",
    "lastUpdated": "2026-01-26T06:16:27.909Z"
  },
  "my-custom-marketplace": {
    "source": {
      "source": "github",
      "repo": "user/my-marketplace"
    },
    "installLocation": "/Users/xxx/.claude/plugins/marketplaces/my-custom-marketplace",
    "lastUpdated": "2026-01-26T10:00:00.000Z"
  }
}
```

### 2.2 installed_plugins.json

```json
{
  "version": 2,
  "plugins": {
    "rust-analyzer-lsp@claude-plugins-official": [
      {
        "scope": "user",
        "installPath": "/Users/xxx/.claude/plugins/cache/claude-plugins-official/rust-analyzer-lsp/1.0.0",
        "version": "1.0.0",
        "installedAt": "2025-12-20T02:07:18.771Z",
        "lastUpdated": "2025-12-20T02:07:18.771Z"
      }
    ],
    "my-plugin@my-custom-marketplace": [
      {
        "scope": "user",
        "installPath": "/Users/xxx/.claude/plugins/cache/my-custom-marketplace/my-plugin/1.0.0",
        "version": "1.0.0",
        "installedAt": "2026-01-26T10:00:00.000Z",
        "lastUpdated": "2026-01-26T10:00:00.000Z"
      }
    ]
  }
}
```

### 2.3 インストールスコープ

| Scope | 意味 | 用途 |
|-------|------|------|
| `user` | 全プロジェクト共通 | 個人のグローバル設定 |
| `project` | プロジェクト固有（git共有） | チーム共有 |
| `local` | プロジェクト固有（gitignore） | 個人のプロジェクト設定 |

## 4. 新規型定義

### 4.1 Plugin型

```rust
/// Claude Code Plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Plugin name (from plugin.json)
    pub name: String,
    /// Version
    pub version: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Author
    pub author: Option<PluginAuthor>,
    /// Source marketplace
    pub marketplace: String,
    /// Local cache path
    pub cache_path: PathBuf,
    /// Installation scope
    pub scope: InstallScope,
    /// Enabled state
    pub enabled: bool,
    /// Components
    pub components: PluginComponents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginComponents {
    pub has_skills: bool,
    pub has_commands: bool,
    pub has_agents: bool,
    pub has_hooks: bool,
    pub has_mcp_servers: bool,
    pub has_lsp_servers: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum InstallScope {
    Global,
    Project,
    Local,
}
```

### 4.2 Marketplace型

```rust
/// Claude Code Plugin Marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marketplace {
    /// Marketplace name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Owner
    pub owner: MarketplaceOwner,
    /// Available plugins
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceOwner {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    pub source: PluginSource,
    pub description: Option<String>,
    pub version: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginSource {
    /// Relative path (e.g., "./plugins/my-plugin")
    Relative(String),
    /// Structured source
    Structured {
        source: String,  // "github", "url"
        repo: Option<String>,
        url: Option<String>,
        r#ref: Option<String>,
        sha: Option<String>,
    },
}
```

## 5. 新規コマンド

### 5.1 Marketplace管理

```bash
# Marketplace追加 (ChannelとしてRegistry)
dot-agent channel add --type=claude-plugin --url=github.com/anthropics/claude-plugins-official

# Marketplace一覧
dot-agent channel list --type=claude-plugin

# Marketplace更新
dot-agent channel update <name>
```

### 5.2 Plugin管理

```bash
# Plugin検索
dot-agent plugin search <query>

# Plugin詳細表示
dot-agent plugin show <plugin-name>

# Pluginインストール
dot-agent plugin install <plugin-name>@<marketplace> [--scope=global|project|local]

# Pluginアンインストール
dot-agent plugin uninstall <plugin-name>

# Plugin一覧
dot-agent plugin list

# Plugin有効/無効
dot-agent plugin enable <plugin-name>
dot-agent plugin disable <plugin-name>
```

## 6. 実装計画

### Phase 1: Channel追加 + Marketplace管理

1. **ChannelType::ClaudePlugin 追加**
   - `channel/types.rs` に新規バリアント追加
   - `ChannelSource::ClaudePlugin` 追加

2. **Claude Plugin型定義**
   - `plugin/types.rs` 新規作成
   - Marketplace/PluginEntry/InstalledPlugin 型定義

3. **marketplace.json パーサー**
   - `plugin/marketplace.rs` 新規作成
   - JSONパース実装

4. **known_marketplaces.json 管理**
   - `plugin/registry.rs` 新規作成
   - Marketplace追加/削除/一覧

### Phase 2: Plugin インストール

5. **installed_plugins.json 管理**
   - `plugin/registry.rs` 拡張
   - インストール状態の読み書き

6. **PluginFetcher**
   - `plugin/fetcher.rs` 新規作成
   - GitHub/URLからプラグインをダウンロード
   - `~/.claude/plugins/cache/` に配置

7. **PluginInstaller**
   - `plugin/installer.rs` 新規作成
   - installed_plugins.json 更新
   - スコープ対応（user/project/local）

### Phase 3: CLI

8. **plugin サブコマンド**
   - `cli/args.rs` 拡張
   - plugin search/install/uninstall/list/enable/disable

9. **channel拡張**
   - `--type=claude-plugin` 対応

## 7. ファイル構成（実装後）

```
crates/dot-agent-core/src/
├── channel/
│   ├── mod.rs
│   ├── types.rs           # ChannelType::ClaudePlugin追加
│   ├── channel_registry.rs
│   ├── hub_registry.rs
│   └── search.rs          # Plugin検索対応
├── plugin/                # NEW: プラグインモジュール
│   ├── mod.rs
│   ├── types.rs           # Marketplace/Plugin型
│   ├── marketplace.rs     # marketplace.jsonパーサー
│   ├── registry.rs        # known_marketplaces.json, installed_plugins.json管理
│   ├── fetcher.rs         # GitHubからのダウンロード
│   └── installer.rs       # インストール処理
├── config.rs
├── installer.rs
├── profile.rs
└── ...

crates/dot-agent-cli/src/
├── main.rs
└── args.rs                # plugin サブコマンド追加
```

## 8. テスト計画

### 8.1 Unit Tests

- marketplace.json パース
- known_marketplaces.json 読み書き
- installed_plugins.json 読み書き
- Plugin型シリアライズ/デシリアライズ

### 8.2 Integration Tests

- Marketplace追加→一覧→削除 フロー
- Plugin検索→インストール→アンインストール フロー
- 複数スコープでのインストール

### 8.3 E2E Tests

- 実際のmarketplace（anthropics/claude-plugins-official）からインストール
- Claude Code での動作確認（mcpServers/lspServers/hooks自動認識）

## 9. Profile → Plugin 変換機能

### 9.1 概要

dot-agentの既存ProfileをClaude Code Plugin形式に変換し、hooks/mcpServers/lspServersの恩恵を受けられるようにする。

### 9.2 変換フロー

```
# 既存のProfile
~/.dot-agent/profiles/my-rust-profile/
├── CLAUDE.md
├── skills/
├── commands/
├── agents/
├── rules/
├── hooks/                 # NEW: 拡張
│   └── hooks.json
├── .mcp.json              # NEW: 拡張
└── .lsp.json              # NEW: 拡張

        │
        │ dot-agent profile publish my-rust-profile
        ▼

# Plugin化して配置
~/.claude/plugins/cache/dot-agent-profiles/my-rust-profile/1.0.0/
├── .claude-plugin/
│   └── plugin.json        # 自動生成
├── skills/
├── commands/
├── agents/
├── hooks/
│   └── hooks.json
├── .mcp.json
└── .lsp.json

# rules/ は別途 .claude/rules/ にコピー（Plugin非対応のため）
```

### 9.3 機能対応表

| 機能 | 現状 | Plugin化後 |
|------|------|-----------|
| skills/ | 対応済 | そのまま動く |
| commands/ | 対応済 | そのまま動く |
| agents/ | 対応済 | そのまま動く |
| rules/ | dot-agent独自 | .claude/rules/に別途コピー |
| CLAUDE.md | dot-agent独自 | .claude/CLAUDE.mdに別途コピー |
| hooks/ | **未対応** | **対応可能に** |
| .mcp.json | **未対応** | **対応可能に** |
| .lsp.json | **未対応** | **対応可能に** |

### 9.4 Profile構造拡張

```toml
# ~/.dot-agent/profiles/my-rust-profile/profile.toml
[profile]
name = "my-rust-profile"
version = "1.0.0"
description = "Rust development profile with TDD workflow"
author = "Your Name"

# Plugin化設定（オプション）
[plugin]
# 有効にするとPlugin形式で管理
enabled = true
# Marketplace名（dot-agent-profiles固定）
marketplace = "dot-agent-profiles"
```

### 9.5 hooks/hooks.json 例

```json
{
  "PostToolUse": [
    {
      "matcher": "Edit",
      "hooks": [
        {
          "type": "command",
          "command": "if [[ \"$CLAUDE_FILE_PATHS\" == *.rs ]]; then cargo fmt; fi"
        }
      ]
    }
  ],
  "Stop": [
    {
      "matcher": "",
      "hooks": [
        {
          "type": "command",
          "command": "cargo clippy --quiet -- -D warnings 2>&1 | head -20"
        }
      ]
    }
  ]
}
```

### 9.6 .mcp.json 例

```json
{
  "mcpServers": {
    "rust-docs": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-rust-docs"],
      "env": {}
    },
    "cargo-workspace": {
      "command": "${CLAUDE_PLUGIN_ROOT}/servers/cargo-mcp",
      "args": ["--workspace", "."]
    }
  }
}
```

### 9.7 .lsp.json 例

```json
{
  "rust-analyzer": {
    "command": "rust-analyzer",
    "extensionToLanguage": {
      ".rs": "rust"
    },
    "initializationOptions": {
      "checkOnSave": {
        "command": "clippy"
      }
    }
  }
}
```

### 9.8 新規コマンド

```bash
# Profile → Plugin 変換 & インストール
dot-agent profile publish <profile-name> [--scope=user|project|local]

# やること:
# 1. plugin.json 自動生成
# 2. ~/.claude/plugins/cache/dot-agent-profiles/<name>/<version>/ にコピー
# 3. installed_plugins.json 更新
# 4. rules/ があれば .claude/rules/ にもコピー
# 5. CLAUDE.md があれば .claude/CLAUDE.md に追記

# Plugin化したProfileの一覧
dot-agent profile list --published

# Plugin化を解除（cacheから削除）
dot-agent profile unpublish <profile-name>

# Profile更新（バージョンアップ）
dot-agent profile publish <profile-name> --bump=patch|minor|major
```

### 9.9 plugin.json 自動生成

```json
{
  "name": "my-rust-profile",
  "version": "1.0.0",
  "description": "Rust development profile with TDD workflow",
  "author": {
    "name": "Your Name"
  },
  "repository": "https://github.com/user/dotfiles",
  "keywords": ["rust", "tdd", "development"]
}
```

### 9.10 dot-agent-profiles Marketplace

Profile → Plugin変換したものは `dot-agent-profiles` という仮想Marketplaceに所属。

```json
// ~/.claude/plugins/known_marketplaces.json に追加
{
  "dot-agent-profiles": {
    "source": {
      "source": "local",
      "path": "~/.dot-agent/profiles"
    },
    "installLocation": "/Users/xxx/.claude/plugins/marketplaces/dot-agent-profiles",
    "lastUpdated": "2026-01-26T10:00:00.000Z",
    "virtual": true  // 仮想Marketplace（実体はdot-agentが管理）
  }
}
```

## 10. 実装計画（更新）

### Phase 1: Channel追加 + Marketplace管理
（変更なし）

### Phase 2: Plugin インストール
（変更なし）

### Phase 3: CLI
（変更なし）

### Phase 4: Profile → Plugin 変換 (NEW)

10. **Profile構造拡張**
    - `profile.rs` 拡張
    - hooks/, .mcp.json, .lsp.json 対応

11. **ProfilePublisher**
    - `plugin/publisher.rs` 新規作成
    - plugin.json 自動生成
    - cache/ へのコピー
    - installed_plugins.json 更新

12. **profile publish コマンド**
    - `cli/args.rs` 拡張
    - publish/unpublish/list --published

## 11. ファイル構成（最終）

```
crates/dot-agent-core/src/
├── channel/
│   ├── mod.rs
│   ├── types.rs           # ChannelType::ClaudePlugin追加
│   ├── channel_registry.rs
│   ├── hub_registry.rs
│   └── search.rs
├── plugin/                # NEW
│   ├── mod.rs
│   ├── types.rs           # Marketplace/Plugin型
│   ├── marketplace.rs     # marketplace.jsonパーサー
│   ├── registry.rs        # known_marketplaces.json, installed_plugins.json管理
│   ├── fetcher.rs         # GitHubからのダウンロード
│   ├── installer.rs       # インストール処理
│   └── publisher.rs       # NEW: Profile → Plugin変換
├── profile.rs             # 拡張: hooks/.mcp.json/.lsp.json対応
├── config.rs
├── installer.rs
└── ...
```

## 12. 利点

| 項目 | 説明 |
|------|------|
| settings.json不要 | ユーザー設定を壊すリスクゼロ |
| Claude Code互換 | ネイティブのプラグインシステムをそのまま利用 |
| アンインストール容易 | cache削除 + JSON更新のみ |
| 競合なし | Claude Code CLIと共存可能 |
| **既存Profile活用** | **既存資産をそのままPlugin化** |
| **hooks対応** | **Profile単位でhooks定義可能** |
| **MCP/LSP対応** | **Profile単位でMCP/LSP設定可能** |
