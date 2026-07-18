use std::{ops::RangeInclusive, path::PathBuf};

/// Stable identity within one font database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub u32);

/// Font posture used for deterministic matching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontSlant {
    #[default]
    Normal,
    Italic,
}

/// Broad Unicode script categories used by fallback and shaping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Script {
    Common,
    Latin,
    Arabic,
    Other,
}

/// Origin of one registered font face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSource {
    Synthetic(String),
    Memory { digest: u64 },
    SystemPath(PathBuf),
}

/// Inclusive Unicode scalar coverage ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontCoverage {
    ranges: Vec<RangeInclusive<u32>>,
}

impl FontCoverage {
    #[must_use]
    pub fn new(ranges: impl IntoIterator<Item = RangeInclusive<u32>>) -> Self {
        let mut ranges = ranges.into_iter().collect::<Vec<_>>();
        ranges.sort_by_key(|range| *range.start());
        Self { ranges }
    }

    #[must_use]
    pub fn contains(&self, character: char) -> bool {
        let scalar = u32::from(character);
        self.ranges.iter().any(|range| range.contains(&scalar))
    }

    #[must_use]
    pub fn latin_basic() -> Self {
        Self::new([0x0000..=0x024f])
    }

    #[must_use]
    pub fn vietnamese() -> Self {
        Self::new([0x0000..=0x024f, 0x0300..=0x036f, 0x1e00..=0x1eff])
    }

    #[must_use]
    pub fn arabic() -> Self {
        Self::new([
            0x0000..=0x007f,
            0x0600..=0x06ff,
            0x0750..=0x077f,
            0x08a0..=0x08ff,
        ])
    }

    #[must_use]
    pub fn universal() -> Self {
        Self::new([0x0000..=0x10ffff])
    }
}

/// One font face known to the database.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFace {
    pub id: FontId,
    pub family: String,
    pub weight: u16,
    pub slant: FontSlant,
    pub scripts: Vec<Script>,
    pub coverage: FontCoverage,
    pub units_per_em: u16,
    pub source: FontSource,
}

/// Inputs that affect deterministic fallback selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontRequest {
    pub families: Vec<String>,
    pub weight: u16,
    pub slant: FontSlant,
    pub locale: Option<String>,
}

impl Default for FontRequest {
    fn default() -> Self {
        Self {
            families: vec!["serif".to_owned()],
            weight: 400,
            slant: FontSlant::Normal,
            locale: None,
        }
    }
}

impl FontRequest {
    #[must_use]
    pub fn new(families: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            families: families.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }
}

/// A contiguous text range assigned to one fallback face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontSpan {
    pub byte_range: RangeInclusive<usize>,
    pub font: FontId,
    pub family: String,
    pub script: Script,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FallbackCacheKey {
    pub families: Vec<String>,
    pub weight: u16,
    pub slant: FontSlant,
    pub locale: Option<String>,
    pub character: char,
}
