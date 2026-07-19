mod builder;
mod model;

pub use builder::{build_box_tree, build_box_tree_with_images};
pub use model::{BoxId, BoxKind, BoxNode, BoxTree};

#[cfg(test)]
mod tests;
