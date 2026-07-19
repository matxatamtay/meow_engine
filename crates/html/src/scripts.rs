//! DOM discovery for classic script elements.

use super::dom::{Document, DomState, NodeHandle, NodeId, NodeKind, attribute_value, node};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCandidate {
    pub node: NodeId,
    pub kind: ScriptCandidateKind,
    pub defer: bool,
    pub async_attribute: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptCandidateKind {
    Inline(String),
    External(String),
}

impl Document {
    #[must_use]
    pub fn script_candidates(&self) -> Vec<ScriptCandidate> {
        let state = self.inner.state.borrow();
        let mut output = Vec::new();
        collect_script_candidates(&state, &self.root, &mut output);
        output
    }
}

fn collect_script_candidates(
    state: &DomState,
    handle: &NodeHandle,
    output: &mut Vec<ScriptCandidate>,
) {
    let current = node(state, handle);
    if let NodeKind::Element { name, attrs, .. } = &current.kind
        && name.ns.as_ref() == "http://www.w3.org/1999/xhtml"
        && name.local.as_ref() == "script"
        && classic_type_is_supported(attribute_value(attrs, "type").as_deref())
    {
        let kind = attribute_value(attrs, "src").map_or_else(
            || {
                let mut source = String::new();
                collect_text_content(state, handle, &mut source);
                ScriptCandidateKind::Inline(source)
            },
            ScriptCandidateKind::External,
        );
        output.push(ScriptCandidate {
            node: handle.id,
            kind,
            defer: attribute_value(attrs, "defer").is_some(),
            async_attribute: attribute_value(attrs, "async").is_some(),
        });
    }
    for child in &current.children {
        collect_script_candidates(state, child, output);
    }
}

fn classic_type_is_supported(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "text/javascript"
                | "application/javascript"
                | "text/ecmascript"
                | "application/ecmascript"
        )
    })
}

fn collect_text_content(state: &DomState, handle: &NodeHandle, output: &mut String) {
    let current = node(state, handle);
    if let NodeKind::Text(text) = &current.kind {
        output.push_str(text);
    }
    for child in &current.children {
        collect_text_content(state, child, output);
    }
}

#[cfg(test)]
mod tests {
    use crate::{ScriptCandidateKind, parse_utf8};

    #[test]
    fn discovers_classic_scripts_in_tree_order() {
        let document = parse_utf8(
            br#"<script>one()</script><script defer src='/two.js'></script><script type='module'>skip()</script>"#,
        )
        .document;
        let scripts = document.script_candidates();
        assert_eq!(scripts.len(), 2);
        assert!(matches!(scripts[0].kind, ScriptCandidateKind::Inline(_)));
        assert!(scripts[1].defer);
        assert!(matches!(scripts[1].kind, ScriptCandidateKind::External(_)));
    }
}
