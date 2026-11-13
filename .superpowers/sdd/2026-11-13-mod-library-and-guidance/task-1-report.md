# Task 1 report — local library, ship catalog and CLI

Status: **DONE_WITH_CONCERNS**. Release CLI and all scoped tests pass. The Windows dev executable overflows its default 1 MiB stack before parsing even `--version`; this was diagnosed but is outside the assigned module/Library-dispatch edits in main.rs. No other main behavior was changed.

## Files and interfaces

Owned production files: `crates/wowsmod/src/library.rs`, `crates/wowsmod/src/ship_catalog.rs`, and five added lines in `crates/wowsmod/src/main.rs`. This report is the fourth owned delivery file. Registry, m4cmd, preview, UI, documentation and skill edits belong to other workers and were not changed by this worker.

```rust
library::cmd_library(cli: &Cli, args: &LibraryArgs) -> anyhow::Result<i32>
LibraryStore::open(home: &Path) -> Result<LibraryStore>
LibraryStore::list(&self) -> Result<LibraryListing>
LibraryStore::add(&self, path: &Path) -> Result<LibraryEntry>
LibraryStore::scan(&self, root: &Path) -> Result<ScanResult>
LibraryStore::update(&self, id: &str, patch: LibraryPatch) -> Result<LibraryEntry>
LibraryStore::relocate(&self, id: &str, path: &Path) -> Result<LibraryEntry>
LibraryStore::remove(&self, id: &str) -> Result<LibraryEntry>
LibraryStore::resolve(&self, id: &str) -> Result<PathBuf>
LibraryStore::touch_opened(&self, id: &str) -> Result<LibraryEntry>
ship_catalog::canonical_ship_key(input: &str) -> String
ShipCatalog::load(home: &Path, game: Option<&GameInstall>) -> Arc<ShipCatalog>
ShipCatalog::lookup(&self, input: &str) -> ShipMetadata
```

`LibraryEntry` has `id`, `path: PathBuf`, `display_name`, `tags: Vec<String>`, `favorite`, `status: LibraryStatus`, `added_unix: u64`, and `opened_unix: Option<u64>`. Status serializes to `working`, `review`, `complete`, or `archived`. `LibraryPatch` has optional `display_name`, `tags`, `favorite`, and `status`; tags replace the current set after trimming/deduplication. Repeated `--tag` supplies this set; `--tag ""` clears it.

`LibraryListing` has `entries: Vec<LibraryEntryView>`, `source: PathBuf`, and `warnings`. Each view flattens its `entry` fields and adds `project: Option<ProjectMetadata>`, `available`, and per-entry `warnings`. Project metadata has `name`, `version`, `ship`, `ship_key`, `game_build`, and `mode`. Invalid or inaccessible projects remain in the catalog and are marked unavailable. Listing reads only mod.toml, never opens PreviewState, and does not traverse assets.

`ScanResult` has `entries`, `warnings`, `visited_dirs`, and `truncated`. Scan rejects a whole drive root, visits at most 10,000 directories to depth 32, collects at most 1,000 projects, stops below every discovered mod.toml, and skips assets/out/target/node_modules/.git/.wowsmod/.install and linked/reparse directories. Individual candidate failures do not discard valid projects.

`ShipMetadata` has `key`, `known`, `level`, `nation`, `species`, `name_local`, `param`, and `warning`. Unknown or empty ship inputs return `known: false` and nullable attributes. SDK/parameter aliases are normalized and lookups are case insensitive. Public `ShipCatalog.source: MetadataSource` has `kind` (`game`, `cache`, `last_cache`, `unavailable`), `root`, `build`, `cache_path`, `stale`, and `warnings`.

Ship data uses the existing GameIndex, PkgReader, GameParams and MO readers. A process-shared cache is keyed by canonical client root/build and tool home; disk caches also encode canonical root/build. The recent successful cache is available with explicit `last_cache`, `stale: true` provenance when the client cannot load. Parsing/localization warnings survive cache round trips. Metadata cache files are stored only under tool home.

The store persists `{ "schema_version": 1, "entries": [...] }` to `<home>/library.json`. Every write takes `<home>/library.lock`, reloads the latest document, applies the change and writes a flushed same-directory temporary file. Windows uses MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH), retaining the old catalog if replacement fails. No remove-before-rename gap is introduced. Corrupt, duplicate-identity, and unsupported-schema catalogs are rejected without overwriting their bytes. A lock abandoned by a crashed process requires explicit inspection/manual removal; live locks are never evicted by age.

CLI wiring: `library add <path>`, `scan <root>`, `list`, `remove <id>`, `relocate <id> <path>`, and `update <id> [--name ... --status ... --favorite true|false --tag ...]`. Every JSON success/error contains `ok`; list adds metadata/source provenance and warnings. Task 3 can compose installation status through its controller without duplicating storage logic.

## Actual JSON evidence

`out/task1/cli-list.json` is the complete successful list response from the release executable. Its first entry is:

```json
{
  "added_unix": 1794573817,
  "available": true,
  "display_name": "My Display Name",
  "favorite": false,
  "id": "project-7dc57ea3ff058dd98bcd6d38",
  "opened_unix": null,
  "path": "\\\\?\\C:\\Users\\Administrator\\AppData\\Local\\Temp\\wowsmod-library-cli-9wor4d9r\\projects\\a",
  "project": {"game_build":"","mode":"loose","name":"same","ship":"FSB510_Bourgogne","ship_key":"PFSB510","version":"0.1.0"},
  "ship_metadata": {"key":"PFSB510","known":true,"level":10,"name_local":"Bourgogne","nation":"France","param":"PFSB510_Bourgogne","species":"Battleship","warning":null},
  "status": "archived",
  "tags": ["paint", "review"],
  "warnings": []
}
```

That response had `ok: true`, `schema_version: 1`, `warnings: []`, `metadata_warnings: []`, and this exact metadata source:

```json
{
  "build": "8871067",
  "cache_path": "C:\\Users\\ADMINI~1\\AppData\\Local\\Temp\\wowsmod-library-cli-9wor4d9r\\home\\ship_metadata\\6eebf7fb1260bb5c4bec388b.json",
  "kind": "game",
  "root": "\\\\?\\E:\\Korabli",
  "stale": false,
  "warnings": []
}
```

The client above was read only. All fixture projects, catalog writes, metadata cache writes and simulated relocations were inside a temporary directory. No installation command was invoked.

## RED / GREEN and validation

- `cargo test -p wowsmod library::tests --features oozextract` first produced 8 expected unimplemented/missing-validation failures plus one junction fixture setup failure; the fixture used an incompatible slash path for mklink and was corrected. Evidence: `out/task1/red-library.log`. The cycle-specific rerun initially hit the other worker's temporary `switch::guard_path` compile dependency; this was not counted as a catalog regression (`red-cycle.log`).
- `cargo test -p wowsmod ship_catalog::tests --features oozextract` then produced 4 expected unimplemented-feature failures after the shared compile dependency was stable (`red-ship.log`).
- A separate cache-warning regression was observed RED before preserving warnings (`red-cache-warnings.log`), then passed with all ship tests.
- Catalog GREEN: **12 passed**, including canonical and Windows junction/case dedupe, distinct same-name projects, ID-preserving relocation, metadata/archiving/removal and byte preservation, corrupt/future schema preservation, stale handle reloads, scan pruning/cycles/depth, an exclusively opened unreadable mod.toml, and eight simultaneous writer threads. `out/task1/green-library-extended.log`.
- Ship GREEN: **4 passed**, including alias normalization, case-insensitive known lookup, explicit unknown/empty metadata, no-client no-home-write behavior, stale cache provenance and cached warning retention. `out/task1/green-ship.log`.
- `cargo build -p wowsmod --release --features oozextract` succeeded (`build-release-final.log`), and release `--version` returned `wowsmod 0.1.0` (`release-version.log`).
- `python out/task1/cli-smoke.py` succeeded: **21 CLI calls**, including all six commands, both true/false favorite values, repeated tags, idempotent add, same-name distinct projects, corrupt/future schema error JSON with unchanged bytes, and **8 independent processes** concurrently registering projects. `out/task1/cli-smoke.log` and `cli-results.json`.
- `source-tree-hashes.json` records identical before/after SHA-256 values for source mod.toml and asset files through add/list/update/remove and logical relocation. The fixture rename was performed by the smoke script, not by the library relocate command.
- The complete workspace suite is coordinated by root after all workers integrate; this report does not claim a fresh whole-workspace result. Existing unrelated warnings remain. `git diff --check` was clean for the working tree at report time.

## Self-review and limitations

Confirmed that all catalog mutations reload under the lock, same-name projects are never merged, relocation changes only the recorded path, add after a previous relocation cannot reuse an occupied deterministic ID, and removal never deletes project content. Metadata is separate from catalog persistence and installation registry state. Missing/corrupt projects can still be listed and removed. No PreviewState constructor or project mutation helper is called by list/add/scan/remove.

The current **dev** Windows executable fails before parameter parsing with stack overflow, including `--version`; its PE stack reservation is 1 MiB. A diagnostic copy changed only to a 16 MiB PE reservation starts successfully (`stack-probe.log`). The existing original-workspace release exe and the newly built release exe both start with the default 1 MiB reservation. This establishes a dev-stack limit but does not prove whether the baseline dev build already had it. No unrelated main entrypoint/stack policy was changed. Release CLI functionality is verified above.

Scan intentionally excludes linked directories even if they point inside root. It reports limits rather than scanning beyond the defined directory/depth/project bounds. Cache fallbacks explicitly warn that recorded root/build may be stale. No client assets, installation state, or source metadata are rewritten by this task.
