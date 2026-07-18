mod builder;
mod model;

pub use builder::build_box_tree;
pub use model::{BoxId, BoxKind, BoxNode, BoxTree};

#[cfg(test)]
mod tests;
