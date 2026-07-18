use std::ops::Range;

use crate::{FontDatabase, FontId, FontRequest, Script, script_for};

use super::model::{ShapedGlyph, ShapedRun, ShapedText, TextDirection};

#[derive(Clone, Debug)]
struct Cluster {
    range: Range<usize>,
    text: String,
    font: FontId,
    script: Script,
    direction: Option<TextDirection>,
}

#[derive(Clone, Debug)]
struct LogicalRun {
    range: Range<usize>,
    text: String,
    font: FontId,
    script: Script,
    direction: TextDirection,
    clusters: Vec<Cluster>,
}

/// Shapes text into deterministic visual-order glyph runs.
#[must_use]
pub fn shape_text(database: &mut FontDatabase, request: &FontRequest, text: &str) -> ShapedText {
    let paragraph_direction = paragraph_direction(text);
    let clusters = build_clusters(database, request, text, paragraph_direction);
    let mut runs = build_runs(clusters, paragraph_direction)
        .into_iter()
        .map(|run| shape_run(database, run))
        .collect::<Vec<_>>();
    if paragraph_direction == TextDirection::Rtl {
        runs.reverse();
    }
    for (visual_index, run) in runs.iter_mut().enumerate() {
        run.visual_index = visual_index;
    }
    ShapedText {
        paragraph_direction,
        runs,
    }
}

fn paragraph_direction(text: &str) -> TextDirection {
    text.chars()
        .find_map(strong_direction)
        .unwrap_or(TextDirection::Ltr)
}

fn build_clusters(
    database: &mut FontDatabase,
    request: &FontRequest,
    text: &str,
    paragraph_direction: TextDirection,
) -> Vec<Cluster> {
    let mut clusters = Vec::<Cluster>::new();
    for (start, character) in text.char_indices() {
        let end = start + character.len_utf8();
        if is_combining_mark(character)
            && let Some(cluster) = clusters.last_mut()
        {
            cluster.range.end = end;
            cluster.text.push(character);
            continue;
        }
        let classified_script = script_for(character);
        let direction = strong_direction(character);
        let inherited = (classified_script == Script::Common && direction.is_none())
            .then(|| {
                clusters
                    .last()
                    .map(|cluster| (cluster.font, cluster.script))
            })
            .flatten();
        let (font, script) = inherited
            .unwrap_or_else(|| (database.select_face(request, character), classified_script));
        clusters.push(Cluster {
            range: start..end,
            text: character.to_string(),
            font,
            script,
            direction,
        });
    }

    let mut last_strong = paragraph_direction;
    for cluster in &mut clusters {
        if let Some(direction) = cluster.direction {
            last_strong = direction;
        } else {
            cluster.direction = Some(last_strong);
        }
    }
    clusters
}

fn build_runs(clusters: Vec<Cluster>, paragraph_direction: TextDirection) -> Vec<LogicalRun> {
    let mut runs = Vec::<LogicalRun>::new();
    for cluster in clusters {
        let direction = cluster.direction.unwrap_or(paragraph_direction);
        if let Some(run) = runs.last_mut()
            && run.font == cluster.font
            && run.script == cluster.script
            && run.direction == direction
            && run.range.end == cluster.range.start
        {
            run.range.end = cluster.range.end;
            run.text.push_str(&cluster.text);
            run.clusters.push(cluster);
            continue;
        }
        runs.push(LogicalRun {
            range: cluster.range.clone(),
            text: cluster.text.clone(),
            font: cluster.font,
            script: cluster.script,
            direction,
            clusters: vec![cluster],
        });
    }
    runs
}

fn shape_run(database: &FontDatabase, run: LogicalRun) -> ShapedRun {
    let face = database
        .face(run.font)
        .expect("run font remains registered");
    let mut glyphs = Vec::new();
    for (cluster_index, cluster) in run.clusters.iter().enumerate() {
        let base = cluster
            .text
            .chars()
            .find(|character| !is_combining_mark(*character));
        for character in cluster.text.chars() {
            let combining = is_combining_mark(character);
            let glyph_id = if script_for(character) == Script::Arabic && !combining {
                arabic_glyph_id(character, cluster_index, &run.clusters)
            } else {
                u32::from(character)
            };
            glyphs.push(ShapedGlyph {
                glyph_id,
                cluster: cluster.range.start,
                character,
                advance: if combining {
                    0
                } else {
                    glyph_advance(base.unwrap_or(character), run.script)
                },
                x_offset: if combining { -3 } else { 0 },
                y_offset: if combining { -4 } else { 0 },
            });
        }
    }
    if run.direction == TextDirection::Rtl {
        glyphs.reverse();
    }
    let advance = glyphs.iter().map(|glyph| glyph.advance).sum();
    ShapedRun {
        logical_range: run.range,
        visual_index: 0,
        font: run.font,
        family: face.family.clone(),
        script: run.script,
        direction: run.direction,
        text: run.text,
        glyphs,
        advance,
        ascent: 12,
        descent: 4,
        line_gap: 0,
    }
}

fn glyph_advance(character: char, script: Script) -> i32 {
    if character.is_whitespace() {
        4
    } else if character.is_ascii_punctuation() {
        5
    } else {
        match script {
            Script::Arabic => 9,
            Script::Latin | Script::Common => 8,
            Script::Other => 10,
        }
    }
}

fn arabic_glyph_id(character: char, index: usize, clusters: &[Cluster]) -> u32 {
    let joins_previous = index > 0 && cluster_joins_arabic(&clusters[index - 1]);
    let joins_next = index + 1 < clusters.len() && cluster_joins_arabic(&clusters[index + 1]);
    let form = match (joins_previous, joins_next) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    };
    0x20_0000 + form * 0x2_0000 + u32::from(character)
}

fn cluster_joins_arabic(cluster: &Cluster) -> bool {
    cluster
        .text
        .chars()
        .find(|character| !is_combining_mark(*character))
        .is_some_and(|character| {
            script_for(character) == Script::Arabic && character.is_alphabetic()
        })
}

fn strong_direction(character: char) -> Option<TextDirection> {
    if character.is_ascii_digit() {
        return Some(TextDirection::Ltr);
    }
    match script_for(character) {
        Script::Arabic => Some(TextDirection::Rtl),
        Script::Latin | Script::Other => Some(TextDirection::Ltr),
        Script::Common => None,
    }
}

#[must_use]
pub fn is_combining_mark(character: char) -> bool {
    matches!(
        u32::from(character),
        0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff
    )
}
