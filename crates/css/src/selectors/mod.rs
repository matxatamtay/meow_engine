//! Selector parsing, specificity, and the W10 semantic selector model.

mod model;
mod parser;

pub use model::{
    AnPlusB, AttributeCaseSensitivity, AttributeMatcher, AttributeSelector, Combinator,
    ComplexSelector, CompoundSelector, PseudoClass, SelectorList, SelectorParseError,
    SelectorSegment, SimpleSelector, Specificity, TypeSelector,
};
pub use parser::parse_selector_list;

#[cfg(test)]
mod tests;
