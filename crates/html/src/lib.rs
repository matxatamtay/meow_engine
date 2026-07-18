//! Streaming HTML decoding and an html5ever `TreeSink` backed by a generational arena.

mod dom;
mod parser;
mod selectors;
mod stylesheets;
mod traversal;
mod tree_sink;

pub use dom::{Document, DocumentQuirksMode, NodeHandle, NodeId};
pub use parser::{ParsedHtml, StreamingParser, parse_bytes, parse_utf8};
pub use stylesheets::{StylesheetCandidate, StylesheetCandidateKind};

#[cfg(test)]
mod tests;
