# W51 release candidate

The RC gate includes four controlled pages in `tests/curated-sites/`: a
multilingual article, accessible forms and console, flex/transform/opacity, and
JavaScript mutation/timers. The real `meow-headless --url` path serves and
renders every page to PNG.

Promotion requires full Rust gates, unchanged selected WPT baseline, curated
corpus pass, zero new fuzz crashes, no budget violations, package tar smoke,
and review of privacy, threat model, known issues, and diagnostics bundle.

The corpus is local and stable. Passing it does not claim compatibility with
arbitrary current production websites.
