//! Streaming HTML decoding and an html5ever `TreeSink` backed by a generational arena.

mod dom;
mod mutation;
mod parser;
mod render_tree;
mod scripts;
mod selectors;
mod stylesheets;
mod traversal;
mod tree_sink;

pub use dom::{Document, DocumentQuirksMode, NodeHandle, NodeId};
pub use mutation::{DomMutation, DomMutationError, DomMutationKind};
pub use parser::{ParsedHtml, StreamingParser, parse_bytes, parse_utf8};
pub use render_tree::RenderChild;
pub use scripts::{ScriptCandidate, ScriptCandidateKind};
pub use stylesheets::{StylesheetCandidate, StylesheetCandidateKind};

#[cfg(test)]
mod tests;
