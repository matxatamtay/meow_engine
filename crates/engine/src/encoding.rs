//! HTML and stylesheet encoding detection helpers.

use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};

use super::model::CharsetSource;

pub(super) fn sniff_encoding(
    bytes: &[u8],
    content_type: Option<&str>,
) -> (&'static Encoding, CharsetSource) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return (UTF_8, CharsetSource::Bom);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return (
            Encoding::for_label(b"utf-16le").expect("encoding_rs provides UTF-16LE"),
            CharsetSource::Bom,
        );
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return (
            Encoding::for_label(b"utf-16be").expect("encoding_rs provides UTF-16BE"),
            CharsetSource::Bom,
        );
    }
    if let Some(label) = content_type.and_then(charset_parameter)
        && let Some(encoding) = Encoding::for_label(label.as_bytes())
    {
        return (encoding, CharsetSource::HttpHeader);
    }
    if let Some(label) = sniff_meta_charset(bytes)
        && let Some(encoding) = Encoding::for_label(label.as_bytes())
    {
        return (encoding, CharsetSource::Meta);
    }
    (WINDOWS_1252, CharsetSource::Default)
}

pub(super) fn charset_parameter(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("charset") {
            return None;
        }
        let value = value.trim().trim_matches(['\'', '"']);
        (!value.is_empty()).then(|| value.to_owned())
    })
}

pub(super) fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let sample = bytes
        .iter()
        .take(1024)
        .map(|byte| {
            if byte.is_ascii() {
                byte.to_ascii_lowercase() as char
            } else {
                ' '
            }
        })
        .collect::<String>();
    let mut search_from = 0;
    while let Some(offset) = sample[search_from..].find("charset") {
        let mut cursor = search_from + offset + "charset".len();
        let tail = sample.as_bytes();
        while tail.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if tail.get(cursor) != Some(&b'=') {
            search_from = cursor;
            continue;
        }
        cursor += 1;
        while tail.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let quote = tail
            .get(cursor)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"'));
        if quote.is_some() {
            cursor += 1;
        }
        let start = cursor;
        while let Some(byte) = tail.get(cursor) {
            let terminates = quote.map_or_else(
                || byte.is_ascii_whitespace() || matches!(byte, b';' | b'/' | b'>'),
                |quote| *byte == quote,
            );
            if terminates {
                break;
            }
            cursor += 1;
        }
        if cursor > start {
            return Some(sample[start..cursor].to_owned());
        }
        search_from = cursor.saturating_add(1);
    }
    None
}
