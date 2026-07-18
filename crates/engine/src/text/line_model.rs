use std::fmt::Write as _;

use crate::{FontId, Script};

use super::{ShapedGlyph, TextDirection};

/// Supported W19 inline alignment values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

/// One visually positioned glyph inside a line run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionedGlyph {
    pub glyph: ShapedGlyph,
    pub x: i32,
    pub baseline_y: i32,
}

/// A shaped run positioned within one line box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRun {
    pub font: FontId,
    pub family: String,
    pub script: Script,
    pub direction: TextDirection,
    pub x: i32,
    pub width: i32,
    pub glyphs: Vec<PositionedGlyph>,
}

/// One deterministic line box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineBox {
    pub index: usize,
    pub y: i32,
    pub baseline: i32,
    pub available_width: i32,
    pub used_width: i32,
    pub offset: i32,
    pub text: String,
    pub runs: Vec<LineRun>,
}

/// Paragraph output after whitespace collapse, wrapping, bidi, and alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphLayout {
    pub width: i32,
    pub direction: TextDirection,
    pub align: TextAlign,
    pub collapsed_text: String,
    pub lines: Vec<LineBox>,
}

impl ParagraphLayout {
    #[must_use]
    pub fn height(&self) -> i32 {
        self.lines.len() as i32 * 16
    }

    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::new();
        writeln!(
            output,
            "#paragraph width={} height={} direction={:?} align={:?} text={:?}",
            self.width,
            self.height(),
            self.direction,
            self.align,
            self.collapsed_text,
        )
        .expect("writing to String cannot fail");
        for line in &self.lines {
            writeln!(
                output,
                "line={} y={} baseline={} available={} used={} offset={} text={:?}",
                line.index,
                line.y,
                line.baseline,
                line.available_width,
                line.used_width,
                line.offset,
                line.text,
            )
            .expect("writing to String cannot fail");
            for run in &line.runs {
                writeln!(
                    output,
                    "  run font={} family={:?} script={:?} direction={:?} x={} width={}",
                    run.font.0, run.family, run.script, run.direction, run.x, run.width,
                )
                .expect("writing to String cannot fail");
                for glyph in &run.glyphs {
                    writeln!(
                        output,
                        "    glyph char={:?} cluster={} x={} baseline={} advance={}",
                        glyph.glyph.character,
                        glyph.glyph.cluster,
                        glyph.x,
                        glyph.baseline_y,
                        glyph.glyph.advance,
                    )
                    .expect("writing to String cannot fail");
                }
            }
        }
        output
    }
}
