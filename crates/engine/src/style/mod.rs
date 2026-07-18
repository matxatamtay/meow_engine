//! W11 cascade, inheritance, and deterministic computed-style snapshots.

mod cascade;
mod model;

pub use cascade::compute_styles;
pub use model::{
    CascadeOrigin, CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot,
    StyleDiagnostic,
};

#[cfg(test)]
mod tests;
