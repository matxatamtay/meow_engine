# W17 font database

W17 introduces a deterministic font database between computed style and shaping.

## Model

A `FontFace` has a stable `FontId`, normalized family matching, weight, slant, script hints, Unicode coverage, units-per-em metadata, and an explicit source. The database owns faces and caches fallback selections by family list, weight, slant, locale, and Unicode scalar.

The built-in fixture database contains synthetic metadata for Latin, Vietnamese, Arabic, bold, italic, and last-resort faces. These faces do not depend on fonts installed on the host, so CI and developer snapshots remain identical.

## Discovery and skrifa

`FontDatabase::discover_system_paths()` returns sorted `.ttf`, `.otf`, and `.ttc` candidates without mutating deterministic face order. Applications may inspect and register those candidates later.

`register_font_bytes()` validates single OpenType font bytes through `skrifa::FontRef`, reads units-per-em through the skrifa metadata provider, and stores a stable content digest rather than embedding host paths in cache keys.

## Fallback

Selection first requires character coverage, then scores requested family order, slant, weight distance, script suitability, and stable registration order. Arabic and Vietnamese have explicit default fallback families, followed by a universal last-resort face.

File-backed fixtures cover Latin, Vietnamese precomposed characters, requested bold/italic faces, missing-family fallback, and Arabic fallback.
