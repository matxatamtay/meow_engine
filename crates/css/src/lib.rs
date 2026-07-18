//! CSS syntax parsing, selector semantics, declaration recovery, and deterministic snapshots.

mod properties;
mod selectors;

use std::fmt;

pub use properties::{
    ALL_PROPERTIES, CssWideKeyword, PropertyDeclaration, PropertyId, SpecifiedValue,
    parse_property_declaration,
};
pub use selectors::{
    AnPlusB, AttributeCaseSensitivity, AttributeMatcher, AttributeSelector, Combinator,
    ComplexSelector, CompoundSelector, PseudoClass, SelectorList, SelectorParseError,
    SelectorSegment, SimpleSelector, Specificity, TypeSelector, parse_selector_list,
};

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, SourceLocation, StyleSheetParser,
    Token, parse_important,
};

/// A one-based source location suitable for diagnostics and snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssSourceLocation {
    /// One-based line number.
    pub line: u32,
    /// One-based UTF-16 column number.
    pub column: u32,
}

impl From<SourceLocation> for CssSourceLocation {
    fn from(location: SourceLocation) -> Self {
        Self {
            line: location.line + 1,
            column: location.column,
        }
    }
}

/// A recoverable CSS syntax diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Location where parsing failed.
    pub location: CssSourceLocation,
    /// Human-readable parser error.
    pub message: String,
    /// Source fragment skipped by error recovery.
    pub source: String,
}

/// Parsed stylesheet preserving source order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stylesheet {
    /// Successfully parsed rules.
    pub rules: Vec<Rule>,
    /// Recoverable syntax errors.
    pub diagnostics: Vec<Diagnostic>,
}

impl Stylesheet {
    /// Produces a deterministic text snapshot of rules and diagnostics.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::new();
        for rule in &self.rules {
            match rule {
                Rule::Style(rule) => {
                    output.push_str(&format!(
                        "rule[{}] style @{}:{} selectors={:?}\n",
                        rule.source_order, rule.location.line, rule.location.column, rule.selectors
                    ));
                    for (index, declaration) in rule.declarations.iter().enumerate() {
                        output.push_str(&format!(
                            "  declaration[{index}] @{}:{} {}={:?} important={}\n",
                            declaration.location.line,
                            declaration.location.column,
                            declaration.name,
                            declaration.value,
                            declaration.important
                        ));
                    }
                }
                Rule::At(rule) => {
                    output.push_str(&format!(
                        "rule[{}] at @{}:{} name={:?} prelude={:?} block={:?}\n",
                        rule.source_order,
                        rule.location.line,
                        rule.location.column,
                        rule.name,
                        rule.prelude,
                        rule.block
                    ));
                }
            }
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            output.push_str(&format!(
                "error[{index}] @{}:{} {} source={:?}\n",
                diagnostic.location.line,
                diagnostic.location.column,
                diagnostic.message,
                diagnostic.source
            ));
        }
        output
    }
}

/// A top-level CSS rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Qualified style rule.
    Style(StyleRule),
    /// At-rule retained as syntax for a later semantic stage.
    At(AtRule),
}

/// A qualified style rule and its declaration list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleRule {
    /// Raw selector prelude after trimming outer whitespace.
    pub selectors: String,
    /// Syntactically valid declarations.
    pub declarations: Vec<Declaration>,
    /// Zero-based order within the stylesheet.
    pub source_order: usize,
    /// Rule start location.
    pub location: CssSourceLocation,
}

impl StyleRule {
    /// Parses the raw W9 selector prelude into the W10 semantic selector model.
    pub fn selector_list(&self) -> Result<SelectorList, SelectorParseError> {
        parse_selector_list(&self.selectors)
    }
}

/// A parsed property declaration without property-specific value validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// Property name. Standard names are ASCII-lowercased; custom properties retain case.
    pub name: String,
    /// Raw declaration value with trailing `!important` removed.
    pub value: String,
    /// Whether the declaration ended in `!important`.
    pub important: bool,
    /// Declaration start location.
    pub location: CssSourceLocation,
}

/// A top-level at-rule retained for later semantic handling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtRule {
    /// At-keyword without `@`.
    pub name: String,
    /// Raw prelude after trimming outer whitespace.
    pub prelude: String,
    /// Raw block body, or `None` for semicolon-terminated rules.
    pub block: Option<String>,
    /// Zero-based order within the stylesheet.
    pub source_order: usize,
    /// Rule start location.
    pub location: CssSourceLocation,
}

/// Parses one complete stylesheet with CSS Syntax error recovery.
#[must_use]
pub fn parse_stylesheet(source: &str) -> Stylesheet {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let mut adapter = StylesheetAdapter::default();
    let mut stylesheet = Stylesheet::default();

    for result in StyleSheetParser::new(&mut input, &mut adapter) {
        match result {
            Ok(rule) => stylesheet.rules.push(rule),
            Err((error, fragment)) => stylesheet.diagnostics.push(to_diagnostic(error, fragment)),
        }
    }
    stylesheet.diagnostics.extend(adapter.diagnostics);
    stylesheet
        .diagnostics
        .sort_by_key(|diagnostic| (diagnostic.location.line, diagnostic.location.column));
    stylesheet
}

#[derive(Default)]
struct StylesheetAdapter {
    next_source_order: usize,
    diagnostics: Vec<Diagnostic>,
}

impl StylesheetAdapter {
    fn source_order(&mut self) -> usize {
        let source_order = self.next_source_order;
        self.next_source_order += 1;
        source_order
    }
}

impl<'i> QualifiedRuleParser<'i> for StylesheetAdapter {
    type Prelude = String;
    type QualifiedRule = Rule;
    type Error = SyntaxError;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        let selectors = consume_raw(input);
        if selectors.is_empty() {
            return Err(input.new_custom_error(SyntaxError::EmptySelector));
        }
        Ok(selectors)
    }

    fn parse_block<'t>(
        &mut self,
        selectors: Self::Prelude,
        start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::QualifiedRule, ParseError<'i, Self::Error>> {
        let mut declarations = Vec::new();
        let mut declaration_parser = DeclarationAdapter;
        for result in RuleBodyParser::new(input, &mut declaration_parser) {
            match result {
                Ok(declaration) => declarations.push(declaration),
                Err((error, fragment)) => self.diagnostics.push(to_diagnostic(error, fragment)),
            }
        }
        Ok(Rule::Style(StyleRule {
            selectors,
            declarations,
            source_order: self.source_order(),
            location: start.source_location().into(),
        }))
    }
}

#[derive(Clone, Debug)]
struct AtPrelude {
    name: String,
    prelude: String,
}

impl<'i> AtRuleParser<'i> for StylesheetAdapter {
    type Prelude = AtPrelude;
    type AtRule = Rule;
    type Error = SyntaxError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::Prelude, ParseError<'i, Self::Error>> {
        Ok(AtPrelude {
            name: name.to_ascii_lowercase(),
            prelude: consume_raw(input),
        })
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        start: &ParserState,
    ) -> Result<Self::AtRule, ()> {
        Ok(Rule::At(AtRule {
            name: prelude.name,
            prelude: prelude.prelude,
            block: None,
            source_order: self.source_order(),
            location: start.source_location().into(),
        }))
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Self::AtRule, ParseError<'i, Self::Error>> {
        Ok(Rule::At(AtRule {
            name: prelude.name,
            prelude: prelude.prelude,
            block: Some(consume_raw(input)),
            source_order: self.source_order(),
            location: start.source_location().into(),
        }))
    }
}

struct DeclarationAdapter;

impl<'i> DeclarationParser<'i> for DeclarationAdapter {
    type Declaration = Declaration;
    type Error = SyntaxError;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        declaration_start: &ParserState,
    ) -> Result<Self::Declaration, ParseError<'i, Self::Error>> {
        let value_start = input.position();
        let mut important_start = None;

        while !input.is_exhausted() {
            let state = input.state();
            match input.next_including_whitespace() {
                Ok(Token::Delim('!')) => {
                    input.reset(&state);
                    let is_important = input
                        .try_parse(|candidate| {
                            parse_important(candidate)?;
                            candidate.expect_exhausted()
                        })
                        .is_ok();
                    if is_important {
                        important_start = Some(state.position());
                        break;
                    }
                    input.reset(&state);
                    let _ = input.next_including_whitespace();
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        let value_end = important_start.unwrap_or_else(|| input.position());
        let raw_name = name.as_ref();
        let name = if raw_name.starts_with("--") {
            raw_name.to_owned()
        } else {
            raw_name.to_ascii_lowercase()
        };

        Ok(Declaration {
            name,
            value: input.slice(value_start..value_end).trim().to_owned(),
            important: important_start.is_some(),
            location: declaration_start.source_location().into(),
        })
    }
}

impl<'i> AtRuleParser<'i> for DeclarationAdapter {
    type Prelude = ();
    type AtRule = Declaration;
    type Error = SyntaxError;
}

impl<'i> QualifiedRuleParser<'i> for DeclarationAdapter {
    type Prelude = ();
    type QualifiedRule = Declaration;
    type Error = SyntaxError;
}

impl<'i> RuleBodyItemParser<'i, Declaration, SyntaxError> for DeclarationAdapter {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SyntaxError {
    EmptySelector,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySelector => formatter.write_str("empty selector prelude"),
        }
    }
}

fn consume_raw(input: &mut Parser<'_, '_>) -> String {
    let start = input.position();
    while input.next_including_whitespace_and_comments().is_ok() {}
    input.slice_from(start).trim().to_owned()
}

fn to_diagnostic(error: ParseError<'_, SyntaxError>, fragment: &str) -> Diagnostic {
    Diagnostic {
        location: error.location.into(),
        message: error.to_string(),
        source: fragment.trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules_declarations_and_important() {
        let stylesheet = parse_stylesheet(
            "article, .card { Color: red; width: calc(100% - 2px) ! important; --Theme: dark; }",
        );

        assert!(stylesheet.diagnostics.is_empty());
        let Rule::Style(rule) = &stylesheet.rules[0] else {
            panic!("expected style rule");
        };
        assert_eq!(rule.selectors, "article, .card");
        assert_eq!(rule.declarations.len(), 3);
        assert_eq!(rule.declarations[0].name, "color");
        assert_eq!(rule.declarations[1].value, "calc(100% - 2px)");
        assert!(rule.declarations[1].important);
        assert_eq!(rule.declarations[2].name, "--Theme");
    }

    #[test]
    fn recovers_after_invalid_declarations_and_rules() {
        let stylesheet = parse_stylesheet(
            "a { color: red; broken; width: 1px } { nope: value } p { display: block }",
        );

        assert_eq!(stylesheet.rules.len(), 2);
        assert_eq!(stylesheet.diagnostics.len(), 2);
        let Rule::Style(first) = &stylesheet.rules[0] else {
            panic!("expected first style rule");
        };
        assert_eq!(first.declarations.len(), 2);
        let Rule::Style(second) = &stylesheet.rules[1] else {
            panic!("expected second style rule");
        };
        assert_eq!(second.selectors, "p");
    }

    #[test]
    fn retains_at_rules_for_later_semantic_parsing() {
        let stylesheet =
            parse_stylesheet("@import url(theme.css); @media screen { a { color: red } }");

        assert_eq!(stylesheet.rules.len(), 2);
        let Rule::At(import) = &stylesheet.rules[0] else {
            panic!("expected import rule");
        };
        assert_eq!(import.name, "import");
        assert!(import.block.is_none());
        let Rule::At(media) = &stylesheet.rules[1] else {
            panic!("expected media rule");
        };
        assert_eq!(media.name, "media");
        assert!(
            media
                .block
                .as_deref()
                .is_some_and(|body| body.contains("a"))
        );
    }
}
