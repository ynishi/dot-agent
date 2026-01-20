pub mod error;
pub mod installer;
pub mod metadata;
pub mod profile;

pub use error::{DotAgentError, Result};
pub use installer::{DiffResult, FileInfo, FileStatus, InstallResult, Installer};
pub use metadata::Metadata;
pub use profile::{Profile, ProfileManager};
