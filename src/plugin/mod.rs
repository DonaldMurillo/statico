pub mod discovery;
pub mod manager;
pub mod protocol;
pub mod runtime;

// Re-export commonly used types.
pub use discovery::{DiscoveredPlugin, PluginKind};
pub use manager::ActivePlugin;
pub use protocol::{HookName, HookMode};
