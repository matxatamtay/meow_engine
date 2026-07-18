mod cache;
mod cascade;
mod model;
mod variables;

pub use cache::{StyleEngine, compute_styles};
pub use model::{
    CascadeOrigin, CascadeStylesheet, ComputedElementStyle, ComputedStyle, ComputedStyleSnapshot,
    DirtyFlag, InvalidationReport, RestyleReport, StyleDiagnostic, ValueDiagnostic,
};

#[cfg(test)]
mod tests;
