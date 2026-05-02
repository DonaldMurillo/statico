pub mod discovery;
pub mod manager;
pub mod pipeline;
pub mod protocol;
pub mod runtime;

// Re-export commonly used types.
pub use discovery::{DiscoveredPlugin, PluginKind};
pub use manager::ActivePlugin;
pub use pipeline::PluginPipeline;
pub use protocol::{HookMode, HookName};
