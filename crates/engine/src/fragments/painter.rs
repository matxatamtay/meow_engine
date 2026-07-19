use std::{collections::BTreeMap, sync::Arc};

use meow_display_list::{DisplayList, DisplayListError, ImageId, Rectangle, Rgba8, Viewport};
use meow_html::NodeId;

use crate::{
    BoxKind, ComputedStyleSnapshot, CssPx, FontSlant, FragmentTree, ImageResource, LayoutRect,
    LayoutTree, is_combining_mark,
    paint::{begin_stacking_context, fill_signed, paint_box_self_at},
};

use super::{GlyphFragment, pixel_font};

const CANVAS: Rgba8 = Rgba8::rgb(255, 255, 255);

/// Paints W16 boxes followed by W20 text fragments and decorations.
pub fn build_fragment_display_list(
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    fragments: &FragmentTree,
    viewport: Viewport,
) -> Result<DisplayList, DisplayListError> {
    build_fragment_display_list_with_images(layout, styles, fragments, viewport, &BTreeMap::new())
}

/// Paints a fragment layout translated by a document-space offset.
pub fn build_fragment_display_list_with_offset(
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    fragments: &FragmentTree,
    viewport: Viewport,
    offset_x: i32,
    offset_y: i32,
) -> Result<DisplayList, DisplayListError> {
    build_fragment_display_list_with_images_and_offset(
        layout,
        styles,
        fragments,
        viewport,
        &BTreeMap::new(),
        offset_x,
        offset_y,
    )
}

pub fn build_fragment_display_list_with_images(
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    fragments: &FragmentTree,
    viewport: Viewport,
    images: &BTreeMap<NodeId, Arc<ImageResource>>,
) -> Result<DisplayList, DisplayListError> {
    build_fragment_display_list_with_images_and_offset(
        layout, styles, fragments, viewport, images, 0, 0,
    )
}

pub fn build_fragment_display_list_with_images_and_offset(
    layout: &LayoutTree,
    styles: &ComputedStyleSnapshot,
    fragments: &FragmentTree,
    viewport: Viewport,
    images: &BTreeMap<NodeId, Arc<ImageResource>>,
    offset_x: i32,
    offset_y: i32,
) -> Result<DisplayList, DisplayListError> {
    let mut list = DisplayList::new();
    list.clear(CANVAS);
    list.push_clip(Rectangle::new(0, 0, viewport.width, viewport.height))?;
    let mut paragraphs = BTreeMap::new();
    for paragraph in fragments.paragraphs() {
        paragraphs
            .entry(paragraph.box_id)
            .or_insert_with(Vec::new)
            .push(paragraph);
    }
    let mut context = FragmentPaintContext {
        styles,
        paragraphs: &paragraphs,
        images,
        image_ids: BTreeMap::new(),
        viewport,
        offset_x,
        offset_y,
    };
    for root in layout.roots() {
        context.paint_node(root, &mut list)?;
    }
    list.pop_clip()?;
    Ok(list)
}

struct FragmentPaintContext<'a, 'p> {
    styles: &'a ComputedStyleSnapshot,
    paragraphs: &'a BTreeMap<crate::BoxId, Vec<&'p super::ParagraphFragment>>,
    images: &'a BTreeMap<NodeId, Arc<ImageResource>>,
    image_ids: BTreeMap<NodeId, ImageId>,
    viewport: Viewport,
    offset_x: i32,
    offset_y: i32,
}

impl FragmentPaintContext<'_, '_> {
    fn paint_node(
        &mut self,
        node: &crate::LayoutBox,
        list: &mut DisplayList,
    ) -> Result<(), DisplayListError> {
        let pushed = begin_stacking_context(
            node,
            self.styles,
            list,
            CssPx(self.offset_x),
            CssPx(self.offset_y),
        );
        paint_box_self_at(
            node,
            self.styles,
            self.viewport,
            list,
            CssPx(self.offset_x),
            CssPx(self.offset_y),
        )?;
        if node.kind == BoxKind::ReplacedImage
            && let Some(source) = node.source
            && let Some(image) = self.images.get(&source)
            && let Some(rectangle) =
                image_rectangle(node, self.viewport, self.offset_x, self.offset_y)
        {
            let image_id = if let Some(image_id) = self.image_ids.get(&source).copied() {
                image_id
            } else {
                let image_id =
                    list.add_image(image.width, image.height, image.pixels.as_ref().to_vec())?;
                self.image_ids.insert(source, image_id);
                image_id
            };
            list.draw_image(image_id, rectangle)?;
        }
        if let Some(paragraphs) = self.paragraphs.get(&node.box_id) {
            for paragraph in paragraphs {
                for line in &paragraph.lines {
                    for glyph in &line.glyphs {
                        paint_glyph(list, glyph, self.viewport, self.offset_x, self.offset_y)?;
                    }
                }
            }
        }
        for child in &node.children {
            self.paint_node(child, list)?;
        }
        if pushed {
            list.pop_layer()?;
        }
        Ok(())
    }
}

fn image_rectangle(
    node: &crate::LayoutBox,
    viewport: Viewport,
    offset_x: i32,
    offset_y: i32,
) -> Option<Rectangle> {
    let x0 = node.content.x.0.saturating_add(offset_x).max(0);
    let y0 = node.content.y.0.saturating_add(offset_y).max(0);
    let x1 = node
        .content
        .x
        .0
        .saturating_add(offset_x)
        .saturating_add(node.content.width.0)
        .max(0)
        .min(viewport.width as i32);
    let y1 = node
        .content
        .y
        .0
        .saturating_add(offset_y)
        .saturating_add(node.content.height.0)
        .max(0)
        .min(viewport.height as i32);
    (x1 > x0 && y1 > y0)
        .then(|| Rectangle::new(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

fn paint_glyph(
    list: &mut DisplayList,
    glyph: &GlyphFragment,
    viewport: Viewport,
    offset_x: i32,
    offset_y: i32,
) -> Result<(), DisplayListError> {
    if glyph.character.is_whitespace() {
        return Ok(());
    }
    if is_combining_mark(glyph.character) {
        fill_signed(
            list,
            LayoutRect {
                x: CssPx(glyph.x.0 + 2 + offset_x),
                y: CssPx(glyph.baseline.0 - 10 + offset_y),
                width: CssPx(2),
                height: CssPx(1),
            },
            glyph.style.color,
            viewport,
        )?;
        return Ok(());
    }
    let Some(rows) = pixel_font::bitmap(glyph.character) else {
        return Ok(());
    };
    let top = glyph.baseline.0 - 7 + offset_y;
    for (row_index, row) in rows.into_iter().enumerate() {
        let skew = if glyph.style.slant == FontSlant::Italic {
            (6 - row_index as i32) / 3
        } else {
            0
        };
        for column in 0..5 {
            if row & (1 << (4 - column)) == 0 {
                continue;
            }
            let x = glyph.x.0 + offset_x + column + skew;
            let y = top + row_index as i32;
            paint_pixel(list, x, y, glyph.style.color, viewport)?;
            if glyph.style.weight >= 600 {
                paint_pixel(list, x + 1, y, glyph.style.color, viewport)?;
            }
        }
    }
    let decoration_width = glyph.advance.0.max(1);
    if glyph.style.decorations.underline {
        fill_signed(
            list,
            LayoutRect {
                x: CssPx(glyph.x.0 + offset_x),
                y: CssPx(glyph.baseline.0 + 1 + offset_y),
                width: CssPx(decoration_width),
                height: CssPx(1),
            },
            glyph.style.color,
            viewport,
        )?;
    }
    if glyph.style.decorations.line_through {
        fill_signed(
            list,
            LayoutRect {
                x: CssPx(glyph.x.0 + offset_x),
                y: CssPx(glyph.baseline.0 - 4 + offset_y),
                width: CssPx(decoration_width),
                height: CssPx(1),
            },
            glyph.style.color,
            viewport,
        )?;
    }
    Ok(())
}

pub(crate) fn append_bitmap_text(
    list: &mut DisplayList,
    text: &str,
    x: i32,
    baseline: i32,
    color: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    let mut cursor = x;
    for character in text.chars() {
        if let Some(rows) = pixel_font::bitmap(character) {
            for (row_index, row) in rows.into_iter().enumerate() {
                for column in 0..5 {
                    if row & (1 << (4 - column)) != 0 {
                        paint_pixel(
                            list,
                            cursor + column,
                            baseline - 7 + row_index as i32,
                            color,
                            viewport,
                        )?;
                    }
                }
            }
        }
        cursor += 6;
    }
    Ok(())
}

fn paint_pixel(
    list: &mut DisplayList,
    x: i32,
    y: i32,
    color: Rgba8,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    fill_signed(
        list,
        LayoutRect {
            x: CssPx(x),
            y: CssPx(y),
            width: CssPx(1),
            height: CssPx(1),
        },
        color,
        viewport,
    )
}
