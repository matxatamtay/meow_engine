use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use skrifa::{FontRef, MetadataProvider, instance::NormalizedCoord};

use super::model::{
    FallbackCacheKey, FontCoverage, FontFace, FontId, FontRequest, FontSlant, FontSource, FontSpan,
    Script,
};

/// Deterministic font registry with explicit fallback ordering and cache keys.
#[derive(Clone, Debug, Default)]
pub struct FontDatabase {
    faces: Vec<FontFace>,
    fallback_cache: BTreeMap<FallbackCacheKey, FontId>,
}

impl FontDatabase {
    /// Creates the CI-stable bundled metadata set used by text fixtures.
    #[must_use]
    pub fn deterministic() -> Self {
        let mut database = Self::default();
        database.register_synthetic(
            "Meow Sans",
            400,
            FontSlant::Normal,
            vec![Script::Latin, Script::Common],
            FontCoverage::latin_basic(),
        );
        database.register_synthetic(
            "Meow Sans Vietnamese",
            400,
            FontSlant::Normal,
            vec![Script::Latin, Script::Common],
            FontCoverage::vietnamese(),
        );
        database.register_synthetic(
            "Meow Sans",
            700,
            FontSlant::Normal,
            vec![Script::Latin, Script::Common],
            FontCoverage::vietnamese(),
        );
        database.register_synthetic(
            "Meow Sans",
            400,
            FontSlant::Italic,
            vec![Script::Latin, Script::Common],
            FontCoverage::vietnamese(),
        );
        database.register_synthetic(
            "Meow Arabic",
            400,
            FontSlant::Normal,
            vec![Script::Arabic, Script::Common],
            FontCoverage::arabic(),
        );
        database.register_synthetic(
            "Meow Last Resort",
            400,
            FontSlant::Normal,
            vec![Script::Common, Script::Latin, Script::Arabic, Script::Other],
            FontCoverage::universal(),
        );
        database
    }

    #[must_use]
    pub fn faces(&self) -> &[FontFace] {
        &self.faces
    }

    #[must_use]
    pub fn face(&self, id: FontId) -> Option<&FontFace> {
        self.faces.get(id.0 as usize)
    }

    pub fn register_synthetic(
        &mut self,
        family: impl Into<String>,
        weight: u16,
        slant: FontSlant,
        scripts: Vec<Script>,
        coverage: FontCoverage,
    ) -> FontId {
        self.register_face(FaceRegistration {
            family: family.into(),
            weight,
            slant,
            scripts,
            coverage,
            units_per_em: 1_000,
            source: FontSource::Synthetic("bundled-metrics-v1".to_owned()),
        })
    }

    /// Validates OpenType bytes through skrifa and records caller-supplied metadata.
    pub fn register_font_bytes(
        &mut self,
        bytes: &[u8],
        family: impl Into<String>,
        weight: u16,
        slant: FontSlant,
        scripts: Vec<Script>,
        coverage: FontCoverage,
    ) -> Result<FontId, String> {
        let font = FontRef::new(bytes).map_err(|error| error.to_string())?;
        let units_per_em = font
            .metrics(
                skrifa::instance::Size::unscaled(),
                &[] as &[NormalizedCoord],
            )
            .units_per_em;
        Ok(self.register_face(FaceRegistration {
            family: family.into(),
            weight,
            slant,
            scripts,
            coverage,
            units_per_em,
            source: FontSource::Memory {
                digest: fnv1a64(bytes),
            },
        }))
    }

    /// Returns sorted font file candidates without changing deterministic face order.
    #[must_use]
    pub fn discover_system_paths(roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut output = Vec::new();
        for root in roots {
            collect_font_paths(root, &mut output);
        }
        output.sort();
        output.dedup();
        output
    }

    #[must_use]
    pub fn select_face(&mut self, request: &FontRequest, character: char) -> FontId {
        let key = FallbackCacheKey {
            families: request
                .families
                .iter()
                .map(|family| normalize_family(family))
                .collect(),
            weight: request.weight.clamp(1, 1_000),
            slant: request.slant,
            locale: request.locale.as_deref().map(str::to_ascii_lowercase),
            character,
        };
        if let Some(font) = self.fallback_cache.get(&key) {
            return *font;
        }
        let script = script_for(character);
        let selected = self
            .faces
            .iter()
            .filter(|face| face.coverage.contains(character))
            .min_by_key(|face| match_score(face, &key, script))
            .map(|face| face.id)
            .expect("deterministic database includes a universal last-resort face");
        self.fallback_cache.insert(key, selected);
        selected
    }

    #[must_use]
    pub fn resolve_text(&mut self, request: &FontRequest, text: &str) -> Vec<FontSpan> {
        let mut spans = Vec::<FontSpan>::new();
        for (start, character) in text.char_indices() {
            let end = start + character.len_utf8() - 1;
            let classified_script = script_for(character);
            let inherited = (classified_script == Script::Common)
                .then(|| spans.last().map(|span| (span.font, span.script)))
                .flatten();
            let (font, script) = inherited
                .unwrap_or_else(|| (self.select_face(request, character), classified_script));
            let face = self.face(font).expect("selected face remains registered");
            if let Some(last) = spans.last_mut()
                && last.font == font
                && (last.script == script || classified_script == Script::Common)
                && *last.byte_range.end() + 1 == start
            {
                last.byte_range = *last.byte_range.start()..=end;
                continue;
            }
            spans.push(FontSpan {
                byte_range: start..=end,
                font,
                family: face.family.clone(),
                script,
            });
        }
        spans
    }

    #[must_use]
    pub fn dump(&self) -> String {
        let mut output = String::from("#font-database\n");
        for face in &self.faces {
            use std::fmt::Write as _;
            writeln!(
                output,
                "face={} family={:?} weight={} slant={:?} scripts={:?} upem={} source={:?}",
                face.id.0,
                face.family,
                face.weight,
                face.slant,
                face.scripts,
                face.units_per_em,
                face.source,
            )
            .expect("writing to String cannot fail");
        }
        output
    }

    fn register_face(&mut self, registration: FaceRegistration) -> FontId {
        let id = FontId(self.faces.len() as u32);
        self.faces.push(FontFace {
            id,
            family: registration.family,
            weight: registration.weight.clamp(1, 1_000),
            slant: registration.slant,
            scripts: registration.scripts,
            coverage: registration.coverage,
            units_per_em: registration.units_per_em,
            source: registration.source,
        });
        self.fallback_cache.clear();
        id
    }
}

struct FaceRegistration {
    family: String,
    weight: u16,
    slant: FontSlant,
    scripts: Vec<Script>,
    coverage: FontCoverage,
    units_per_em: u16,
    source: FontSource,
}

fn match_score(
    face: &FontFace,
    key: &FallbackCacheKey,
    script: Script,
) -> (bool, u16, usize, bool, u32) {
    let family = normalize_family(&face.family);
    let family_rank = key
        .families
        .iter()
        .position(|requested| requested == &family)
        .unwrap_or_else(|| default_family_rank(&family, script));
    (
        face.slant != key.slant,
        face.weight.abs_diff(key.weight),
        family_rank,
        !face.scripts.contains(&script),
        face.id.0,
    )
}

fn default_family_rank(family: &str, script: Script) -> usize {
    match (script, family) {
        (Script::Arabic, "meow arabic") => 100,
        (Script::Latin, "meow sans vietnamese") => 101,
        (Script::Latin | Script::Common, "meow sans") => 102,
        (_, "meow last resort") => usize::MAX - 1,
        _ => usize::MAX / 2,
    }
}

#[must_use]
pub fn script_for(character: char) -> Script {
    let scalar = u32::from(character);
    match scalar {
        0x0041..=0x024f | 0x1e00..=0x1eff => Script::Latin,
        0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff => Script::Arabic,
        0x0000..=0x0040 | 0x0300..=0x036f | 0x2000..=0x206f => Script::Common,
        _ => Script::Other,
    }
}

fn normalize_family(family: &str) -> String {
    family.trim().trim_matches(['\'', '"']).to_ascii_lowercase()
}

fn collect_font_paths(path: &Path, output: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc"
                )
            })
        {
            output.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        collect_font_paths(&entry, output);
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
