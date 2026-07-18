//! Internal CSS component-value helpers and W9 snapshot compatibility.

use cssparser::{ParseError, Parser, ParserInput, Token, parse_important};

use crate::{Declaration, SyntaxError};

pub(super) fn consume_component_values<'i, 't>(
    input: &mut Parser<'i, 't>,
) -> Result<(), ParseError<'i, SyntaxError>> {
    while !input.is_exhausted() {
        let nested_block = matches!(
            input.next_including_whitespace_and_comments(),
            Ok(Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock)
        );
        if nested_block {
            input.parse_nested_block(consume_component_values)?;
        }
    }
    Ok(())
}

pub(super) fn legacy_declaration_value(declaration: &Declaration) -> String {
    let source = if declaration.important {
        format!("{} !important", declaration.value)
    } else {
        declaration.value.clone()
    };
    let mut input = ParserInput::new(&source);
    let mut input = Parser::new(&mut input);
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
    input.slice(value_start..value_end).trim().to_owned()
}
