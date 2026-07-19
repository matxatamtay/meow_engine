# W40 profiling and cache

Computed styles are interned as shared `Arc<ComputedStyle>` values inside each style-engine run. Identical elements reuse the same allocation while mutation generations and diagnostics remain per element.

The deterministic pixel font uses a process-wide glyph bitmap cache with hit, miss, and resident counters. The image cache is shared by a navigator across reloads, bounded by entry count, and reports hits, misses, decodes, evictions, resident entries, and bytes. `BrowserEngine` also reports document-view rebuilds and same-viewport cache hits.

`DocumentViewMetrics` records style, box-tree, fragment-layout, interaction, total build time, structural counts, style-sharing counts, glyphs, and images. Tracing spans named `document_view_build`, `style_compute`, `box_tree_build`, `fragment_layout`, `display_list_build`, and `image_decode` provide flamegraph/perf attachment points.

Run the deterministic 200-card benchmark corpus with:

```bash
cargo run --release -p meow-engine --example pipeline_benchmark -- 200
```

The regression test locks structural counts, high style-sharing reuse, identical repeated command streams, and second-frame glyph-cache hits.
