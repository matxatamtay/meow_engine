mod line_breaker;
mod line_model;
mod model;
mod shaper;

pub use line_breaker::{collapse_whitespace, layout_paragraph};
pub use line_model::{LineBox, LineRun, ParagraphLayout, PositionedGlyph, TextAlign};
pub use model::{ShapedGlyph, ShapedRun, ShapedText, TextDirection};
pub use shaper::{is_combining_mark, shape_text};

#[cfg(test)]
mod tests;
