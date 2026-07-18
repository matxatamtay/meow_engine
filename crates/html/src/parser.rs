//! Streaming byte decoding and html5ever parser orchestration.

use encoding_rs::{CoderResult, Encoding, UTF_8};
use html5ever::{parse_document, tendril::TendrilSink};

use super::{dom::Document, tree_sink::DomSink};

/// Incremental decoder feeding UTF-8 tendrils into html5ever.
pub struct StreamingParser {
    parser: html5ever::driver::Parser<DomSink>,
    decoder: encoding_rs::Decoder,
    had_replacements: bool,
}

impl StreamingParser {
    /// Creates a parser for a chosen Encoding Standard decoder.
    #[must_use]
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            parser: parse_document(DomSink::new(), Default::default()),
            decoder: encoding.new_decoder(),
            had_replacements: false,
        }
    }

    /// Feeds another network byte chunk.
    pub fn feed(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let capacity = self
                .decoder
                .max_utf8_buffer_length(bytes.len())
                .unwrap_or(bytes.len().saturating_mul(3).saturating_add(8));
            let mut decoded = String::with_capacity(capacity.max(8));
            let (result, read, replacements) =
                self.decoder.decode_to_string(bytes, &mut decoded, false);
            self.had_replacements |= replacements;
            if !decoded.is_empty() {
                self.parser.process(decoded.into());
            }
            bytes = &bytes[read..];
            if result == CoderResult::InputEmpty {
                break;
            }
            assert!(
                read > 0,
                "decoder made no progress with a full output buffer"
            );
        }
    }

    /// Completes decoding and tree construction.
    #[must_use]
    pub fn finish(mut self) -> ParsedHtml {
        let capacity = self.decoder.max_utf8_buffer_length(0).unwrap_or(8);
        let mut decoded = String::with_capacity(capacity.max(8));
        let (_, _, replacements) = self.decoder.decode_to_string(b"", &mut decoded, true);
        self.had_replacements |= replacements;
        if !decoded.is_empty() {
            self.parser.process(decoded.into());
        }
        ParsedHtml {
            document: self.parser.finish(),
            had_replacements: self.had_replacements,
        }
    }
}

/// Result of HTML byte decoding and tree construction.
#[derive(Clone, Debug)]
pub struct ParsedHtml {
    /// Parsed document.
    pub document: Document,
    /// Whether malformed byte sequences were replaced.
    pub had_replacements: bool,
}

/// Parses a complete byte slice while preserving the streaming implementation path.
#[must_use]
pub fn parse_bytes(bytes: &[u8], encoding: &'static Encoding) -> ParsedHtml {
    let mut parser = StreamingParser::new(encoding);
    parser.feed(bytes);
    parser.finish()
}

/// Parses UTF-8 HTML bytes.
#[must_use]
pub fn parse_utf8(bytes: &[u8]) -> ParsedHtml {
    parse_bytes(bytes, UTF_8)
}
