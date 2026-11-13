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
