import type { Viewer } from '../engine/viewer';
import type { RenderParams } from './render';
export interface ExactCamera { pos: number[]; tgt: number[]; up: number[]; fov: number; near: number; far: number; aspect?: number }
const bad = (message: string): never => { throw Object.assign(new Error(message), { code: 'bad_param' }); };
function vector(value: string | undefined, label: string): number[] {
  const parts = value?.split(',');
  if (!parts || parts.length !== 3 || parts.some((part) => !part.trim() || !Number.isFinite(Number(part)))) return bad(`${label}= must contain three finite numbers`);
  return parts.map(Number);
}
export function explicitCamera(params: RenderParams): ExactCamera | null {
  if (params.pos === undefined) return null;
  if (params.focus !== undefined || params.dist !== undefined || params.az !== undefined || params.el !== undefined) return bad('pos cannot be combined with focus, dist, az or el');
  const pos = vector(params.pos, 'pos'); const tgt = vector(params.tgt, 'tgt');
  if (pos.every((value, index) => value === tgt[index])) return bad('pos and tgt must differ');
  const fov = params.fov ?? 34; const near = params.near ?? .05; const far = params.far ?? 1600;
  if (![fov, near, far].every(Number.isFinite) || fov <= 5 || fov >= 120 || near <= 0 || far <= near) return bad('Invalid fov, near or far');
  return { pos, tgt, up: [0, 1, 0], fov, near, far };
}
export function applyExplicitCamera(viewer: Viewer, exact: ExactCamera): void {
  viewer.baseFov = exact.fov;
  viewer.setView(exact.pos, exact.tgt);
  // OrbitControls may clamp or round through spherical coordinates. The deterministic
  // render performs no advance/update after this assignment, so its supplied pose wins.
  viewer.camera.position.fromArray(exact.pos); viewer.controls.target.fromArray(exact.tgt);
  viewer.camera.up.fromArray(exact.up); viewer.camera.lookAt(viewer.controls.target);
  viewer.camera.fov = exact.fov; viewer.camera.near = exact.near; viewer.camera.far = exact.far;
  viewer.camera.updateProjectionMatrix(); viewer.camera.updateMatrixWorld(true);
}
export function renderCameraMeta(viewer: Viewer): { camera: ExactCamera; bounds: {min:number[];max:number[]} } {
  const c = viewer.camera;
  return { camera: { pos: c.position.toArray(), tgt: viewer.controls.target.toArray(), up: c.up.toArray(), fov: c.fov, near: c.near, far: c.far, aspect: c.aspect }, bounds: { min: viewer.BOX.min.toArray(), max: viewer.BOX.max.toArray() } };
}
