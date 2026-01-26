# dot-agent

Profile-based configuration manager for AI agents (Claude Code, etc.)

## Overview

`dot-agent` manages reusable configuration profiles for AI coding assistants. Create profiles with skills, commands, rules, and instructions, then install them to any project.

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
└── agents/             # Agent configurations
```

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

## Crates

- `dot-agent-cli`: CLI binary and optional GUI
- `dot-agent-core`: Core library for profile management

## License

MIT
