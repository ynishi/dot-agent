# dot-agent v2 設計

## 背景・問題点

### 現状の問題

1. **`plugin` サブコマンドが独立している**
   - Channel/Hub/Profileと別体系
   - 「Claude Codeで直接Installすればいいじゃん」問題

2. **Marketplace管理が二重**
   - `channel add-plugin` と `plugin add-marketplace` が共存
   - 同じ概念が2箇所に分散

3. **ChannelType自動検出の曖昧さ**
   - URLからAwesome/Direct/Marketplaceを推測
   - 明示的でなく、誤検出の可能性

4. **Profileメタデータが散在**
   - ディレクトリ存在チェックのみ
   - ソース情報、バージョン、依存関係などが未管理

---

## 設計方針

### 核心原則

1. **Pluginは実装詳細**: ユーザーにはProfile/Channelのみ見せる
2. **明示的な型指定**: 自動検出より明示的なフラグ
3. **メタデータ一元管理**: profiles.toml + profile-metadata.toml

---

## 新アーキテクチャ

### ディレクトリ構造

```
~/.dot-agent/
├── config.toml              # グローバル設定
├── hubs.toml                # 登録Hub一覧
├── channels.toml            # 有効Channel一覧
├── profiles.toml            # Profile一覧・メタデータ索引
├── cache/
│   ├── hubs/                # Hubコンテンツキャッシュ
│   └── channels/            # Channelコンテンツキャッシュ
└── profiles/
    └── <profile-name>/
        ├── .dot-agent.toml  # Profileメタデータ
        ├── CLAUDE.md
        ├── skills/
        ├── commands/
        ├── rules/
        ├── agents/
        ├── hooks/           # → 裏でPlugin登録
        ├── .mcp.json        # → 裏でPlugin登録
        └── .lsp.json        # → 裏でPlugin登録
```

### profiles.toml

```toml
# Profile一覧・メタデータ索引
version = 1

[profiles.my-rust-profile]
path = "profiles/my-rust-profile"
source = { type = "local" }
created_at = "2025-01-26T00:00:00Z"
updated_at = "2025-01-26T00:00:00Z"

[profiles.awesome-dotfiles]
path = "profiles/awesome-dotfiles"
source = { type = "git", url = "https://github.com/user/dotfiles", branch = "main", commit = "abc123" }
created_at = "2025-01-26T00:00:00Z"
updated_at = "2025-01-26T00:00:00Z"

[profiles.rust-lsp-plugin]
path = "profiles/rust-lsp-plugin"
source = { type = "marketplace", channel = "claude-official", plugin = "rust-lsp", version = "1.2.0" }
created_at = "2025-01-26T00:00:00Z"
updated_at = "2025-01-26T00:00:00Z"
```

### .dot-agent.toml (Profile内)

```toml
# Profileメタデータ
[profile]
name = "my-rust-profile"
version = "1.0.0"
description = "Rust development profile with TDD workflow"
author = "username"

[source]
type = "git"  # local | git | marketplace
url = "https://github.com/user/profile"
branch = "main"
commit = "abc123def456"

[plugin]
# hooks/MCP/LSP がある場合、自動的にPlugin形式で登録
enabled = true
scope = "user"  # user | project | local

[dependencies]
# 他のProfileへの依存（将来拡張）
# requires = ["base-profile"]
```

---

## CLIコマンド変更

### 削除するコマンド

```bash
# 全て削除
dot-agent plugin search
dot-agent plugin list
dot-agent plugin installed
dot-agent plugin info
dot-agent plugin install
dot-agent plugin uninstall
dot-agent plugin add-marketplace
dot-agent plugin remove-marketplace
dot-agent plugin list-marketplaces
dot-agent plugin update-marketplace
```

### 変更するコマンド

```bash
# Channel追加 - 明示的なタイプ指定
dot-agent channel add <url>                      # 自動検出（非推奨）
dot-agent channel add -m/--marketplace <url>     # Marketplace
dot-agent channel add -a/--awesome <url>         # Awesome List
dot-agent channel add -d/--direct <url>          # Direct repo
dot-agent channel add -H/--hub <hub> <name>      # From Hub

# 例
dot-agent channel add -m anthropics/claude-plugins-official
dot-agent channel add -a https://github.com/rockerBOO/awesome-neovim
dot-agent channel add -d https://github.com/user/dotfiles
```

### 新規・拡張コマンド

```bash
# 統合検索 - 全Channel横断
dot-agent search <query>
dot-agent search <query> --channel <name>    # 特定Channel
dot-agent search <query> --type marketplace  # タイプフィルタ

# Profile import - Marketplace対応
dot-agent profile import <source>                    # Git/ローカル
dot-agent profile import <plugin>@<marketplace>      # Marketplaceから
dot-agent profile import rust-lsp@claude-official

# Install - 裏でPlugin登録
dot-agent install <profile> [--path <target>]
# → hooks/.mcp.json/.lsp.jsonがあれば自動的にPlugin登録
```

---

## データフロー

### Marketplace Plugin → Profile

```
1. channel add -m anthropics/claude-plugins-official
   ↓
2. Channel (type: ClaudePlugin) が channels.toml に登録
   ↓
3. search "rust-lsp"
   ↓
4. 結果: rust-lsp@claude-official (type: marketplace)
   ↓
5. profile import rust-lsp@claude-official
   ↓
6. profiles.toml に登録
   profiles/<name>/.dot-agent.toml に source.type = "marketplace"
   ↓
7. install <profile>
   ↓
8. 裏側で自動的にClaude Code Plugin形式で ~/.claude/plugins/ に登録
```

### 通常Profile → インストール

```
1. profile import https://github.com/user/dotfiles
   ↓
2. profiles.toml に登録
   profiles/<name>/.dot-agent.toml に source.type = "git"
   ↓
3. install <profile> --path ~/project
   ↓
4. ファイルコピー
   ↓
5. hooks/.mcp.json/.lsp.json があれば
   → 裏でPlugin形式に変換してClaude Codeに登録
```

---

## 実装フェーズ

### Phase 1: メタデータ管理

1. `profiles.toml` 導入
2. `.dot-agent.toml` 導入
3. 既存プロファイルのマイグレーション

### Phase 2: Channel明示化

1. `channel add` に `-m/-a/-d/-H` フラグ追加
2. 自動検出を非推奨化（警告表示）
3. `channel add-plugin` 削除

### Phase 3: Plugin統合

1. `plugin` サブコマンド削除
2. `profile import <plugin>@<marketplace>` 対応
3. `install` 時の自動Plugin登録

### Phase 4: 統合検索

1. `search` コマンドでChannel横断検索
2. Marketplace/Awesome/GitHub統合

---

## PRO/CON 分析

### この設計のPRO

1. **概念の統一**: ユーザーはProfile/Channelだけ意識
2. **明示的**: 自動検出の曖昧さ排除
3. **メタデータ管理**: ソース情報・バージョン追跡可能
4. **拡張性**: 依存関係管理などの将来機能の基盤

### この設計のCON

1. **マイグレーション必要**: 既存ユーザーへの影響
2. **コマンド変更**: 学習コスト
3. **裏側の複雑さ**: Plugin自動登録の実装

### 期待値

- ユーザー体験の一貫性向上
- dot-agentの価値明確化（単なるPlugin管理ツールとの差別化）
- 長期的なメンテナビリティ向上
