# dot-agent

Profile-based configuration manager for AI agents (Claude Code, etc.)

## Overview

`dot-agent` manages reusable configuration profiles for AI coding assistants. Create profiles with skills, commands, rules, and instructions, then install them to any project.

**Key Features:**
- Profile management with snapshots and versioning
- Channel system for discovering profiles from multiple sources
- Claude Code Plugin integration (hooks, mcpServers, lspServers)
- Profile → Plugin conversion for advanced features

## Installation

```bash
# CLI only
cargo install dot-agent-cli

# With GUI
cargo install dot-agent-cli --features gui
```

## Quick Start

```bash
# Create a new profile
dot-agent profile add my-profile

# List profiles
dot-agent profile list

# Install to a project
dot-agent install -p my-profile ~/my-project

# Check status
dot-agent status ~/my-project

# Upgrade to latest
dot-agent upgrade -p my-profile ~/my-project

# Show differences
dot-agent diff -p my-profile ~/my-project

# Remove from project
dot-agent remove -p my-profile ~/my-project
```

## Profile Structure

```
~/.dot-agent/profiles/my-profile/
├── CLAUDE.md           # Main instructions
├── skills/             # Skill definitions
├── commands/           # Custom commands
├── rules/              # Project rules
├── agents/             # Agent configurations
├── hooks/              # Claude Code hooks (via plugin)
│   └── hooks.json
├── .mcp.json           # MCP servers config (via plugin)
└── .lsp.json           # LSP servers config (via plugin)
```

> **Note:** `hooks/`, `.mcp.json`, `.lsp.json` require publishing as a Claude Code Plugin to be active.

## Commands

| Command | Description |
|---------|-------------|
| `profile add <name>` | Create a new profile |
| `profile list` | List all profiles |
| `profile remove <name>` | Delete a profile |
| `profile import <source>` | Import from directory or git URL |
| `profile snapshot <action>` | Manage profile snapshots |
| `install -p <profile> [target]` | Install profile to target |
| `upgrade -p <profile> [target]` | Update installed files |
| `diff -p <profile> [target]` | Show differences |
| `remove -p <profile> [target]` | Remove installed files |
| `status [target]` | Show installation status |
| `switch -p <profile> [target]` | Switch to a different profile |
| `snapshot <action>` | Manage target snapshots |
| `completions <shell>` | Generate shell completions |
| `channel <action>` | Manage channels (profile sources) |
| `hub <action>` | Manage hubs (channel aggregators) |
| `plugin <action>` | Manage Claude Code plugins |
| `profile publish <name>` | Publish profile as Claude Code plugin |
| `profile unpublish <name>` | Remove published plugin |
| `profile published` | List published profiles |

## Options

- `--global` / `-g`: Use `~/.claude` as target
- `--force` / `-f`: Overwrite conflicts
- `--dry-run` / `-d`: Preview without changes
- `--no-prefix`: Don't add profile prefix to filenames
- `--gui`: Launch GUI (requires `gui` feature)

## Import from Git

```bash
# Import entire repository
dot-agent profile import https://github.com/user/my-profile

# Import subdirectory
dot-agent profile import https://github.com/user/repo --path profiles/rust

# Import specific branch
dot-agent profile import https://github.com/user/repo --branch develop
```

## File Prefixing

By default, installed files are prefixed with the profile name to avoid conflicts:

- `rules/testing.md` → `rules/my-profile-testing.md`
- `skills/tdd/SKILL.md` → `skills/my-profile-tdd/SKILL.md`

Use `--no-prefix` to disable this behavior.

## Snapshots

Snapshots allow you to save and restore states of both profiles (source) and installed targets.

### Profile Snapshots

Save/restore profile source directories:

```bash
# Save current state
dot-agent profile snapshot save my-profile -m "before refactoring"

# List snapshots
dot-agent profile snapshot list my-profile

# Show changes since snapshot
dot-agent profile snapshot diff my-profile <id>

# Restore to previous state
dot-agent profile snapshot restore my-profile <id>

# Delete old snapshots, keep recent 5
dot-agent profile snapshot prune my-profile --keep 5
```

### Target Snapshots

Save/restore installed configurations:

```bash
# Save current installation state
dot-agent snapshot save --path ~/my-project -m "working config"

# List snapshots
dot-agent snapshot list --path ~/my-project

# Show changes since snapshot
dot-agent snapshot diff <id> --path ~/my-project

# Restore to previous state
dot-agent snapshot restore <id> --path ~/my-project

# Prune old snapshots
dot-agent snapshot prune --keep 10 --path ~/my-project
```

Snapshots are automatically created before `switch` operations.

## Channels & Hubs

Channels are sources for discovering profiles. Hubs aggregate multiple channels.

```bash
# List channels
dot-agent channel list

# Add a channel from GitHub
dot-agent channel add https://github.com/user/awesome-profiles

# Add Claude Code Plugin Marketplace as channel
dot-agent channel add-plugin anthropics/claude-plugins-official

# Discover channels from hubs
dot-agent channel discover

# Manage hubs
dot-agent hub list
dot-agent hub add https://github.com/dot-agent/official-hub
```

### Channel Types

| Type | Description |
|------|-------------|
| `github-global` | Search all of GitHub |
| `awesome` | Curated Awesome Lists |
| `hub` | Channels from a Hub repository |
| `direct` | Direct URL to a repository |
| `claude-plugin` | Claude Code Plugin Marketplace |

## Claude Code Plugin Integration

Integrate with Claude Code's native plugin system for hooks, MCP servers, and LSP servers.

### Plugin Management

```bash
# Search plugins across marketplaces
dot-agent plugin search rust-analyzer

# List plugins from a marketplace
dot-agent plugin list claude-plugins-official

# Install a plugin
dot-agent plugin install rust-lsp -m claude-plugins-official --scope user

# List installed plugins
dot-agent plugin installed

# Uninstall a plugin
dot-agent plugin uninstall rust-lsp -m claude-plugins-official
```

### Marketplace Management

```bash
# Add a marketplace
dot-agent plugin add-marketplace anthropics/claude-plugins-official

# List marketplaces
dot-agent plugin list-marketplaces

# Update marketplaces
dot-agent plugin update-marketplace
```

### Install Scopes

| Scope | Location | Use Case |
|-------|----------|----------|
| `user` | `~/.claude/plugins/` | Personal global settings |
| `project` | `.claude/plugins/` | Team shared (git tracked) |
| `local` | `.claude/plugins/` | Personal project (gitignored) |

## Profile Publishing

Publish profiles as Claude Code plugins to enable advanced features like hooks, MCP servers, and LSP servers.

```bash
# Publish a profile as a plugin
dot-agent profile publish my-rust-profile --scope user

# Publish with version bump
dot-agent profile publish my-rust-profile --bump patch  # 1.0.0 -> 1.0.1
dot-agent profile publish my-rust-profile --bump minor  # 1.0.0 -> 1.1.0
dot-agent profile publish my-rust-profile --bump major  # 1.0.0 -> 2.0.0

# List published profiles
dot-agent profile published

# Unpublish a profile
dot-agent profile unpublish my-rust-profile
```

### What Gets Published

| Content | Destination |
|---------|-------------|
| `skills/`, `commands/`, `agents/` | Plugin cache (auto-loaded) |
| `hooks/hooks.json` | Plugin cache (auto-loaded) |
| `.mcp.json`, `.lsp.json` | Plugin cache (auto-loaded) |
| `rules/` | `.claude/rules/` (scope-aware) |
| `CLAUDE.md` | `.claude/CLAUDE.md` (merged) |

### hooks.json Example

```json
{
  "PostToolUse": [
    {
      "matcher": "Edit",
      "hooks": [
        {
          "type": "command",
          "command": "cargo fmt"
        }
      ]
    }
  ]
}
```

### .mcp.json Example

```json
{
  "mcpServers": {
    "rust-docs": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-rust-docs"]
    }
  }
}
```

## Crates

- `dot-agent-cli`: CLI binary and optional GUI
- `dot-agent-core`: Core library for profile management

## License

MIT
