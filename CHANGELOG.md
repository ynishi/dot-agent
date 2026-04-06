# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [0.5.0] - 2026-04-06

### Added

- **dot-agent-mcp server** (new crate): MCP server for profile management via Claude Code. Tools: `list`, `installed`, `install`, `remove`, `switch`, `diff`, `sync_back`, `status`, `snapshot_list`, `snapshot_restore`. Follows established MCP patterns (Self::to_mcp_error, tool annotations, fail-fast on multi-step operations).
- **Auto-snapshot on destructive operations** (`dot-agent-mcp`): `install`, `remove`, `switch` automatically save a snapshot before execution. `snapshot_restore` saves a pre-restore snapshot for safety.
- **`snapshot_list` tool** (`dot-agent-mcp`): List all snapshots for a target directory with timestamps, file counts, and trigger types.
- **`snapshot_restore` tool** (`dot-agent-mcp`): Restore a snapshot to a target directory with pre-restore snapshot for rollback.
- **`sync-back` command** (`dot-agent-cli`): Write modified installed files back to the source profile.
- **`--keep-local` and `--interactive` flags** (`dot-agent-cli`): New flags for `switch` command to control conflict resolution behaviour.
- **ConflictResolver trait** (`dot-agent-core`): Strategy-pattern interface for resolving file conflicts during install. Implementations receive the conflicting file path and return a `Resolution` (`KeepLocal`, `OverwriteWithProfile`, or `Abort`).
- **Resolution enum** (`dot-agent-core`): Three-variant enum representing conflict resolution outcomes — `KeepLocal`, `OverwriteWithProfile`, `Abort`.
- **`conflict_resolver` field in `InstallOptions`** (`dot-agent-core`): `Option<&dyn ConflictResolver>` field added to `InstallOptions`. Defaults to `None`, preserving existing behaviour (conflicts are skipped and reported).
- **`DotAgentError::Aborted` variant** (`dot-agent-core`): New error variant returned when a `ConflictResolver` returns `Resolution::Abort`. Maps to exit code 32.
- **`ForceResolver`** (`dot-agent-cli`): `ConflictResolver` implementation that always returns `OverwriteWithProfile`. Used internally when `--force` is passed to `switch`.
- **`InteractiveResolver`** (`dot-agent-cli`): `ConflictResolver` implementation that prompts the user at the terminal (`k` = keep local, `o` = overwrite, `a` = abort) for each conflicting file during `switch`.
- **Interactive `switch` conflict flow** (`dot-agent-cli`): `handle_switch` now performs a three-phase flow — diff check → remove old profile → install new profile with resolver.
- **Integration tests for ConflictResolver** (`dot-agent-core`): Four tests covering `KeepLocal`, `OverwriteWithProfile`, `Abort`, and the no-resolver fallback path in `install()`.

### Fixed

- **Snapshot now includes `.dot-agent-meta.toml`** (`dot-agent-core`): Metadata was previously excluded from snapshots, causing `snapshot restore` to restore files but leave installed-profiles metadata stale. Removed from `EXCLUDED_FILES` and narrowed `.dot-agent` prefix exclusion to `.dot-agent-history` only.
- **`switch` no longer silently continues on remove failure** (`dot-agent-mcp`): Previously, if removing the old profile failed (e.g., local modifications), the error was logged as a warning and the new profile was installed anyway, leaving both profiles in metadata. Now fails fast with proper error propagation.
