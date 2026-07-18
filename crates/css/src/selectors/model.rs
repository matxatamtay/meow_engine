use std::{error::Error, fmt};

use crate::CssSourceLocation;

use super::parser::parse_selector_list;

/// A comma-separated selector list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorList {
    /// Complex selectors in source order.
    pub selectors: Vec<ComplexSelector>,
}

impl SelectorList {
    /// Parses a complete selector list.
    pub fn parse(source: &str) -> Result<Self, SelectorParseError> {
        parse_selector_list(source)
    }

    /// Returns the maximum specificity in the list.
    #[must_use]
    pub fn max_specificity(&self) -> Specificity {
        self.selectors
            .iter()
            .map(ComplexSelector::specificity)
            .max()
            .unwrap_or_default()
    }
}

/// One selector made from compound selectors connected by combinators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplexSelector {
    /// Segments stored left-to-right. The first segment has no combinator.
    pub segments: Vec<SelectorSegment>,
    pub(super) specificity: Specificity,
}

impl ComplexSelector {
    /// Returns the selector specificity.
    #[must_use]
    pub const fn specificity(&self) -> Specificity {
        self.specificity
    }
}

/// One compound selector and its relation to the preceding segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorSegment {
    /// Relation from the preceding segment to this segment.
    pub combinator: Option<Combinator>,
    /// Simple selectors that all apply to one element.
    pub compound: CompoundSelector,
}

/// A set of simple selectors applying to the same element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompoundSelector {
    /// Optional type or universal selector.
    pub type_selector: Option<TypeSelector>,
    /// ID, class, attribute, and pseudo-class selectors.
    pub simple_selectors: Vec<SimpleSelector>,
}

/// A type selector at the start of a compound selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeSelector {
    /// `*` universal selector.
    Universal,
    /// Element local name.
    Type(String),
}

/// A non-type simple selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimpleSelector {
    /// `#id` selector.
    Id(String),
    /// `.class` selector.
    Class(String),
    /// Attribute selector.
    Attribute(AttributeSelector),
    /// Supported structural pseudo-class.
    PseudoClass(PseudoClass),
}

/// A relation between two compound selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    /// Whitespace descendant combinator.
    Descendant,
    /// `>` child combinator.
    Child,
    /// `+` adjacent sibling combinator.
    NextSibling,
    /// `~` general sibling combinator.
    SubsequentSibling,
}

/// An attribute selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeSelector {
    /// Attribute local name.
    pub name: String,
    /// Existence or value matcher.
    pub matcher: AttributeMatcher,
    /// Explicit selector value case modifier.
    pub case_sensitivity: AttributeCaseSensitivity,
}

/// Attribute selector matching operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeMatcher {
    /// `[name]`.
    Exists,
    /// `[name=value]`.
    Equals(String),
    /// `[name~=word]`.
    Includes(String),
    /// `[name|=prefix]`.
    DashMatch(String),
    /// `[name^=prefix]`.
    Prefix(String),
    /// `[name$=suffix]`.
    Suffix(String),
    /// `[name*=fragment]`.
    Substring(String),
}

/// Attribute value comparison mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttributeCaseSensitivity {
    /// Use the document-language default. W10 treats values as case-sensitive.
    #[default]
    Default,
    /// ASCII case-insensitive `i` modifier.
    AsciiInsensitive,
    /// Explicit case-sensitive `s` modifier.
    CaseSensitive,
}

/// Structural pseudo-classes supported by W10.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    /// `:root`.
    Root,
    /// `:empty`.
    Empty,
    /// `:first-child`.
    FirstChild,
    /// `:last-child`.
    LastChild,
    /// `:only-child`.
    OnlyChild,
    /// `:nth-child(An+B)`.
    NthChild(AnPlusB),
    /// `:nth-last-child(An+B)`.
    NthLastChild(AnPlusB),
    /// `:first-of-type`.
    FirstOfType,
    /// `:last-of-type`.
    LastOfType,
    /// `:only-of-type`.
    OnlyOfType,
    /// `:nth-of-type(An+B)`.
    NthOfType(AnPlusB),
    /// `:nth-last-of-type(An+B)`.
    NthLastOfType(AnPlusB),
}

/// Parsed `An+B` expression used by `:nth-*()` pseudo-classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnPlusB {
    /// Multiplier of `n`.
    pub a: i32,
    /// Constant offset.
    pub b: i32,
}

impl AnPlusB {
    /// Returns whether a one-based index matches this expression.
    #[must_use]
    pub fn matches(self, index: usize) -> bool {
        let Ok(index) = i64::try_from(index) else {
            return false;
        };
        let a = i64::from(self.a);
        let b = i64::from(self.b);
        if a == 0 {
            return index == b;
        }
        let delta = index - b;
        delta % a == 0 && delta / a >= 0
    }
}

/// CSS specificity represented as `(id, class/attribute/pseudo, type)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    /// Number of ID selectors.
    pub ids: u32,
    /// Number of class, attribute, and pseudo-class selectors.
    pub classes: u32,
    /// Number of type selectors.
    pub types: u32,
}

impl Specificity {
    pub(super) fn add(&mut self, other: Self) {
        self.ids = self.ids.saturating_add(other.ids);
        self.classes = self.classes.saturating_add(other.classes);
        self.types = self.types.saturating_add(other.types);
    }
}

/// Selector syntax failure with a one-based source location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorParseError {
    /// Location where parsing failed.
    pub location: CssSourceLocation,
    /// Human-readable failure reason.
    pub message: String,
}

impl fmt::Display for SelectorParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "selector parse error at {}:{}: {}",
            self.location.line, self.location.column, self.message
        )
    }
}

impl Error for SelectorParseError {}
