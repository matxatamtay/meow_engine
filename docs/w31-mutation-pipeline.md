# W31 mutation pipeline

DOM writes continue to emit explicit `DomMutation` records. Event callbacks, timer callbacks, form-state synchronization, and initial scripts all use the same mutation path.

The embedder collects all records produced in one task burst and schedules one document frame. The cached `DocumentView` is invalidated once, not once per record. Rendering flushes the batch and rebuilds style, layout, hit testing, and paint once for that frame. `MutationPipelineReport` exposes pending record count, frame state, and rebuild count for conformance tests.

This is frame-level coalescing. The lower `StyleEngine` still provides subtree-aware dirty roots, while the current `DocumentView` rebuild remains conservative at flush time.
