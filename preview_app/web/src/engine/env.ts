import GL from './gamelook.json';
// 环境（影棚 / 清水）× 光照（白天 / 黄昏 / 夜间）。暗色影棚 = 影棚 + 夜间。
export type EnvName = 'studio' | 'water';
export type LightName = 'day' | 'dusk' | 'night';
export interface Palette {
  zenith: string; horizon: string; below: string; floor: string; grid: string; sun: string; sunI: number; sunDir: [number, number, number]; sunDisk: number;
  hemiSky: string; hemiGround: string; hemi: number; envI: number; panel: number; exposure: number; shadow: number; sheet: string; vig: number;
  deep?: string; shallow?: string; tint?: string; foam: string; bloomT: number; bloomS: number; wl: string;
}
const studioDay: Palette = { zenith: '#d9e2e9', horizon: '#f3f6f8', below: '#e8edf1', floor: '#edf1f4', grid: '#cfd8df', sun: '#fffaf2', sunI: 2.3, sunDir: [0.5, 0.78, -0.36], sunDisk: 0,
  hemiSky: '#eef3f7', hemiGround: '#c9d2d8', hemi: 0.35, envI: 0.7, panel: 3, exposure: 0.95, shadow: 0.2, sheet: '#3a9fb5', vig: 0.06, foam: '#ffffff', bloomT: 0.95, bloomS: 0.35, wl: '#2a93ad' };
export const PALETTES: Record<string, Palette> = {
  studio_day: studioDay,
  studio_dusk: { ...studioDay, zenith: '#9aa6b8', horizon: '#e8c4a2', below: '#cbbcb0', floor: '#ddd2c8', grid: '#c6b9ad', sun: '#ffb877', sunI: 1.7, sunDir: [0.86, 0.2, -0.47],
    hemiSky: '#b9c2d2', hemiGround: '#a8988a', hemi: 0.26, envI: 0.45, panel: 1.3, exposure: 1.0, shadow: 0.26, vig: 0.12, bloomT: 0.9, bloomS: 0.4 },
  studio_night: { ...studioDay, zenith: '#0c1219', horizon: '#18212b', below: '#141c25', floor: '#151e28', grid: '#223040', sun: '#c2d2e8', sunI: 0.8, sunDir: [-0.35, 0.85, 0.4],
    hemiSky: '#9fb3c8', hemiGround: '#20303f', hemi: 0.28, envI: 0.3, panel: 0.35, exposure: 1.1, shadow: 0.38, sheet: '#4fb9cf', vig: 0.28, bloomT: 0.22, bloomS: 0.9, wl: '#4fc0d6' },
  water_day: { ...studioDay, zenith: '#c4d9e7', horizon: '#eef4f7', below: '#7fbccb', floor: '#7fbccb', sun: '#fff6ea', sunI: 2.2, sunDir: [0.55, 0.75, -0.38], hemiGround: '#6aa9b6',
    shadow: 0, deep: '#2f8ea3', shallow: '#9fd3dc', tint: '#4aa3b4' },
  water_dusk: { ...studioDay, zenith: '#7488a6', horizon: '#efb98c', below: '#3c6878', floor: '#3c6878', sun: '#ffab66', sunI: 1.6, sunDir: [0.86, 0.16, -0.48], sunDisk: 1,
    hemiSky: '#aab6c8', hemiGround: '#4b6a72', hemi: 0.24, envI: 0.45, panel: 1.2, exposure: 1.0, shadow: 0, vig: 0.12, deep: '#1f5a6e', shallow: '#5d8f98', tint: '#3d7c8a', foam: '#f4e6da', bloomT: 0.9, bloomS: 0.4 },
  water_night: { ...studioDay, zenith: '#0a1018', horizon: '#1a2633', below: '#0d1d26', floor: '#0d1d26', sun: '#b8c8e0', sunI: 0.6, sunDir: [-0.35, 0.8, 0.45], sunDisk: 0.4,
    hemiSky: '#8fa4bb', hemiGround: '#14242e', hemi: 0.22, envI: 0.28, panel: 0.3, exposure: 1.1, shadow: 0, vig: 0.28, deep: '#0b2430', shallow: '#1d4651', tint: '#1c4d5a', foam: '#b9c6d2', bloomT: 0.22, bloomS: 0.9, wl: '#4fc0d6' },
};
export const palette = (env: EnvName, light: LightName) => PALETTES[env + '_' + light];

// ---------- 游戏对齐(港口)外观 ----------
// look = 'game': 船体光照/色调按港口实拍拟合(gamelook.json, 由 fit_game_look.py 生成); 背景仍按 env(影棚浅色 / 清水)。
// look = 'studio': 原影棚展台光照(好看、中性, 但与港口不一致)。
export type LookName = 'studio' | 'game';
export interface GameLook {
  sunDir: [number, number, number]; sun: string; sunI: number; hemiSky: string; hemiGround: string; hemi: number; envI: number;
  zenith: string; horizon: string; below: string; exposure: number;
  /** 船体响应(港口对暗色的非线性提亮/金发黄绿由此复现): x = max(辐亮度·gain − toe, 0); y = min(tmS·x^tmP/(x^tmP+1), 1); 再按 sat 调饱和度 */
  gain: [number, number, number]; toe: [number, number, number]; tmP: number; sat: number; /** 曲线肩部放大(>1 时亮部提前到顶, 金色因 R 先饱和而偏黄) */ tmS?: number;
  /** 拟合用: >0 时船体直接输出 辐亮度×raw(不做色调映射) */
  raw?: number;
  fogCol: string; fogDen: number; emisK: number; bloomT: number; bloomS: number; mgSwap: number;
  deep: string; shallow: string; tint: string; shadow: number;
  /** 近似 AO(0..1): 朝下/掠射压暗, 补港口 SSAO/檐下(外带件平涂). 白天 ~0.55 */
  contactAO?: number;
  /** 水面不透明度倍率(1=原版 mix(0.10,0.62); 1.5≈港口更浑) */
  waterOpa?: number;
  /** 金色色相补偿(度→viewer/25): 正=橙金→黄绿橄榄 */
  goldHue?: number;
  /** peri/royal 色相补偿(度→viewer/30): 正=青紫→品红(港口深紫更红) */
  periHue?: number;
}
export const GAME_LOOK: { version: number; day: GameLook; night: GameLook } = GL as any;
export const gameLook = (light: LightName): GameLook => (light === 'night' ? GAME_LOOK.night : GAME_LOOK.day);
