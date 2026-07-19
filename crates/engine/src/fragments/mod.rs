mod builder;
mod model;
mod painter;
mod pixel_font;

pub use builder::{build_fragment_tree, layout_fragment_tree};
pub use model::{
    FragmentId, FragmentLayout, FragmentTree, GlyphFragment, InlinePaintStyle, LineFragment,
    ParagraphFragment, TextDecorations,
};
pub(crate) use painter::append_bitmap_text;
pub use painter::{
    build_fragment_display_list, build_fragment_display_list_with_images,
    build_fragment_display_list_with_images_and_offset, build_fragment_display_list_with_offset,
};
pub use pixel_font::{GlyphCacheMetrics, glyph_cache_metrics};

#[cfg(test)]
mod tests;
