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
  /** 项目 fx.json(特效主题; 无则 null)。preview 段 = 工作台怎么画 */
  fx?: { file: string; name?: string; preview?: import('../engine/viewer').FxTheme } | null;
}
export interface InstallState { state: 'not_installed' | 'installed' | 'outdated'; rev: Rev | null; files: number; time: string | null }
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
export interface Job { id: string; kind: 'install' | 'uninstall'; state: 'running' | 'done' | 'failed'; progress: number; message: string; error?: { code: string; message: string } }
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
