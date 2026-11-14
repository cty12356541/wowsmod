# Preview loading performance

The user reports slow loading of the imported Bourgogne × Robin project in the library workbench. Improve loading in the existing managed worktree without changing model geometry, texture resolution, rendering quality, installation state, or project assets.

## Evidence

- Existing release, isolated Robin source copy, game build 8871067, cached GLB: fresh render-service startup 15.03 s, including state preparation 10.36 s, browser page 1.44 s and first render 2.94 s. This is one local sample, not a promise about every machine.
- Real resource index: 344,451 entries. Robin triggers 147 game texture basename lookups; the current repeated scan takes 7,650.4 ms. A once-built basename index plus the identical queries takes 28.7 ms; results agree including missing/ambiguous names.
- Workbench boot waits for simulation before material preparation. Simulation is optional for static review.
- Viewer construction and the subsequent setFx call regenerate the same environment map twice.
- Warm project/state HTTP requests cost about 0.3–0.4 s, largely from tasklist; ordinary scoped model requests cost about 27 ms. These are separate from the proven cold-start bottleneck.

## Approach

Use a preview-local DDS basename lookup with the existing ASCII-insensitive and unique-match rules. Compile invariant material-path regular expressions once per build rather than per material. Keep project-first resolution and the existing session freshness/identity gate.

Move optional simulation initialization after complete static preview readiness, retaining its generation, cancellation, deduplication, and failure semantics. Keep full B/A material readiness, comparison state, texture quality and window.__ready behavior. Update FX tint directly without rebuilding the environment map.

A persistent project-state cache would introduce invalidation work before the main bottleneck is removed. Loading only currently used materials could reduce PNG traffic but also affects comparison, bloom and idle predicates. Both are deferred; so are tasklist caching and reduced-resolution preview.

## Validation and delivery

Use meaningful regression tests for lookup ambiguity/case and actual boot ordering/cancellation/FX behavior, retain existing source-refresh and renderer stale-result tests, and measure a fresh release against the same isolated project and client. Compare model/texture resource identities and inspect final render output. Rebuild the embedded frontend and update the running local workbench only after validation, preserving its registered project library and URL. No game installation or game GUI is needed.

The user already authorized improving this toolchain and now requests a remedy for slow loading. Per their AGENTS.md, this bounded correctness-preserving repair proceeds without another approval round. Existing codex/mod-library worktree is retained; no merge or push.
