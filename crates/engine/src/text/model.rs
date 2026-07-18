use std::{fmt::Write as _, ops::Range};

use crate::{FontId, Script};

/// Resolved inline text direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

/// Deterministic glyph metrics produced by the W18 shaper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub cluster: usize,
    pub character: char,
    pub advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

/// One script, direction, and font-homogeneous shaped run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapedRun {
    pub logical_range: Range<usize>,
    pub visual_index: usize,
    pub font: FontId,
    pub family: String,
    pub script: Script,
    pub direction: TextDirection,
    pub text: String,
    pub glyphs: Vec<ShapedGlyph>,
    pub advance: i32,
    pub ascent: i32,
    pub descent: i32,
    pub line_gap: i32,
}

/// Paragraph-level shaping output in visual run order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapedText {
    pub paragraph_direction: TextDirection,
    pub runs: Vec<ShapedRun>,
}

impl ShapedText {
    #[must_use]
    pub fn advance(&self) -> i32 {
        self.runs.iter().map(|run| run.advance).sum()
    }

    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::new();
        writeln!(
            output,
            "#shaped-text direction={:?} advance={}",
            self.paragraph_direction,
            self.advance()
        )
        .expect("writing to String cannot fail");
        for run in &self.runs {
            writeln!(
                output,
                "run visual={} logical={}..{} font={} family={:?} script={:?} direction={:?} text={:?} advance={} metrics=({},{},{})",
                run.visual_index,
                run.logical_range.start,
                run.logical_range.end,
                run.font.0,
                run.family,
                run.script,
                run.direction,
                run.text,
                run.advance,
                run.ascent,
                run.descent,
                run.line_gap,
            )
            .expect("writing to String cannot fail");
            for glyph in &run.glyphs {
                writeln!(
                    output,
                    "  glyph id={} cluster={} char={:?} advance={} offset=({}, {})",
                    glyph.glyph_id,
                    glyph.cluster,
                    glyph.character,
                    glyph.advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
                .expect("writing to String cannot fail");
            }
        }
        output
    }
}
