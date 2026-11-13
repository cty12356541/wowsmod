# Mod Library and Guided Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a filterable local Mod project library, project-scoped previews, reliable same-ship application switching, and curated AI production guidance.

**Architecture:** A lightweight persisted catalog is separate from project assets and the game installation registry. The existing preview state is instantiated lazily per project ID; a transaction service switches same-ship SDK variants with recoverable snapshots. A new library screen opens the existing workbench using an explicit project URL.

**Tech Stack:** Rust 2021, existing clap/tiny_http/serde/sha2; React 18, TypeScript, Vite 6, Zustand and Three.js r147.

**Spec:** `docs/superpowers/specs/2026-11-13-mod-library-and-guidance-design.md` (approved by the user).

## Global Constraints

- Work only in this managed worktree; preserve the original workspace and its running synchronization job. Use the existing registered client only for read-only samples. Never start a game or write its res_mods during automated tests.
- Follow `C:/Users/Administrator/.codex/AGENTS.md`: routine implementation/documentation/review uses gpt-6.1-sol, complex transaction work and final critical review uses gpt-6-astra; explicit reasoning_effort=ultra and fork_turns=none.
- Existing mod.toml, `preview <project>`, render and MCP remain compatible. No online downloads of Mod/SDK assets, no full-disk scanning, no game UI switcher.
- Project identity is not mod.name. Selection does not install. Applied state is scoped by actual client root/build/ship and project path, not a stale project manifest.
- A same-ship switch leaves at most one SDK variant active; conflicting other ships/global files and user modifications are preserved. Failed/interrupted writes must recover the previous snapshot.
- Missing metadata stays unknown; missing arbitrary-ship model/simulation data never falls back silently to Bourgogne. Source placeholders are legal; installed placeholders are not complete resources.
- Meaningful behavior tests precede production changes. Low-risk documentation/build packaging uses contract checks instead of tests mirroring text. Baseline and final full suites are run once each; focused tests cover intermediate changes.
- Frontend source is imported from the locally cached public branch's preview_app only, recorded at 1a1fba8, and lives in this worktree. Build output is deliberately embedded into crates/wowsmod/preview_web; never edit the externally auto-synced preview_app.
- Controller owns the task ledger, integration/build scripts, and final integration. Workers own only the files named in their task, must not revert others, and must not spawn agents. Stage/commit exact owned files only.

## Review Focus

- Same display name, same PnF name, SDK/parameter ship aliases, case/Junction paths and relocated projects must not cause identity confusion (Tasks 1–3 tests).
- Switch crash after file replacement but before registry commit, preexisting foreign files, shared resources, and user-modified installs must not lose data (Task 2 tests).
- Concurrent tabs and same revision IDs must not reuse another project's blobs, jobs, actions, caches or applied badge (Tasks 3–4 tests).
- No-game/invalid metadata, stale source paths, unsupported simulation and global loose projects must remain browseable without fake ship data (Tasks 1, 3, 4 tests).
- Older instructions and copied projects must not cause silent installation, globally rewritten guidance, unsupported animation claims, or misquoted PowerShell examples (Task 5 scenario checks).

## Task 1: Local catalog, ship metadata and CLI

**Files:** Create `crates/wowsmod/src/library.rs`, `crates/wowsmod/src/ship_catalog.rs`; modify only the library module/command dispatch portions of `crates/wowsmod/src/main.rs`. Inline unit tests in the new modules. Do not edit registry, preview, UI, or skill files.

**Interfaces:**
- `library::LibraryArgs` and `library::cmd_library(cli: &Cli, args: &LibraryArgs) -> anyhow::Result<i32>`.
- `library::LibraryStore::open(home: &Path) -> Result<LibraryStore>`; methods `list`, `add(&Path)`, `scan(&Path)`, `update(id, patch)`, `relocate(id, &Path)`, `remove(id)`, `resolve(id)` and `touch_opened(id)`. Return serializable structs; controller/Task 3 may adapt wrappers, not duplicate storage logic.
- Store path `<m4cmd::home_dir()>/library.json`, schema version 1. Records: id, path, display_name, tags, favorite, status (`working`, `review`, `complete`, `archived`), added_unix, opened_unix. Deterministic ID from canonical path is acceptable; relocating preserves it. Add is idempotent for canonical path.
- `ship_catalog::canonical_ship_key(&str) -> String`; `ship_catalog::ShipCatalog` loads once per root/build and exposes metadata lookup. Canonical `FSB510_Bourgogne` and `PFSB510_Bourgogne` map to `PFSB510`; unknown and empty remain explicit, not guessed. Preserve case-insensitive lookup.
- CLI: `library add <path>`, `scan <root>`, `list`, `remove <id>`, `relocate <id> <path>`, `update <id> [--name ... --status ... --favorite true|false --tag ...]`. `--json` output always includes ok; list includes entries and source/metadata warnings. Launching the library is Task 3's `preview --library`.

- [ ] Write failing tests for canonical path dedupe with distinct same-name projects; ID-preserving relocation; update/archiving/removal without touching assets; corrupt/unknown schema preservation; scan pruning plus inaccessible/invalid entries; ship aliases and unknown metadata.
- [ ] Run the focused failing tests; record expected RED evidence.
- [ ] Implement atomic locked catalog writes and bounded explicit-root scan. Stop scanning below a discovered project, skip assets/out/target/node_modules/.git/.wowsmod/.install and linked directory cycles. No full-disk scan. Preserve good projects when individual candidates fail.
- [ ] Load GameParams/localized names via existing reader and cache by canonical client root/build; fall back to last cache with explicit provenance. Library listing does not create PreviewState or modify projects.
- [ ] Run catalog/ship tests and CLI smoke on temporary projects. Verify a before/after tree hash shows reads/add/remove never mutate assets or source mod.toml.
- [ ] Commit exact owned files and write report with signatures, commands, RED/GREEN and limitations.

## Task 2: Transactional application and recovery

**Files:** `crates/wowsmod/src/registry.rs`, optional child `crates/wowsmod/src/registry/switch.rs`, and `crates/wowsmod/src/m4cmd.rs`. Do not edit main/preview/library/UI/docs. Normal legacy install/uninstall behavior must remain valid.

**Interfaces:**
- `registry::switch(res_mods: &Path, info: &ModInfo, items: &[InstallItem], dry_run: bool) -> Result<Value>` prepares same-ship replacement and updates the registry under one process lock.
- `registry::recover_pending(res_mods: &Path) -> Result<Value>` restores or finalizes a journal before any next mutating registry action; all install/uninstall/migrate write paths must respect pending work. Expose recovery information in errors; do not silently discard a journal.
- `m4cmd::apply_project(cli: &Cli, project: &Path, dry_run: bool) -> Result<Value>` resolves the selected client, rejects running game, prepares/stub-fills items then calls switch. It may reuse internal m4 preparation methods. Result includes target client/build/res_mods, previous mod/project, selected mod/project, counts, warnings and dry_run.
- Replacement set: for SDK nonempty ship, currently managed SDK entries with the same normalized ship; same-name prior install only when it is that ship/project, otherwise conflict. Loose/global applications never replace unrelated entries. Do not depend on Task 1 for alias normalization (a small helper may later be consolidated).
- Dry-run does not write registry, project manifests, lock, backup or journal. State-changing application updates only the selected candidate's manifest after success; original manifest failure must not strand an inconsistent game registry.

- [ ] Add failing fixture tests for A→B, same PnF name/different project, PFSB/FSB aliases, failed prepare/conflict leaving A, shared resource ownership, foreign-file restoration and external modifications blocking replace.
- [ ] Add deterministic fault injection in test-only seams for interruption at file-write and registry-commit boundaries; test full bytes/hash/registry/backup restoration, without reading the old project or old client package. Add stale-lock recovery and competing-operation tests.
- [ ] Implement a durable prewrite snapshot/journal of touched paths and registry/backup state. Validate relative paths, current installed hashes and planned conflicts before committing. For crashes, next mutation restores prestate or finalizes a committed transaction; preserve evidence if restoration fails.
- [ ] Reject live game before any mutation. Use a lock protocol that cannot expire a live long-running operation simply at ten minutes; recovery must handle an actually abandoned lock safely on Windows.
- [ ] Run focused registry/application tests, including existing registry coexistence/uninstall tests. Report exactly how process-crash versus ordinary error rollback is covered.
- [ ] Commit exact files and write report; CLI routing and HTTP wiring belong to Task 3.

## Task 3: Project-scoped library/preview API and application routing

**Dependencies:** Tasks 1–2. **Files:** `crates/wowsmod/src/preview/{mod.rs,http.rs,state.rs}`, new `preview/library.rs` as appropriate; root main.rs only for Apply command wiring. Do not edit UI or registry internals without controller coordination.

**Interfaces:**
- Keep `preview <project>`; add `preview --library` (project optional; conflicting project+library arguments rejected). Expose `apply <project-path> [--dry-run]` through main, calling m4cmd::apply_project.
- Library endpoints: GET `/api/v1/library` returns `{ok, projects, ships, facets, game, warnings}`; POST `/api/v1/library/add`, `/scan`, `/projects/{id}/update`, `/relocate`, `/remove`, `/open`. JSON bodies use path/root/id and metadata fields; open returns project URL `/?project=<id>`.
- Scoped endpoints `/api/v1/projects/{id}/project|state|steps|parts|model|textures|sim|events|geometry/...|blob/...|jobs/...` reuse existing behavior with one explicit project state. Rewrite returned resource URLs to the same scope. Unknown IDs return 404; unavailable projects return actionable errors, not the previous state.
- Scoped POST `/api/v1/projects/{id}/apply` accepts `{"dry_run":true|false}` and returns a job or immediate preflight result. `/uninstall` means uninstall exactly. Pass st.game.root/selected client into child commands. Selecting/opening never installs.
- Library summaries include independent production status and actual installation status. Match canonical source project path (future ID optional), not name; detect applied content behind current assets without declaring the current head already applied.
- Capability fields advertise library/apply/undo/restore/simulation as actually supported. Unknown ships/globals remain in the catalog; when 3D preview is unavailable return a clear capability/message rather than seeded Bourgogne content.

- [ ] Add failing tests for precise install/uninstall/apply action dispatch, explicit --game propagation, two IDs with the same revision and name, scoped resource/job URLs, unknown/missing IDs and read-only catalog requests.
- [ ] Implement lazy per-project states plus bounded disposal; no global selected state, no expensive full-state creation for library listing. Preserve legacy single-project and warm-render endpoints.
- [ ] Make returned API types and request errors stable; JSON mutating endpoints validate content type and locally expected origin, paths come from catalog registration, removal never deletes projects. Serialize updates through the catalog/registry locks.
- [ ] Fix installed-state identity and unsupported capability reporting. Remove inappropriate Bourgogne fallback when project/model metadata is absent; disable unsupported simulation while retaining valid generic model display.
- [ ] Run focused HTTP/session tests and legacy preview/render tests. Provide a temporary no-game library server smoke (requests only; no real installs).
- [ ] Commit exact files; update API contract in a short controller-visible report for Task 4.

## Task 4: Library filters, same-ship variants and workbench actions

**Dependency:** Task 3 API contract. **Files:** `preview_app/web/src` (new Library view/store and tests, App/TitleBar/Viewport/api/types/store/History/styles/i18n), web package/lock for a focused test runner if necessary. Do not edit Rust/skill files. Root owns build/embed script.

**Interfaces:** Use Task 3 actual JSON contract. Query `?project=<id>` selects scoped API base; missing project in library mode opens Library. Existing direct-preview and `?render` remain functional. Full page navigation between projects is intentional.

- [ ] Add failing behavior tests for OR within facet/AND across facets, search across ship/mod/tag in Chinese/English, default archive hiding, favorites, empty/missing metadata and deterministic sort. Include three same-ship variants with identical mod names but distinct IDs.
- [ ] Implement responsive Library: add/scan/import-path flow, filters, ship group cards, variant cards, editable display name/tags/status/favorite, lost-path relocation and unregister-only removal. Provide keyboard-accessible controls and clear progress/errors.
- [ ] Add breadcrumb/return-to-library and quick same-ship selector; retain library filters/selected ship/scroll. Selection navigates project only. Apply first fetches a dry-run preview, shows current→new and target build, then explicitly submits; running game/failure messages remain visible.
- [ ] Make installed badge truthful and distinct from production status. Disable unsupported undo/restore/sim with reason instead of presenting executable actions. Handle global projects with catalog actions even when 3D preview unavailable.
- [ ] Scope revision caches and requests; dispose viewer, timers and SSE on unmount. Add a regression test that two projects with revision s1 cannot reuse material manifests.
- [ ] Run unit tests/typecheck/build. Browser-check library empty/list/filters/variants/apply preview, two projects/tabs and old workbench at wide and narrow desktop sizes; controller handles screenshot inspection with a computer-use agent.
- [ ] Commit exact frontend files and report the observed flows plus any environment limitations.

## Task 5: Curated built-in guidance

**Files:** high-frequency `docs/skill/root.md`, `workflow.md`, `workflow.book.md`, `workflow.replicate.md`, `workflow.review.md`, `workflow.layers*.md` only where contradictory, `ref.project/install/migrate/glow/model.animate/pack.md`; add `ref.variants.md`; move case details to an appropriate retained case node/archive with links. Do not edit product code or fabricate new command support.

**Interfaces:** Existing build.rs embeds skill markdown and guide.rs validates the tree. Preserve existing node IDs and working parent/child links. New command examples: `library add/scan/list/update/remove/relocate`, `preview --library`, `apply <project> --dry-run`; final contract adjustment after Tasks 1–3 is allowed.

- [ ] Read relevant skill-authoring guidance and inventory conflicts using the approved spec; keep a short requirements-to-node checklist in the report, not another universal ruleset.
- [ ] Rewrite core paths into request/reference→small concept→chosen direction→relevant layers→preview checks→authorized port acceptance. Simple edits bypass irrelevant design layers. Preserve evidence-driven aesthetic methods, not universal color/ratio/size thresholds.
- [ ] Move Robin P5/C2 hardcoded numbers and historical experiments out of universal instructions; label build/ship/port and known limits. Distinguish proven/experimental/unknown, migration versus rebase, source placeholders versus installed resources, and file/register/visual evidence.
- [ ] Add multi-variant project selection guidance without silent install or same-ship stacking. Stop requiring every Mod iteration to rewrite the global skill. Correct quoted color examples and required Main.py packaging wording.
- [ ] Validate help tree/link/length contracts with the rebuilt executable. Review five scenarios (simple recolor, video replication, character theme, unknown version, same-ship variants) against command support; no new dependencies or long benchmark needed.
- [ ] Commit exact docs and report retained IDs, moved case notes, checks and unresolved technical limitations.

## Task 6: Reproducible build, integration, review and delivery

**Owner:** controller with a fresh critical reviewer. **Files:** `scripts/build_preview.ps1` (or equivalent Node build script), embedded preview_web generated outputs, README/API guidance, integration fixture/script and test evidence under ignored out. No source workspace/game mutation.

- [ ] Build frontend from tracked source, copy only expected dist files to embedded directory and write a source/build manifest. Preserve license notices. Record frontend baseline source commit 1a1fba8; add a verification mode that catches mismatched embedded output.
- [ ] Verify catalog fixtures (2 ships, 3 variants of one ship, global/missing projects), same-name identity, stale metadata and path aliases; validate application and interrupted recovery in a temporary fake client.
- [ ] Run full Rust suite with the available real samples and oozextract feature, web tests/typecheck/build, embedded help checks and production release build. Record any baseline failures separately; no silent skip claims.
- [ ] UI inspection through computer-use delegate, only for the local tool workbench; previous game UI Escape stop remains respected. If screenshots cannot be inspected, state that limitation rather than declaring visual QA passed.
- [ ] Fresh Astra ultra whole-branch review against the approved spec and diff; fix concrete findings with focused regression tests and re-review the touched area. Do not re-run unchanged broad suites unless a new concern justifies it.
- [ ] Update agentdash from actual evidence and SDD progress. Keep completed worktree/branch and executable artifact accessible; do not push, merge or publish unless requested. Report outcomes, tests, paths and material limitations.

## Execution rulings

- User explicitly said “开始” and the new AGENTS permits direct progress with clear authorization; no further design/plan approval is required.
- Independent Tasks 1, 2 and 5 may run concurrently with disjoint ownership. This adapts the skill's default serial implementation to the developer/user parallelism policy; main.rs is Task 1's until it finishes, registry/m4cmd is Task 2's, docs/skill is Task 5's. Task 3 waits for 1–2; Task 4 waits for the API contract.
- Each task gets scoped review; final critical review covers cross-module behavior. Workers do not launch their own reviewers.
- All installation tests use temporary fake clients; prior authorization for Robin's completed port test does not silently authorize more gameplay or real installations.
