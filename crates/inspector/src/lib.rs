//! Built-in inspector snapshot and diagnostics bundle models.

use std::{fs, io, path::Path};

use serde::{Deserialize, Serialize};

/// One request in the built-in network waterfall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkWaterfallEntry {
    pub sequence: u64,
    pub method: String,
    pub requested_url: String,
    pub final_url: Option<String>,
    pub status: Option<u16>,
    pub transferred_bytes: usize,
    pub elapsed_ms: u64,
    pub backend: String,
    pub error: Option<String>,
}

/// One page-console entry retained for inspection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectorConsoleEntry {
    pub level: String,
    pub message: String,
}

/// Complete W47 snapshot usable without external developer tools.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectorSnapshot {
    pub schema_version: u32,
    pub engine_version: String,
    pub url: String,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub dom_tree: String,
    pub computed_style: String,
    pub box_model: String,
    pub layout_tree: String,
    pub accessibility_tree: serde_json::Value,
    pub network_waterfall: Vec<NetworkWaterfallEntry>,
    pub console: Vec<InspectorConsoleEntry>,
    pub stylesheet_errors: Vec<String>,
    pub image_errors: Vec<String>,
}

impl InspectorSnapshot {
    pub fn write_json(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        bytes.push(b'\n');
        fs::write(path, bytes)
    }

    #[must_use]
    pub fn has_required_panels(&self) -> bool {
        !self.dom_tree.is_empty()
            && !self.computed_style.is_empty()
            && !self.box_model.is_empty()
            && !self.layout_tree.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_panels_reject_empty_snapshot() {
        let snapshot = InspectorSnapshot {
            schema_version: 1,
            engine_version: "test".to_owned(),
            url: "about:blank".to_owned(),
            viewport_width: 1,
            viewport_height: 1,
            dom_tree: String::new(),
            computed_style: String::new(),
            box_model: String::new(),
            layout_tree: String::new(),
            accessibility_tree: serde_json::Value::Null,
            network_waterfall: Vec::new(),
            console: Vec::new(),
            stylesheet_errors: Vec::new(),
            image_errors: Vec::new(),
        };
        assert!(!snapshot.has_required_panels());
    }
}
