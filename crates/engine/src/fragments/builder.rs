use std::ops::Range;

use meow_css::PropertyId;
use meow_html::NodeId;

use crate::{
    BoxKind, BoxNode, BoxTree, ComputedStyle, ComputedStyleSnapshot, CssPx, FontDatabase,
    FontRequest, FontSlant, LayoutBox, LayoutRect, LayoutTree, LayoutViewport, TextAlign,
    layout_normal_flow, layout_normal_flow_with_inline_heights, layout_paragraph,
    paint::color_property,
};

use super::model::{
    FragmentId, FragmentLayout, FragmentTree, GlyphFragment, InlinePaintStyle, LineFragment,
    ParagraphFragment, TextDecorations,
};

#[derive(Clone, Debug)]
struct StyledSpan {
    range: Range<usize>,
    source: Option<NodeId>,
}

#[derive(Clone, Debug, Default)]
struct StyledText {
    text: String,
    spans: Vec<StyledSpan>,
}

/// Resolves block flow twice so measured paragraphs push following blocks.
#[must_use]
pub fn layout_fragment_tree(
    boxes: &BoxTree,
    styles: &ComputedStyleSnapshot,
    viewport: LayoutViewport,
    fonts: &mut FontDatabase,
) -> FragmentLayout {
    let provisional = layout_normal_flow(boxes, styles, viewport);
    let measured = build_fragment_tree(boxes, &provisional, styles, fonts);
    let final_layout =
        layout_normal_flow_with_inline_heights(boxes, styles, viewport, &measured.inline_heights());
    let fragments = build_fragment_tree(boxes, &final_layout, styles, fonts);
    FragmentLayout {
        layout: final_layout,
        fragments,
    }
}

/// Builds final inline fragments against an already positioned layout tree.
#[must_use]
pub fn build_fragment_tree(
    boxes: &BoxTree,
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    fonts: &mut FontDatabase,
) -> FragmentTree {
    let mut builder = Builder {
        styles,
        fonts,
        next_id: 0,
        paragraphs: Vec::new(),
    };
    for (box_root, layout_root) in boxes.roots().iter().zip(layout.roots()) {
        builder.visit(box_root, layout_root, box_root.source);
    }
    FragmentTree::new(builder.paragraphs)
}

struct Builder<'a> {
    styles: &'a ComputedStyleSnapshot,
    fonts: &'a mut FontDatabase,
    next_id: u32,
    paragraphs: Vec<ParagraphFragment>,
}

impl Builder<'_> {
    fn visit(&mut self, node: &BoxNode, layout: &LayoutBox, inherited_source: Option<NodeId>) {
        debug_assert_eq!(node.id, layout.box_id);
        let source = node.source.or(inherited_source);
        if is_paragraph_container(node) {
            if let Some(paragraph) = self.build_paragraph(node, layout, source) {
                self.paragraphs.push(paragraph);
            }
            return;
        }
        for (child, child_layout) in node.children.iter().zip(&layout.children) {
            self.visit(child, child_layout, source);
        }
    }

    fn build_paragraph(
        &mut self,
        node: &BoxNode,
        layout: &LayoutBox,
        source: Option<NodeId>,
    ) -> Option<ParagraphFragment> {
        let styled = collect_styled_text(node, source);
        if styled.text.is_empty() {
            return None;
        }
        let container_style = source.and_then(|source| self.styles.style_for(source));
        let request = font_request(container_style);
        let align = text_align(container_style);
        let paragraph = layout_paragraph(
            self.fonts,
            &request,
            &styled.text,
            layout.content.width.0,
            align,
        );
        let paragraph_height = paragraph.height();
        let paragraph_id = self.allocate_id();
        let mut search_cursor = 0;
        let mut lines = Vec::new();
        for line in paragraph.lines {
            let line_start = styled.text[search_cursor..]
                .find(&line.text)
                .map_or(search_cursor, |offset| search_cursor + offset);
            search_cursor = line_start + line.text.len();
            while styled.text.as_bytes().get(search_cursor) == Some(&b' ') {
                search_cursor += 1;
            }
            let line_id = self.allocate_id();
            let mut glyphs = Vec::new();
            for run in line.runs {
                for positioned in run.glyphs {
                    let global_cluster = line_start + positioned.glyph.cluster;
                    let glyph_source = span_source(&styled.spans, global_cluster).or(source);
                    glyphs.push(GlyphFragment {
                        id: self.allocate_id(),
                        box_id: node.id,
                        source: glyph_source,
                        font: run.font,
                        script: run.script,
                        direction: run.direction,
                        character: positioned.glyph.character,
                        cluster: global_cluster,
                        x: CssPx(layout.content.x.0 + positioned.x),
                        baseline: CssPx(layout.content.y.0 + positioned.baseline_y),
                        advance: CssPx(positioned.glyph.advance),
                        style: inline_style(
                            glyph_source.and_then(|source| self.styles.style_for(source)),
                        ),
                    });
                }
            }
            lines.push(LineFragment {
                id: line_id,
                rect: LayoutRect {
                    x: layout.content.x,
                    y: CssPx(layout.content.y.0 + line.y),
                    width: layout.content.width,
                    height: CssPx(16),
                },
                baseline: CssPx(layout.content.y.0 + line.baseline),
                used_width: CssPx(line.used_width),
                glyphs,
            });
        }
        Some(ParagraphFragment {
            id: paragraph_id,
            box_id: node.id,
            source,
            rect: LayoutRect {
                x: layout.content.x,
                y: layout.content.y,
                width: layout.content.width,
                height: CssPx(paragraph_height),
            },
            text: styled.text,
            lines,
        })
    }

    fn allocate_id(&mut self) -> FragmentId {
        let id = FragmentId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("fragment tree exceeded u32 identities");
        id
    }
}

fn is_paragraph_container(node: &BoxNode) -> bool {
    matches!(node.kind, BoxKind::PrincipalBlock | BoxKind::AnonymousBlock)
        && !node.children.is_empty()
        && node
            .children
            .iter()
            .all(|child| child.kind.is_inline_level())
}

fn collect_styled_text(node: &BoxNode, source: Option<NodeId>) -> StyledText {
    let mut pieces = Vec::<(Option<NodeId>, String)>::new();
    for child in &node.children {
        collect_pieces(child, source, &mut pieces);
    }
    collapse_pieces(pieces)
}

fn collect_pieces(
    node: &BoxNode,
    inherited_source: Option<NodeId>,
    output: &mut Vec<(Option<NodeId>, String)>,
) {
    let source = if node.kind == BoxKind::PrincipalInline {
        node.source.or(inherited_source)
    } else {
        inherited_source
    };
    if node.kind == BoxKind::TextRun {
        if let Some(text) = node.raw_text.as_ref().or(node.text.as_ref()) {
            output.push((source, text.clone()));
        }
        return;
    }
    for child in &node.children {
        collect_pieces(child, source, output);
    }
}

fn collapse_pieces(pieces: Vec<(Option<NodeId>, String)>) -> StyledText {
    let mut output = StyledText::default();
    let mut pending_space = false;
    let mut pending_source = None;
    for (source, text) in pieces {
        for character in text.chars() {
            if character.is_whitespace() {
                if !output.text.is_empty() {
                    pending_space = true;
                    pending_source = source;
                }
                continue;
            }
            if pending_space {
                push_styled_char(&mut output, ' ', pending_source.or(source));
                pending_space = false;
            }
            push_styled_char(&mut output, character, source);
        }
    }
    output
}

fn push_styled_char(output: &mut StyledText, character: char, source: Option<NodeId>) {
    let start = output.text.len();
    output.text.push(character);
    let end = output.text.len();
    if let Some(last) = output.spans.last_mut()
        && last.source == source
        && last.range.end == start
    {
        last.range.end = end;
        return;
    }
    output.spans.push(StyledSpan {
        range: start..end,
        source,
    });
}

fn span_source(spans: &[StyledSpan], byte_offset: usize) -> Option<NodeId> {
    spans
        .iter()
        .find(|span| span.range.contains(&byte_offset))
        .and_then(|span| span.source)
}

fn font_request(style: Option<&ComputedStyle>) -> FontRequest {
    let Some(style) = style else {
        return FontRequest::default();
    };
    FontRequest {
        families: style
            .get(PropertyId::FontFamily)
            .split(',')
            .map(|family| family.trim().trim_matches(['\'', '"']).to_owned())
            .filter(|family| !family.is_empty())
            .collect(),
        weight: font_weight(style.get(PropertyId::FontWeight)),
        slant: font_slant(style.get(PropertyId::FontStyle)),
        locale: None,
    }
}

fn inline_style(style: Option<&ComputedStyle>) -> InlinePaintStyle {
    let color = style
        .map(|style| color_property(style.typed(PropertyId::Color)))
        .unwrap_or_else(|| meow_display_list::Rgba8::rgb(0, 0, 0));
    let weight = style.map_or(400, |style| font_weight(style.get(PropertyId::FontWeight)));
    let slant = style.map_or(FontSlant::Normal, |style| {
        font_slant(style.get(PropertyId::FontStyle))
    });
    let decoration = style.map_or("none", |style| style.get(PropertyId::TextDecorationLine));
    InlinePaintStyle {
        color,
        weight,
        slant,
        decorations: TextDecorations {
            underline: decoration
                .split_whitespace()
                .any(|value| value == "underline"),
            line_through: decoration
                .split_whitespace()
                .any(|value| value == "line-through"),
        },
    }
}

fn font_weight(value: &str) -> u16 {
    match value {
        "normal" => 400,
        "bold" | "bolder" => 700,
        "lighter" => 300,
        value => value.parse::<u16>().unwrap_or(400).clamp(1, 1_000),
    }
}

fn font_slant(value: &str) -> FontSlant {
    if matches!(value, "italic" | "oblique") {
        FontSlant::Italic
    } else {
        FontSlant::Normal
    }
}

fn text_align(style: Option<&ComputedStyle>) -> TextAlign {
    match style.map(|style| style.get(PropertyId::TextAlign)) {
        Some("end") => TextAlign::End,
        Some("left") => TextAlign::Left,
        Some("right") => TextAlign::Right,
        Some("center") => TextAlign::Center,
        Some("justify") => TextAlign::Justify,
        _ => TextAlign::Start,
    }
}
