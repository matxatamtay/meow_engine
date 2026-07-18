mod builder;
mod model;
mod painter;
mod pixel_font;

pub use builder::{build_fragment_tree, layout_fragment_tree};
pub use model::{
    FragmentId, FragmentLayout, FragmentTree, GlyphFragment, InlinePaintStyle, LineFragment,
    ParagraphFragment, TextDecorations,
};
pub use painter::build_fragment_display_list;

#[cfg(test)]
mod tests;
