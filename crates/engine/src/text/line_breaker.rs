use crate::{FontDatabase, FontRequest};

use super::{
    TextDirection, is_combining_mark,
    line_model::{LineBox, LineRun, ParagraphLayout, PositionedGlyph, TextAlign},
    shape_text,
};

const LINE_HEIGHT: i32 = 16;
const ASCENT: i32 = 12;

/// Collapses CSS `white-space: normal` input to one inter-word ASCII space.
#[must_use]
pub fn collapse_whitespace(text: &str) -> String {
    let mut output = String::new();
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
                pending_space = false;
            }
            output.push(character);
        }
    }
    output
}

/// Greedily lays out a paragraph into deterministic line boxes.
#[must_use]
pub fn layout_paragraph(
    database: &mut FontDatabase,
    request: &FontRequest,
    text: &str,
    width: i32,
    align: TextAlign,
) -> ParagraphLayout {
    let width = width.max(1);
    let collapsed_text = collapse_whitespace(text);
    let paragraph_direction = shape_text(database, request, &collapsed_text).paragraph_direction;
    let logical_lines = wrap_text(database, request, &collapsed_text, width);
    let line_count = logical_lines.len();
    let lines = logical_lines
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            position_line(
                database,
                request,
                text,
                width,
                align,
                index,
                index + 1 == line_count,
            )
        })
        .collect();
    ParagraphLayout {
        width,
        direction: paragraph_direction,
        align,
        collapsed_text,
        lines,
    }
}

fn wrap_text(
    database: &mut FontDatabase,
    request: &FontRequest,
    text: &str,
    width: i32,
) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if measure(database, request, &candidate) <= width {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if measure(database, request, word) <= width {
            current.push_str(word);
        } else {
            let mut chunks = hard_wrap_word(database, request, word, width);
            if let Some(last) = chunks.pop() {
                lines.extend(chunks);
                current = last;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn hard_wrap_word(
    database: &mut FontDatabase,
    request: &FontRequest,
    word: &str,
    width: i32,
) -> Vec<String> {
    let clusters = text_clusters(word);
    let mut chunks = Vec::new();
    let mut current = String::new();
    for cluster in clusters {
        let candidate = format!("{current}{cluster}");
        if !current.is_empty() && measure(database, request, &candidate) > width {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(&cluster);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn text_clusters(text: &str) -> Vec<String> {
    let mut clusters = Vec::<String>::new();
    for character in text.chars() {
        if is_combining_mark(character)
            && let Some(cluster) = clusters.last_mut()
        {
            cluster.push(character);
        } else {
            clusters.push(character.to_string());
        }
    }
    clusters
}

fn measure(database: &mut FontDatabase, request: &FontRequest, text: &str) -> i32 {
    shape_text(database, request, text).advance()
}

fn position_line(
    database: &mut FontDatabase,
    request: &FontRequest,
    text: String,
    width: i32,
    align: TextAlign,
    index: usize,
    last_line: bool,
) -> LineBox {
    let shaped = shape_text(database, request, &text);
    let shaped_advance = shaped.advance();
    let remaining = (width - shaped_advance).max(0);
    let offset = alignment_offset(align, shaped.paragraph_direction, remaining);
    let spaces = shaped
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .filter(|glyph| glyph.character == ' ')
        .count() as i32;
    let justify = align == TextAlign::Justify && !last_line && spaces > 0;
    let extra_per_space = if justify { remaining / spaces } else { 0 };
    let mut extra_remainder = if justify { remaining % spaces } else { 0 };
    let baseline = index as i32 * LINE_HEIGHT + ASCENT;
    let mut cursor = offset;
    let mut runs = Vec::new();
    for shaped_run in shaped.runs {
        let run_x = cursor;
        let mut glyphs = Vec::new();
        for glyph in shaped_run.glyphs {
            glyphs.push(PositionedGlyph {
                x: cursor + glyph.x_offset,
                baseline_y: baseline + glyph.y_offset,
                glyph: glyph.clone(),
            });
            cursor += glyph.advance;
            if justify && glyph.character == ' ' {
                cursor += extra_per_space;
                if extra_remainder > 0 {
                    cursor += 1;
                    extra_remainder -= 1;
                }
            }
        }
        runs.push(LineRun {
            font: shaped_run.font,
            family: shaped_run.family,
            script: shaped_run.script,
            direction: shaped_run.direction,
            x: run_x,
            width: cursor - run_x,
            glyphs,
        });
    }
    LineBox {
        index,
        y: index as i32 * LINE_HEIGHT,
        baseline,
        available_width: width,
        used_width: if justify { width } else { shaped_advance },
        offset,
        text,
        runs,
    }
}

fn alignment_offset(align: TextAlign, direction: TextDirection, remaining: i32) -> i32 {
    match align {
        TextAlign::Left => 0,
        TextAlign::Right => remaining,
        TextAlign::Center => remaining / 2,
        TextAlign::Justify => 0,
        TextAlign::Start => match direction {
            TextDirection::Ltr => 0,
            TextDirection::Rtl => remaining,
        },
        TextAlign::End => match direction {
            TextDirection::Ltr => remaining,
            TextDirection::Rtl => 0,
        },
    }
}
