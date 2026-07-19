# MeowEngine v0.2-alpha

First public alpha after the W1-W52 year-one plan. This Linux-first engine is
experimental, not a daily-driver browser or production security boundary.

Highlights: HTML/CSS/DOM and layout subsets, CPU/GPU presentation, images/SVG,
interaction/forms/classic JavaScript/events/timers/Fetch/CORS/storage,
single-process WebSocket, versioned IPC, content/network processes, crash
recovery, experimental sandbox, selected WPT/accessibility, F12 inspector,
profile migration, packaging, diagnostics, fuzzing, budgets, and RC corpus.

Local evidence: selected WPT 20/20; fuzz 10,000 mutations with zero new crashes;
release budgets without violations; curated corpus 4/4 rendered; the same
Ubuntu-24.04-built tar passed headless and multiprocess crash-recovery smoke on
Ubuntu 24.04 and Fedora 42 containers.

Read `docs/known-issues.md`, `docs/privacy.md`, and `docs/threat-model.md` before
running untrusted content.
