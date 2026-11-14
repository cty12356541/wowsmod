# Local Mod library API v1

This contract extends the existing single-project preview. The server binds to
loopback only. Library registration changes its catalog, not project assets.

## Routing

- `preview <path>` keeps the direct workbench; `preview --library` starts at the library.
- `GET /api/v1/health` includes `library_mode: boolean`.
- `/?library=1` explicitly opens the library in either mode.
- `/?project=<id>` opens that registered project. Switching projects uses a full navigation.
- Library calls use `/api/v1/library`. Workbench calls use `/api/v1/projects/<id>`
  when the query contains a project ID; otherwise existing `/api/v1` routes remain valid.
- Geometry, blob and job URLs returned for a project include the same project prefix.

## Library summary

`GET /api/v1/library` returns `{ok, projects, ships, facets, game, warnings}`.
The UI can derive groups and facet counts from `projects`; no heavy preview state
is constructed for this request.

Each `projects` entry contains the catalog fields `id`, `path`, `display_name`,
`tags`, `favorite`, `status`, `added_unix`, `opened_unix`, plus:

- `available: boolean`, `warnings: string[]`.
- `project`: nullable `{name, version, ship, ship_key, game_build, mode}`.
- `ship_metadata`: `{key, known, level, nation, species, name_local, param, warning}`.
  Unknown metadata uses null values. A missing ship key represents a global project.
- `install`: `{state, rev, files, time, ...}`. State is `not_installed`, `installed`,
  `outdated` or `unknown`; it describes the selected client's actual registry and
  source identity, separately from the catalog's production status. Revision is
  nullable and never inferred to equal the current head merely from a name match.

`game` contains `{available, path, build, version, running}`. Missing client values
are null or empty; `available` is false. Metadata cache warnings remain visible.
`ships` and `facets` are convenience summaries, not a separate source of identity.

## Catalog mutations

All requests are POST with `Content-Type: application/json`. Browser origins must
match the local server origin. Errors use `{ok:false,error:{code,message}}`.

- `/api/v1/library/add`: `{path}` → `{ok,entry}`.
- `/api/v1/library/scan`: `{root}` → `{ok,entries,warnings,visited_dirs,truncated}`.
- `/api/v1/library/projects/<id>/update`: optional `display_name`, `tags`, `favorite`,
  `status` → `{ok,entry}`. Status is `working`, `review`, `complete` or `archived`.
- `/api/v1/library/projects/<id>/relocate`: `{path}` → `{ok,entry}`; preserves ID.
- `/api/v1/library/projects/<id>/remove`: `{}` → `{ok,entry}`; unregisters only.
- `/api/v1/library/projects/<id>/open`: `{}` → `{ok,url}` with `/?project=<id>`.

Unknown IDs return 404. Missing/invalid projects remain listed; opening their
workbench returns an explicit error. Registration/opening never installs a Mod.

## Default-view covers

`GET /api/v1/library/projects/<id>/thumbnail` returns
`{ok,state,url,stale,reason,width,height}`. The state is `queued`, `rendering`,
`ready`, `failed`, or `missing`; `url` and `reason` may be null. A previous cover
may remain available while `stale` is true. Visible cards request covers lazily;
the library summary does not wait for rendering. Background jobs use the
registered project and the selected client's resources, serially.

`POST /api/v1/library/projects/<id>/thumbnail/refresh` accepts `{}` and returns
the same status shape. It retries generation without discarding the last usable
cover. It uses the same origin and JSON checks as other catalog mutations.

The root-relative `url` addresses
`GET /api/v1/library/projects/<id>/thumbnail/<sha256>.png`. Clients must not rewrite
it to the workbench's project API prefix. Covers are 640 × 360 PNGs using the
workbench's `deck` camera, current project revision, day lighting and fixed LOD.
The cache belongs to the application's data directory; it is separate from the
project's assets and user render exports. Registration, removal, relocation and
source changes are checked so one variant cannot inherit another one's cover.
Missing browser/model data or rendering errors leave a usable placeholder and
an explicit retry action.

For configured external geometry inputs, changing the source without rebuilding
the preview geometry must not promote an old model image to a fresh cover. The
server retains the previous image and reports that the model needs rebuilding.
An in-place client upgrade requires restarting the preview service before new
covers are generated. These checks do not install or modify game resources.

## Workbench capabilities and application

The existing `project` view adds `capabilities` with boolean `library`, `preview`,
`apply`, `undo`, `restore`, `simulation` and a `reasons` object keyed by capability.
Unavailable model/simulation data does not prevent project metadata from loading.
Unsupported actions are disabled with the corresponding explanation.

`POST <project-prefix>/apply` requires `{dry_run: true|false}`. A dry run returns
the preflight result immediately. The result uses the transaction service's
`previous` array, `selected`, `target_client`, `build`/`target_build`, `res_mods`,
`files`, `warnings` and `dry_run`. The UI displays the current→selected projects
and target client/build before an explicit second request with `dry_run:false`.
The real request may return the result immediately or `{ok,job_id}`; polling
`<project-prefix>/jobs/<job_id>` then returns `{state,message,result|error}`.
Failed jobs provide `error.code` and a human-readable string `error.message`;
structured diagnostic context, when present, is kept separately from the message.
Selecting/opening a project never triggers either request.

`/uninstall` dispatches uninstall exactly. Child processes inherit the selected
client, including an explicitly unavailable path; they must not auto-select a
different client or follow a project manifest that names another installation.
Existing render and warm-render routes remain compatible.

## Source identity and user verification

`GET /api/v1/library/projects/:id/source-revision` returns
`{ok,project_id,revision}`. Content identity includes ordinary project source files,
`mod.toml`, assets, edits, FX and design files; root-level build/cache/install
directories are excluded. It hashes content rather than relying on timestamps.

`GET .../:id/compatibility` returns current source revision, project and target
builds, actual install state, historical observations and `matching_record_id`.
`POST .../:id/verifications` accepts
`{expected_revision,build,outcome,scene,note,evidence}`, where outcome is
`passed|issues|failed`. The server rejects a changed source with
409 `source_changed`. Evidence is descriptive text; the server does not fetch it.
Only the latest appended observation with matching project/revision/build counts
as current. Cards show historical records without claiming uncomputed validity.

## Selective scanning

`POST /api/v1/library/scan/preview {root}` discovers without registering. It
returns `scan_id`, candidates with `new|registered|invalid` status, warnings,
truncation and expiry. `POST /api/v1/library/scan/import
{scan_id,candidate_ids}` registers only the selected candidates. Tokens expire
after ten minutes; at most sixteen scans are retained. Any selected candidate
whose directory identity or manifest changed makes the entire batch fail with
409 `scan_changed`. Concurrent existing registrations are returned without
duplicates. The original `/scan` endpoint retains its register-all behavior.

## Comparing independent variants

`POST /api/v1/library/comparisons {left_id,right_id,refresh?}` queues an ordered
pair of distinct registered variants of the same ship. `GET
/api/v1/library/comparisons/:id` returns `queued|rendering|ready|failed|missing`,
both project IDs, revisions and image URLs, common camera, recipe, stale flag
and reason. Request/refresh verifies content in a bounded background queue;
status polling does not repeatedly hash project assets.

Both final images share exact numerical camera position, target, up, FOV,
clipping planes and aspect. The recipe is 640×360, DPR 1, high quality,
day/studio, LOD 0, current revision and time zero. Initial library covers are
explicitly unsynchronized placeholders. The pair becomes ready only after both
images and their metadata validate and the descriptor is atomically published.
Source/build/external-resource/geometry/frontend changes invalidate cached pairs.
Image routes are `/comparisons/:id/:side/:hash.png`.

## Checkpoints and recovery

`GET /api/v1/library/projects/:id/checkpoints` returns saved checkpoints and
source scope. `POST` accepts `{label,expected_revision}` and captures a verified
content-addressed source snapshot. Frozen preview generation is asynchronous;
preview failure leaves the saved source recoverable. The image route is
`.../:id/checkpoints/:checkpoint/preview/:hash.png`.

`GET .../:id/checkpoints/:checkpoint/diff` reports files added, removed and
changed relative to the checkpoint. `POST .../:checkpoint/restore
{destination,expected_revision}` materializes and verifies a new directory,
then registers a new variant. Existing destinations, source/game/store
directories and reparse paths are rejected. Recovery never restores installation
state or overwrites the source. Missing original source still allows recovery.
If registration fails after successful materialization, the result retains
`ok:true`, `entry:null`, `restored_path` and actionable warnings; the completed
directory can be registered later. `expected_revision` identifies the checkpoint
being restored, not the current mutable source.
