# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- **ConflictResolver trait** (`dot-agent-core`): Strategy-pattern interface for resolving file conflicts during install. Implementations receive the conflicting file path and return a `Resolution` (`KeepLocal`, `OverwriteWithProfile`, or `Abort`).
- **Resolution enum** (`dot-agent-core`): Three-variant enum representing conflict resolution outcomes — `KeepLocal`, `OverwriteWithProfile`, `Abort`.
- **`conflict_resolver` field in `InstallOptions`** (`dot-agent-core`): `Option<&dyn ConflictResolver>` field added to `InstallOptions`. Defaults to `None`, preserving existing behaviour (conflicts are skipped and reported).
- **`DotAgentError::Aborted` variant** (`dot-agent-core`): New error variant returned when a `ConflictResolver` returns `Resolution::Abort`. Maps to exit code 32.
- **`ForceResolver`** (`dot-agent-cli`): `ConflictResolver` implementation that always returns `OverwriteWithProfile`. Used internally when `--force` is passed to `switch`.
- **`InteractiveResolver`** (`dot-agent-cli`): `ConflictResolver` implementation that prompts the user at the terminal (`k` = keep local, `o` = overwrite, `a` = abort) for each conflicting file during `switch`.
- **Interactive `switch` conflict flow** (`dot-agent-cli`): `handle_switch` now performs a three-phase flow — diff check → remove old profile → install new profile with resolver. Local modifications (`MODIFIED` / `ADDED` files) are displayed before removal. Without `--force`, the user is prompted interactively for each conflict.
- **Integration tests for ConflictResolver** (`dot-agent-core`): Four tests covering `KeepLocal`, `OverwriteWithProfile`, `Abort`, and the no-resolver fallback path in `install()`.
