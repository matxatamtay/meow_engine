# W18 shaping and bidi

W18 adds a deterministic shaping layer above the W17 font database.

## Pipeline

Text is segmented into grapheme-like clusters, script runs, directional runs, and font runs. Combining marks attach to the preceding base cluster, keep the base byte offset, have zero advance, and receive deterministic mark offsets.

Paragraph direction is selected from the first strong Latin or Arabic character. Neutral characters inherit the preceding strong direction. LTR paragraphs preserve logical run order; RTL paragraphs reverse run order. Glyphs inside RTL runs are emitted in visual order.

## Metrics and Arabic subset

Bundled synthetic metrics use stable integer advances: spaces 4px, punctuation 5px, Latin 8px, Arabic 9px, and other scripts 10px. Runs expose 12px ascent, 4px descent, and no line gap.

Arabic alphabetic clusters receive deterministic isolated, final, initial, or medial pseudo-glyph identifiers based on neighboring Arabic clusters. This validates joining and visual ordering without depending on host font rasterization. Full OpenType GSUB/GPOS shaping remains future work.

## Fixtures

Three file-backed snapshots cover decomposed Vietnamese marks, Arabic joining and RTL visual glyph order, and mixed Latin/Arabic direction runs.
