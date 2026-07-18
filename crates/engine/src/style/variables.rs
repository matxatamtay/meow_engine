use std::collections::{BTreeMap, BTreeSet};

use meow_css::{CssWideKeyword, parse_css_wide_keyword};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VarError {
    Missing(String),
    Cycle(String),
    Syntax(String),
}

impl std::fmt::Display for VarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(name) => write!(formatter, "missing custom property {name}"),
            Self::Cycle(name) => write!(formatter, "custom property cycle through {name}"),
            Self::Syntax(message) => formatter.write_str(message),
        }
    }
}

pub(super) fn resolve_custom_properties(
    parent: Option<&BTreeMap<String, String>>,
    winners: &BTreeMap<String, String>,
) -> (BTreeMap<String, String>, Vec<(String, VarError)>) {
    let mut raw = parent.cloned().unwrap_or_default();
    for (name, value) in winners {
        match parse_css_wide_keyword(value) {
            Some(CssWideKeyword::Initial) => {
                raw.remove(name);
            }
            Some(CssWideKeyword::Inherit | CssWideKeyword::Unset) => {}
            None => {
                raw.insert(name.clone(), value.clone());
            }
        }
    }

    let mut resolved = BTreeMap::new();
    let mut errors = Vec::new();
    for name in raw.keys() {
        let mut stack = BTreeSet::new();
        match resolve_name(name, &raw, &mut resolved, &mut stack) {
            Ok(_) => {}
            Err(error) => errors.push((name.clone(), error)),
        }
    }
    (resolved, errors)
}

pub(super) fn substitute_vars(
    source: &str,
    custom_properties: &BTreeMap<String, String>,
) -> Result<String, VarError> {
    resolve_text(source, &mut |name| {
        custom_properties
            .get(name)
            .cloned()
            .ok_or_else(|| VarError::Missing(name.to_owned()))
    })
}

fn resolve_name(
    name: &str,
    raw: &BTreeMap<String, String>,
    memo: &mut BTreeMap<String, String>,
    stack: &mut BTreeSet<String>,
) -> Result<String, VarError> {
    if let Some(value) = memo.get(name) {
        return Ok(value.clone());
    }
    if !stack.insert(name.to_owned()) {
        return Err(VarError::Cycle(name.to_owned()));
    }
    let source = raw
        .get(name)
        .ok_or_else(|| VarError::Missing(name.to_owned()))?;
    let value = resolve_text(source, &mut |dependency| {
        resolve_name(dependency, raw, memo, stack)
    });
    stack.remove(name);
    let value = value?;
    memo.insert(name.to_owned(), value.clone());
    Ok(value)
}

fn resolve_text(
    source: &str,
    lookup: &mut dyn FnMut(&str) -> Result<String, VarError>,
) -> Result<String, VarError> {
    let mut output = String::new();
    let mut cursor = 0;
    while let Some(start) = find_var(source, cursor) {
        output.push_str(&source[cursor..start]);
        let body_start = start + 4;
        let end = matching_paren(source, body_start)?;
        let body = &source[body_start..end];
        let (name, fallback) = split_var_arguments(body);
        let name = name.trim();
        if !valid_custom_name(name) {
            return Err(VarError::Syntax(format!(
                "invalid var() custom property name {name:?}"
            )));
        }
        let replacement = match lookup(name) {
            Ok(value) => value,
            Err(error) => match fallback {
                Some(fallback) => resolve_text(fallback.trim(), lookup)?,
                None => return Err(error),
            },
        };
        output.push_str(&replacement);
        cursor = end + 1;
    }
    output.push_str(&source[cursor..]);
    Ok(output.trim().to_owned())
}

fn find_var(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = from;
    while index + 4 <= bytes.len() {
        let is_var = bytes[index..index + 4].eq_ignore_ascii_case(b"var(");
        let boundary = index == 0
            || !bytes[index - 1].is_ascii_alphanumeric()
                && bytes[index - 1] != b'-'
                && bytes[index - 1] != b'_';
        if is_var && boundary {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn matching_paren(source: &str, body_start: usize) -> Result<usize, VarError> {
    let bytes = source.as_bytes();
    let mut depth = 1_u32;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate().skip(body_start) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth -= 1;
            if depth == 0 {
                return Ok(index);
            }
        }
    }
    Err(VarError::Syntax("unterminated var() function".to_owned()))
}

fn split_var_arguments(body: &str) -> (&str, Option<&str>) {
    let bytes = body.as_bytes();
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth = depth.saturating_sub(1);
        } else if byte == b',' && depth == 0 {
            return (&body[..index], Some(&body[index + 1..]));
        }
    }
    (body, None)
}

fn valid_custom_name(name: &str) -> bool {
    name.starts_with("--") && name.len() > 2 && !name.bytes().any(|byte| byte.is_ascii_whitespace())
}
