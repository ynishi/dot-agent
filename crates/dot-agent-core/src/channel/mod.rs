//! Channel and Hub management
//!
//! # Concepts
//!
//! - **Hub**: A repository that aggregates multiple Channels (e.g., dot-agent-hub)
//! - **Channel**: A source of profiles (e.g., awesome-dotfiles, official)
//! - **Profile**: Actual configuration files
//!
//! # Hierarchy
//!
//! ```text
//! Hub (github.com/xxx/dot-agent-hub)
//! ├── channels/
//! │   ├── awesome-dotfiles.toml
//! │   └── awesome-neovim.toml
//! └── official/
//!     └── profiles/
//!         ├── rust-claude.toml
//!         └── python-claude.toml
//!
//! Local (~/.dot-agent/)
//! ├── hubs.toml              # Registered Hubs
//! ├── channels.toml          # Enabled Channels
//! ├── cache/
//! │   ├── hubs/              # Hub content cache
//! │   └── channels/          # Channel content cache
//! └── profiles/              # Imported profiles
//! ```

mod channel_registry;
mod hub_registry;
mod search;
mod types;

pub use channel_registry::ChannelRegistry;
pub use hub_registry::HubRegistry;
pub use search::ChannelManager;
pub use types::{Channel, ChannelRef, ChannelSource, ChannelType, Hub, ProfileRef, SearchOptions};
