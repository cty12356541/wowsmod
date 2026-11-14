// API v1 的数据类型（见 API.md）。只声明前端用到的字段。
export type Rev = string;
export interface TexRef { name: string; hash?: string; url?: string; width?: number; height?: number; missing?: boolean }
export interface MaterialDef { family: 'pbs' | 'emissive' | 'wire' | 'alpha' | 'glass'; fx?: string; params: Record<string, number>; textures: Partial<Record<'a' | 'n' | 'mg' | 'ao', TexRef>> }
export interface Manifest {
  rev: Rev; geometry: { hash: string; url: string; bytes: number };
  waterline_y: number; camo?: { color0: [number, number, number] }; materials: Record<string, MaterialDef>;
}
export interface Project {
  api_version: number; server: { name: string; version: string; mock?: boolean };
  project: { name: string; version: string; description?: string; dir: string };
  ship: { code: string; param: string; index: string; hull_model: string; name: Record<string, string>; level: number; nation: string; species: string };
  game: { client: string; version: string; build: string; path: string; running: boolean };
  install: InstallState; activity: Activity; head: Rev | null; etag: number;
  capabilities?: Capabilities;
  /** 项目 fx.json(特效主题; 无则 null)。preview 段 = 工作台怎么画 */
  fx?: { file: string; name?: string; preview?: import('../engine/viewer').FxTheme } | null;
}
export interface InstallState { state: 'not_installed' | 'installed' | 'outdated' | 'unknown'; rev: Rev | null; files: number; time: string | null }
export type Capability = 'library' | 'preview' | 'apply' | 'undo' | 'restore' | 'simulation';
export type Capabilities = Record<Capability, boolean> & { reasons: Partial<Record<Capability, string>> };
export type LibraryStatus = 'working' | 'review' | 'complete' | 'archived';
export interface LibraryEntry {
  id: string; path: string; display_name: string; tags: string[]; favorite: boolean; status: LibraryStatus;
  added_unix: number; opened_unix: number | null; modified_unix?: number | null; available: boolean; warnings: string[];
  project: null | { name: string; version: string; ship: string; ship_key: string; game_build: string; mode: string };
  ship_metadata: { key: string; known: boolean; level: number | null; nation: string | null; species: string | null; name_local: string | null; param: string | null; warning?: string | null };
  install: InstallState;
  verification?: VerificationRecord | null;
}
export type VerificationOutcome = 'passed' | 'issues' | 'failed';
export interface VerificationRecord {
  id: string; project_id: string; revision: string; build: string; outcome: VerificationOutcome;
  scene: string; note: string; evidence: string[]; recorded_unix: number;
}
export interface VerificationRequest {
  expected_revision: string; build: string; outcome: VerificationOutcome; scene: string; note: string; evidence: string[];
}
export interface CompatibilityInfo {
  ok: boolean; project_id: string; project_build: string | null; target_build: string | null;
  current_revision: string | null; install: InstallState; records: VerificationRecord[];
  matching_record_id: string | null; warnings: string[];
}
export interface ScanCandidate {
  candidate_id: string; path: string; project: LibraryEntry['project'];
  status: 'new' | 'registered' | 'invalid'; existing_id: string | null; warning: string | null;
}
export interface ScanPreview {
  ok: boolean; scan_id: string; root: string; candidates: ScanCandidate[];
  warnings: string[]; visited_dirs: number; truncated: boolean; expires_unix: number;
}
export interface ScanImportResult { ok: boolean; entries: unknown[]; already_registered: unknown[]; warnings: string[] }
export interface ComparisonStatus {
  ok: boolean; id: string; state: 'queued' | 'rendering' | 'ready' | 'failed' | 'missing';
  left: { project_id: string; revision: string | null; url: string | null };
  right: { project_id: string; revision: string | null; url: string | null };
  camera: import('../render/camera').ExactCamera | null; recipe: unknown; stale: boolean; reason: string | null;
}
export interface Checkpoint {
  id: string; label: string; created_unix: number; revision: string; scope_version: number;
  file_count: number; total_bytes: number; project_build: string; target_build: string | null;
  preview: { state: 'pending' | 'ready' | 'failed'; url: string | null; reason: string | null };
}
export interface CheckpointList { ok: boolean; checkpoints: Checkpoint[]; scope_version: number; excluded_roots: string[] }
export interface CheckpointDiff { ok: boolean; checkpoint_revision: string; current_revision: string; added: string[]; removed: string[]; changed: string[]; unchanged_count: number }
export interface CheckpointRestore { ok: boolean; entry: { id: string; path: string } | null; checkpoint_id: string; revision: string; url: string | null; warnings: string[]; restored_path?: string }
export interface LibrarySummary {
  ok: boolean; projects: LibraryEntry[]; ships: unknown[]; facets: unknown; warnings: string[];
  game: { available: boolean; path: string | null; build: string | null; version: string | null; running: boolean };
}
export interface LibraryThumbnailStatus {
  ok: true;
  state: 'queued' | 'rendering' | 'ready' | 'failed' | 'missing';
  url: string | null;
  stale: boolean;
  reason: string | null;
  width: number;
  height: number;
}
export interface ApplyPlan {
  previous: AppliedProject[]; selected: AppliedProject; target_client?: string; build?: string | number; target_build?: string | number;
  res_mods?: string; files?: number | unknown[]; warnings?: string[]; dry_run?: boolean;
}
export interface AppliedProject { name: string; version?: string; ship?: string; mode?: string; project_dir?: string; game_build?: string; files?: string[] }
export interface Activity { state: 'idle' | 'working'; title?: string | null; since?: string | null; step?: string | null; user_prompt?: string | null }
export interface EditView { file: string; cmd: string; time?: string; command_line: string; outputs?: { path: string }[]; warnings?: string[]; stats?: unknown }
export interface Step {
  id: Rev; index: number; state: 'active' | 'reverted'; title: string; user_prompt?: string | null; ai_note?: string | null;
  time: string; time_unix: number; edits: EditView[]; files: { path: string; change: string }[];
  textures: string[]; materials: string[]; parts: string[]; files_summary?: { imported?: number; stubs?: number };
}
export interface Part {
  id: string; group: 'hull' | 'main' | 'secondary' | 'aa' | 'director' | 'radar'; label: string; section?: string; hp?: string; model?: string;
  module?: string; caliber_mm?: number | null; barrels?: number | null; verts: number; tris: number; materials: string[]; source?: string; changed?: boolean;
  lods?: { lod: number; extent: number; verts: number; tris: number }[]; bones?: string[];
}
export interface TexInfo {
  name: string; slot: string; width: number; height: number; source?: string; hash: string; base_hash: string | null; url: string; base_url: string | null;
  changed: boolean; changed_in: Rev[]; materials: string[]; parts: number;
}
export interface Job { id: string; kind: 'install' | 'uninstall' | 'apply'; state: 'running' | 'done' | 'failed'; progress: number; message: string; result?: ApplyPlan; error?: { code: string; message: string } }
export interface MountSpec {
  group: 'main' | 'secondary' | 'aa'; model: string; bones: string[]; muzzles: number; numBarrels?: number; barrelDiameter?: number;
  rotationSpeed?: [number, number] | null; horizSector?: [number, number] | null; vertSector?: [number, number] | null;
  deadZone?: [number, number][]; pitchDeadZones?: unknown[]; shotDelay?: number; smallGun?: boolean | null; maxDist?: number | null;
}
export interface SimData {
  ship: { maxSpeed_kn: number; speedCoef?: number; rudderTime_s: number; turningRadius_m: number; maxRudderAngle_deg?: number; size_m: [number, number, number];
    draft_m?: number; tonnage_t?: number; rollEffect?: number; forwardEngineUpTime_s?: number; backwardEngineUpTime_s?: number; backwardPowerCoef?: number };
  mounts: Record<string, MountSpec>;
}
