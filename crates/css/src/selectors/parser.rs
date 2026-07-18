use std::fmt;

use cssparser::{ParseError, Parser, ParserInput, Token, parse_nth};

use crate::CssSourceLocation;

use super::model::{
    AnPlusB, AttributeCaseSensitivity, AttributeMatcher, AttributeSelector, Combinator,
    ComplexSelector, CompoundSelector, PseudoClass, SelectorList, SelectorParseError,
    SelectorSegment, SimpleSelector, Specificity, TypeSelector,
};

/// Parses one complete comma-separated selector list.
pub fn parse_selector_list(source: &str) -> Result<SelectorList, SelectorParseError> {
    let mut input = ParserInput::new(source);
    let mut input = Parser::new(&mut input);
    let result = input.parse_comma_separated(parse_complex_selector);
    match result {
        Ok(selectors) if selectors.is_empty() => Err(SelectorParseError {
            location: CssSourceLocation { line: 1, column: 1 },
            message: "selector list is empty".to_owned(),
        }),
        Ok(selectors) => Ok(SelectorList { selectors }),
        Err(error) => Err(SelectorParseError {
            location: error.location.into(),
            message: error.to_string(),
        }),
    }
}

fn parse_complex_selector<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<ComplexSelector, ParseError<'i, SelectorSyntaxError>> {
    input.skip_whitespace();
    if input.is_exhausted() {
        return Err(input.new_custom_error(SelectorSyntaxError::EmptySelector));
    }

    let first = parse_compound_selector(input)?;
    let mut specificity = compound_specificity(&first);
    let mut segments = vec![SelectorSegment {
        combinator: None,
        compound: first,
    }];

    loop {
        let had_whitespace = consume_whitespace(input);
        if input.is_exhausted() {
            break;
        }

        let state = input.state();
        let token = input.next_including_whitespace()?.clone();
        let combinator = match token {
            Token::Delim('>') => {
                consume_whitespace(input);
                Combinator::Child
            }
            Token::Delim('+') => {
                consume_whitespace(input);
                Combinator::NextSibling
            }
            Token::Delim('~') => {
                consume_whitespace(input);
                Combinator::SubsequentSibling
            }
            _ if had_whitespace => {
                input.reset(&state);
                Combinator::Descendant
            }
            other => {
                return Err(
                    input.new_custom_error(SelectorSyntaxError::ExpectedCombinator(token_name(
                        &other,
                    ))),
                );
            }
        };

        if input.is_exhausted() {
            return Err(input.new_custom_error(SelectorSyntaxError::TrailingCombinator));
        }
        let compound = parse_compound_selector(input)?;
        specificity.add(compound_specificity(&compound));
        segments.push(SelectorSegment {
            combinator: Some(combinator),
            compound,
        });
    }

    Ok(ComplexSelector {
        segments,
        specificity,
    })
}

fn parse_compound_selector<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<CompoundSelector, ParseError<'i, SelectorSyntaxError>> {
    let mut compound = CompoundSelector::default();
    let start = input.state();
    match input.next_including_whitespace() {
        Ok(Token::Ident(name)) => {
            compound.type_selector = Some(TypeSelector::Type(name.to_string()));
        }
        Ok(Token::Delim('*')) => compound.type_selector = Some(TypeSelector::Universal),
        Ok(_) | Err(_) => input.reset(&start),
    }

    loop {
        let state = input.state();
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(_) => break,
        };
        match token {
            Token::WhiteSpace(_) => {
                input.reset(&state);
                break;
            }
            Token::IDHash(id) => compound
                .simple_selectors
                .push(SimpleSelector::Id(id.to_string())),
            Token::Hash(_) => {
                return Err(input.new_custom_error(SelectorSyntaxError::InvalidIdSelector));
            }
            Token::Delim('.') => {
                let class = expect_adjacent_ident(input, "class selector")?;
                compound.simple_selectors.push(SimpleSelector::Class(class));
            }
            Token::SquareBracketBlock => {
                let attribute = input.parse_nested_block(parse_attribute_selector)?;
                compound
                    .simple_selectors
                    .push(SimpleSelector::Attribute(attribute));
            }
            Token::Colon => {
                let pseudo = parse_pseudo_class(input)?;
                compound
                    .simple_selectors
                    .push(SimpleSelector::PseudoClass(pseudo));
            }
            Token::Delim('>') | Token::Delim('+') | Token::Delim('~') => {
                input.reset(&state);
                break;
            }
            other => {
                return Err(input
                    .new_custom_error(SelectorSyntaxError::UnexpectedToken(token_name(&other))));
            }
        }
    }

    if compound.type_selector.is_none() && compound.simple_selectors.is_empty() {
        return Err(input.new_custom_error(SelectorSyntaxError::EmptyCompound));
    }
    Ok(compound)
}

fn parse_attribute_selector<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<AttributeSelector, ParseError<'i, SelectorSyntaxError>> {
    let name = input.expect_ident()?.to_string();
    if input.is_exhausted() {
        return Ok(AttributeSelector {
            name,
            matcher: AttributeMatcher::Exists,
            case_sensitivity: AttributeCaseSensitivity::Default,
        });
    }

    let operator = input.next()?.clone();
    let value = match input.next()?.clone() {
        Token::Ident(value) | Token::QuotedString(value) => value.to_string(),
        other => {
            return Err(
                input.new_custom_error(SelectorSyntaxError::ExpectedAttributeValue(token_name(
                    &other,
                ))),
            );
        }
    };
    let matcher =
        match operator {
            Token::Delim('=') => AttributeMatcher::Equals(value),
            Token::IncludeMatch => AttributeMatcher::Includes(value),
            Token::DashMatch => AttributeMatcher::DashMatch(value),
            Token::PrefixMatch => AttributeMatcher::Prefix(value),
            Token::SuffixMatch => AttributeMatcher::Suffix(value),
            Token::SubstringMatch => AttributeMatcher::Substring(value),
            other => {
                return Err(input.new_custom_error(
                    SelectorSyntaxError::ExpectedAttributeOperator(token_name(&other)),
                ));
            }
        };

    let case_sensitivity = if input.is_exhausted() {
        AttributeCaseSensitivity::Default
    } else {
        let modifier = input.expect_ident()?.to_string();
        if modifier.eq_ignore_ascii_case("i") {
            AttributeCaseSensitivity::AsciiInsensitive
        } else if modifier.eq_ignore_ascii_case("s") {
            AttributeCaseSensitivity::CaseSensitive
        } else {
            return Err(
                input.new_custom_error(SelectorSyntaxError::UnknownAttributeModifier(modifier))
            );
        }
    };
    input.expect_exhausted()?;

    Ok(AttributeSelector {
        name,
        matcher,
        case_sensitivity,
    })
}

fn parse_pseudo_class<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<PseudoClass, ParseError<'i, SelectorSyntaxError>> {
    let token = input.next_including_whitespace()?.clone();
    match token {
        Token::Ident(name) => parse_plain_pseudo(input, name.as_ref()),
        Token::Function(name) => {
            let lowered = name.to_ascii_lowercase();
            input.parse_nested_block(|nested| parse_functional_pseudo(nested, &lowered))
        }
        Token::Colon => Err(input.new_custom_error(SelectorSyntaxError::PseudoElementsUnsupported)),
        other => {
            Err(input
                .new_custom_error(SelectorSyntaxError::ExpectedPseudoClass(token_name(&other))))
        }
    }
}

fn parse_plain_pseudo<'i>(
    input: &mut Parser<'i, '_>,
    name: &str,
) -> Result<PseudoClass, ParseError<'i, SelectorSyntaxError>> {
    let pseudo = if name.eq_ignore_ascii_case("root") {
        PseudoClass::Root
    } else if name.eq_ignore_ascii_case("empty") {
        PseudoClass::Empty
    } else if name.eq_ignore_ascii_case("first-child") {
        PseudoClass::FirstChild
    } else if name.eq_ignore_ascii_case("last-child") {
        PseudoClass::LastChild
    } else if name.eq_ignore_ascii_case("only-child") {
        PseudoClass::OnlyChild
    } else if name.eq_ignore_ascii_case("first-of-type") {
        PseudoClass::FirstOfType
    } else if name.eq_ignore_ascii_case("last-of-type") {
        PseudoClass::LastOfType
    } else if name.eq_ignore_ascii_case("only-of-type") {
        PseudoClass::OnlyOfType
    } else {
        return Err(
            input.new_custom_error(SelectorSyntaxError::UnsupportedPseudoClass(name.to_owned()))
        );
    };
    Ok(pseudo)
}

fn parse_functional_pseudo<'i>(
    input: &mut Parser<'i, '_>,
    name: &str,
) -> Result<PseudoClass, ParseError<'i, SelectorSyntaxError>> {
    let (a, b) = parse_nth(input)?;
    input.expect_exhausted()?;
    let expression = AnPlusB { a, b };
    if name.eq_ignore_ascii_case("nth-child") {
        Ok(PseudoClass::NthChild(expression))
    } else if name.eq_ignore_ascii_case("nth-last-child") {
        Ok(PseudoClass::NthLastChild(expression))
    } else if name.eq_ignore_ascii_case("nth-of-type") {
        Ok(PseudoClass::NthOfType(expression))
    } else if name.eq_ignore_ascii_case("nth-last-of-type") {
        Ok(PseudoClass::NthLastOfType(expression))
    } else {
        Err(input.new_custom_error(SelectorSyntaxError::UnsupportedPseudoClass(name.to_owned())))
    }
}

fn expect_adjacent_ident<'i>(
    input: &mut Parser<'i, '_>,
    context: &'static str,
) -> Result<String, ParseError<'i, SelectorSyntaxError>> {
    match input.next_including_whitespace()?.clone() {
        Token::Ident(value) => Ok(value.to_string()),
        other => Err(
            input.new_custom_error(SelectorSyntaxError::ExpectedAdjacentIdent {
                context,
                found: token_name(&other),
            }),
        ),
    }
}

fn consume_whitespace(input: &mut Parser<'_, '_>) -> bool {
    let mut consumed = false;
    loop {
        let state = input.state();
        match input.next_including_whitespace() {
            Ok(Token::WhiteSpace(_)) => consumed = true,
            Ok(_) | Err(_) => {
                input.reset(&state);
                break;
            }
        }
    }
    consumed
}

fn compound_specificity(compound: &CompoundSelector) -> Specificity {
    let mut specificity = Specificity::default();
    if matches!(compound.type_selector, Some(TypeSelector::Type(_))) {
        specificity.types = 1;
    }
    for selector in &compound.simple_selectors {
        match selector {
            SimpleSelector::Id(_) => specificity.ids = specificity.ids.saturating_add(1),
            SimpleSelector::Class(_)
            | SimpleSelector::Attribute(_)
            | SimpleSelector::PseudoClass(_) => {
                specificity.classes = specificity.classes.saturating_add(1);
            }
        }
    }
    specificity
}

fn token_name(token: &Token<'_>) -> String {
    match token {
        Token::Ident(value) => format!("identifier {value:?}"),
        Token::IDHash(value) | Token::Hash(value) => format!("hash {value:?}"),
        Token::QuotedString(value) => format!("string {value:?}"),
        Token::Delim(value) => format!("delimiter {value:?}"),
        Token::WhiteSpace(_) => "whitespace".to_owned(),
        Token::Colon => "colon".to_owned(),
        Token::Comma => "comma".to_owned(),
        Token::SquareBracketBlock => "attribute block".to_owned(),
        Token::Function(value) => format!("function {value:?}"),
        other => format!("{other:?}"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectorSyntaxError {
    EmptySelector,
    EmptyCompound,
    InvalidIdSelector,
    ExpectedCombinator(String),
    TrailingCombinator,
    UnexpectedToken(String),
    ExpectedAdjacentIdent {
        context: &'static str,
        found: String,
    },
    ExpectedAttributeOperator(String),
    ExpectedAttributeValue(String),
    UnknownAttributeModifier(String),
    ExpectedPseudoClass(String),
    UnsupportedPseudoClass(String),
    PseudoElementsUnsupported,
}

impl fmt::Display for SelectorSyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySelector => formatter.write_str("selector list contains an empty selector"),
            Self::EmptyCompound => formatter.write_str("expected a compound selector"),
            Self::InvalidIdSelector => formatter.write_str("invalid ID selector"),
            Self::ExpectedCombinator(found) => {
                write!(formatter, "expected a combinator before {found}")
            }
            Self::TrailingCombinator => formatter.write_str("selector ends with a combinator"),
            Self::UnexpectedToken(found) => write!(formatter, "unexpected {found} in selector"),
            Self::ExpectedAdjacentIdent { context, found } => {
                write!(
                    formatter,
                    "expected an identifier adjacent to {context}, found {found}"
                )
            }
            Self::ExpectedAttributeOperator(found) => {
                write!(formatter, "expected an attribute operator, found {found}")
            }
            Self::ExpectedAttributeValue(found) => {
                write!(formatter, "expected an attribute value, found {found}")
            }
            Self::UnknownAttributeModifier(modifier) => {
                write!(formatter, "unknown attribute case modifier {modifier:?}")
            }
            Self::ExpectedPseudoClass(found) => {
                write!(formatter, "expected a pseudo-class name, found {found}")
            }
            Self::UnsupportedPseudoClass(name) => {
                write!(formatter, "unsupported pseudo-class {name:?}")
            }
            Self::PseudoElementsUnsupported => {
                formatter.write_str("pseudo-elements are outside the W10 selector subset")
            }
        }
    }
}
