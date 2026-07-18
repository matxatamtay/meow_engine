//! W11 property registry and semantic declaration values.

mod model;
mod parser;

pub use model::{ALL_PROPERTIES, CssWideKeyword, PropertyDeclaration, PropertyId, SpecifiedValue};
pub use parser::parse_property_declaration;

#[cfg(test)]
mod tests;
