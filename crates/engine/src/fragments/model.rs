use std::{collections::BTreeMap, fmt::Write as _};

use meow_display_list::Rgba8;
use meow_html::NodeId;

use crate::{BoxId, CssPx, FontId, FontSlant, LayoutRect, LayoutTree, Script, TextDirection};

/// Stable identity inside one fragment tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FragmentId(pub u32);

/// Text-decoration lines supported by W20.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextDecorations {
    pub underline: bool,
    pub line_through: bool,
}

/// Paint-relevant computed inline style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlinePaintStyle {
    pub color: Rgba8,
    pub weight: u16,
    pub slant: FontSlant,
    pub decorations: TextDecorations,
}

/// One glyph fragment with final page-space baseline coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphFragment {
    pub id: FragmentId,
    pub box_id: BoxId,
    pub source: Option<NodeId>,
    pub font: FontId,
    pub script: Script,
    pub direction: TextDirection,
    pub character: char,
    pub cluster: usize,
    pub x: CssPx,
    pub baseline: CssPx,
    pub advance: CssPx,
    pub style: InlinePaintStyle,
}

/// One final line fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineFragment {
    pub id: FragmentId,
    pub rect: LayoutRect,
    pub baseline: CssPx,
    pub used_width: CssPx,
    pub glyphs: Vec<GlyphFragment>,
}

/// One paragraph formatting fragment associated with a box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphFragment {
    pub id: FragmentId,
    pub box_id: BoxId,
    pub source: Option<NodeId>,
    pub rect: LayoutRect,
    pub text: String,
    pub lines: Vec<LineFragment>,
}

/// DOM-independent final inline fragment tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FragmentTree {
    paragraphs: Vec<ParagraphFragment>,
}

impl FragmentTree {
    pub(super) fn new(paragraphs: Vec<ParagraphFragment>) -> Self {
        Self { paragraphs }
    }

    #[must_use]
    pub fn paragraphs(&self) -> &[ParagraphFragment] {
        &self.paragraphs
    }

    #[must_use]
    pub fn inline_heights(&self) -> BTreeMap<BoxId, CssPx> {
        self.paragraphs
            .iter()
            .map(|paragraph| (paragraph.box_id, paragraph.rect.height))
            .collect()
    }

    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::from("#fragment-tree\n");
        for paragraph in &self.paragraphs {
            writeln!(
                output,
                "paragraph fragment={} box={} source={} rect=({},{} {}x{}) text={:?}",
                paragraph.id.0,
                paragraph.box_id.0,
                source_slot(paragraph.source),
                paragraph.rect.x.0,
                paragraph.rect.y.0,
                paragraph.rect.width.0,
                paragraph.rect.height.0,
                paragraph.text,
            )
            .expect("writing to String cannot fail");
            for line in &paragraph.lines {
                writeln!(
                    output,
                    "  line fragment={} rect=({},{} {}x{}) baseline={} used={}",
                    line.id.0,
                    line.rect.x.0,
                    line.rect.y.0,
                    line.rect.width.0,
                    line.rect.height.0,
                    line.baseline.0,
                    line.used_width.0,
                )
                .expect("writing to String cannot fail");
                for glyph in &line.glyphs {
                    writeln!(
                        output,
                        "    glyph fragment={} box={} source={} font={} script={:?} direction={:?} char={:?} cluster={} x={} baseline={} advance={} color=#{:02x}{:02x}{:02x}{:02x} weight={} slant={:?} decoration=({},{})",
                        glyph.id.0,
                        glyph.box_id.0,
                        source_slot(glyph.source),
                        glyph.font.0,
                        glyph.script,
                        glyph.direction,
                        glyph.character,
                        glyph.cluster,
                        glyph.x.0,
                        glyph.baseline.0,
                        glyph.advance.0,
                        glyph.style.color.red(),
                        glyph.style.color.green(),
                        glyph.style.color.blue(),
                        glyph.style.color.alpha(),
                        glyph.style.weight,
                        glyph.style.slant,
                        glyph.style.decorations.underline,
                        glyph.style.decorations.line_through,
                    )
                    .expect("writing to String cannot fail");
                }
            }
        }
        output
    }
}

/// Final block layout paired with its independent inline fragment tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FragmentLayout {
    pub layout: LayoutTree,
    pub fragments: FragmentTree,
}

fn source_slot(source: Option<NodeId>) -> String {
    source
        .map(|source| source.slot.to_string())
        .unwrap_or_else(|| "-".to_owned())
}
