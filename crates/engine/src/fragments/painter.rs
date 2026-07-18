use meow_display_list::{DisplayList, DisplayListError, Rectangle, Rgba8, Viewport};

use crate::{
    ComputedStyleSnapshot, CssPx, FontSlant, FragmentTree, LayoutRect, LayoutTree,
    is_combining_mark,
    paint::{fill_signed, paint_box},
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
    let mut list = DisplayList::new();
    list.clear(CANVAS);
    list.push_clip(Rectangle::new(0, 0, viewport.width, viewport.height))?;
    for root in layout.roots() {
        paint_box(root, styles, viewport, &mut list)?;
    }
    for paragraph in fragments.paragraphs() {
        for line in &paragraph.lines {
            for glyph in &line.glyphs {
                paint_glyph(&mut list, glyph, viewport)?;
            }
        }
    }
    list.pop_clip()?;
    Ok(list)
}

fn paint_glyph(
    list: &mut DisplayList,
    glyph: &GlyphFragment,
    viewport: Viewport,
) -> Result<(), DisplayListError> {
    if glyph.character.is_whitespace() {
        return Ok(());
    }
    if is_combining_mark(glyph.character) {
        fill_signed(
            list,
            LayoutRect {
                x: CssPx(glyph.x.0 + 2),
                y: CssPx(glyph.baseline.0 - 10),
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
    let top = glyph.baseline.0 - 7;
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
            let x = glyph.x.0 + column + skew;
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
                x: glyph.x,
                y: CssPx(glyph.baseline.0 + 1),
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
                x: glyph.x,
                y: CssPx(glyph.baseline.0 - 4),
                width: CssPx(decoration_width),
                height: CssPx(1),
            },
            glyph.style.color,
            viewport,
        )?;
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
