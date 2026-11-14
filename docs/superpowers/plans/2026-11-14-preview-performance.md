# Preview Loading Performance Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Keep task ownership disjoint and retain existing generation/identity protections.

**Goal:** Reduce Robin preview preparation and static-review startup without lowering visual quality.

**Architecture:** A preview-local DDS basename index removes repeated full game-index scans. Static review no longer waits for optional simulation, and FX updates reuse the existing environment map.

**Tech Stack:** Rust, React/TypeScript, Three.js, existing Node tests and Windows release CLI.

**Spec:** `docs/superpowers/specs/2026-11-14-preview-performance-design.md`

## Constraints and review focus

- Preserve ASCII-insensitive unique-match lookup, including ambiguous/missing resources and project overrides.
- Preserve scoped snapshot refresh, source edits, catalog relocation/removal serialization and immutable resource identities.
- Preserve actual renderer material-generation guards, A/B readiness, camera behavior and full texture quality.
- Optional simulation must reject stale/unmounted responses and must not issue duplicate requests during overlapping resync.
- FX tint reset and later environment changes must retain the same colors and appearance without an extra PMREM generation.

## Task 1: Indexed preview resource lookup (Sol worker)

**Owner:** `crates/wowsmod/src/preview/state.rs`; tests in that module. No broader GameCtx, texcmd or registry changes unless a proven requirement is reported first.

- [ ] Add lookup regressions for mixed ASCII case, missing/non-DDS paths, unique match and multiple same basenames; establish a failing optimized-path check without timing-sensitive CI assertions.
- [ ] Build the DDS basename → unique entry index once for state construction, preserve ambiguity, and reuse it for material fallback lookups. Hoist constant material-path regexes out of material loops.
- [ ] Run relevant Rust preview/source-refresh tests; report commands, semantic preservation and benchmark results. Commit exact owned files.

## Task 2: Remove redundant frontend startup work (Sol worker)

**Owner:** `preview_app/web/src/store.ts`, `engine/viewer.ts`, relevant tests. Do not modify generated embedded output.

- [ ] Add a held-simulation regression proving static preview readiness must not wait for `/sim`, plus stale identity/unmount/overlap cases using the actual store.
- [ ] Reuse existing simulation generation/attempt guards for post-ready initialization; preserve optional legacy capabilities and full B/A rendering readiness.
- [ ] Add a real-method FX test preserving tint/reset while proving setFx does not regenerate the environment; update only necessary FX uniforms and request redraw.
- [ ] Run focused actual renderer/material/simulation tests, complete frontend tests/typecheck/build, then commit exact owned files.

## Task 3: Review, measured release and live update (controller)

- [ ] Fresh Astra scoped review of both fixes; resolve concrete findings with focused tests.
- [ ] Build frontend embedding, verify source/output hashes, run full Rust with real samples, and build release.
- [ ] Run the same isolated-project fresh-service timing; report measured before/after with sample limitations. Compare geometry/model/texture identities and inspect output if it differs.
- [ ] Preserve the active delivery data directory and registered project catalog. Stop only the verified owned preview process, replace the executable with a backup, restart the same explicit client/port and refresh the existing user-facing IAB tab.
- [ ] Verify the project remains registered and displayed, record actual evidence in agentdash, and report the performance change and any remaining limitations. Do not alter game installation.
