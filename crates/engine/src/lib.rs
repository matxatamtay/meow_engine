//! Top-level orchestration crate for MeowEngine.

/// Human-readable engine name used by first-party applications.
pub const ENGINE_NAME: &str = "MeowEngine";

/// Returns the workspace package version embedded at compile time.
#[must_use]
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
