/* 界面文案：中文 / English / Русский（三种语言完整，缺键会在开发时报警）。
 * 值是字符串（{name} 占位）或函数（复数/语序）。用户与 AI 写的步骤标题、说明按原文显示，不翻译。 */
import type { Part } from './types';

export type Lang = 'zh' | 'en' | 'ru';
type V = string | ((p: Record<string, any>) => string);
type Dict = Record<string, V>;

const ruPl = (n: number, one: string, few: string, many: string) => { const a = Math.abs(n) % 100, b = a % 10; return a > 10 && a < 20 ? many : b > 1 && b < 5 ? few : b === 1 ? one : many; };
const enPl = (n: number, one: string, many: string) => (n === 1 ? one : many);

const zh: Dict = {
  appName: 'wowsmod 预览', docTitle: '{p} · wowsmod 预览', language: '界面语言', more: '更多', ok: '知道了', cancel: '取消', close: '关闭', retry: '重试', auto: '自动',
  loadingShort: '加载中…', tierShort: '{n} 级', original: '原版', stepN: '第 {n} 步',
  species_Battleship: '战列舰', species_Cruiser: '巡洋舰', species_Destroyer: '驱逐舰', species_AirCarrier: '航母', species_Submarine: '潜艇',
  workspace: '工作区', wsReview: '审阅', wsSim: '模拟', wsReviewTip: '对比改动、看贴图、回看与撤回步骤', wsSimTip: '按游戏数据航行、转炮、开火，检查发光动画与 LOD', simUnavailable: '后端没有提供模拟数据（/api/v1/sim）', simUnavailableDesc: '升级 wowsmod 后端，或在 mock 里生成 sim.json。',
  stWorking: 'AI 正在修改…', stOffline: '连接已断开', gameRunning: '游戏运行中', gameRunningHint: '游戏客户端正在运行：安装与卸载需要先退出游戏。',
  install: '装进游戏', installed: '已装进游戏', installUpdate: '更新游戏里的版本', installing: '正在安装', uninstalling: '正在卸载', reinstall: '重新装进游戏', uninstall: '从游戏卸载',
  notInstalled: '这个 mod 目前没有装进游戏。', installInfo: '游戏里是第 {n} 步的结果 · {files} 个文件', installOutdated: '之后又有了新的改动，游戏里还是旧版本。',
  installedTitle: '已装进游戏', uninstalledTitle: '已从游戏卸载', jobFailed: '操作没有完成', opFailed: '操作没有完成',
  uninstallTitle: '从游戏卸载这个 mod？', uninstallDesc: '会删除 wowsmod 写进游戏 res_mods 的文件并还原备份。项目本身和时间线不受影响，之后可以随时再装进去。',
  gameRunningTitle: '游戏正在运行', gameRunningDesc: '请先退出游戏客户端，再装进或卸载 mod（游戏运行时会锁定文件）。',
  reconnected: '已重新连接', reconnectedDesc: '已同步断开期间的变化。', offlineBar: '与 wowsmod 的连接已断开，正在自动重连。显示的是断开前的内容，撤回和安装暂不可用。',
  connOnline: '已连接本地服务', connOffline: '本地服务未连接', connConnecting: '正在连接…', headIs: '最新：第 {n} 步', mockServer: '模拟后端', mockTip: '当前连的是 mock 服务（真实勃艮第数据，功能与 API.md 一致）',
  drawsN: '{n} 次绘制', idle: '空闲（不重绘）', quality: '画质', q_high: '高', q_mid: '中', q_low: '低',
  q_high_tip: '2× 像素密度、4× 抗锯齿、2048 阴影、Bloom、原尺寸贴图', q_mid_tip: '1× 像素密度、2× 抗锯齿、1024 阴影、Bloom、1024 贴图', q_low_tip: '关闭阴影、Bloom、抗锯齿与叠加对比，贴图 512，适合集成显卡',
  ldConnect: '正在连接 wowsmod…', ldProject: '正在读取项目…', ldModel: '正在下载模型', ldParse: '正在解析模型…', texLoading: '贴图 {d}/{n}', texFailTitle: '{n} 张贴图没有加载出来',
  fatalNetTitle: '连接不上 wowsmod 预览服务', fatalNetDesc: '请确认命令行里的 <code>wowsmod preview &lt;项目目录&gt;</code> 还在运行，然后点重试。',
  fatalNoModelTitle: '这个项目还没有可预览的模型', fatalNoModelDesc: '先用 wowsmod 导入一艘船（基线步骤），预览会自动出现。', fatalTitle: '预览出错了', ctxLost: '显卡驱动重置了 WebGL，请刷新页面。',
  newStep: 'AI 完成了第 {n} 步', revertedTo: '已撤回到第 {n} 步', restored: '已恢复', noReady: '没有可开火的炮塔（装填中或目标在射界外）',
  // 左栏
  outline: '大纲', textures: '贴图', nChanged: '{n} 改动', nOfChanged: '{c}/{n}', filterParts: '筛选部件', onlyChanged: '只看改动', showAll: '全部显示',
  noChangedParts: '和对比基准相比，没有部件被改动', noMatch: '没有匹配的部件', changedVsBase: '相对对比基准有改动', unchanged: '未改动', toggleVis: '显示 / 隐藏', focus: '定位（双击行也可以）', solo: '只显示这个', deselect: '取消选择',
  group_hull: '船体', group_main: '主炮', group_secondary: '副炮', group_aa: '防空炮', group_director: '火控指挥仪', group_radar: '雷达',
  short_main: '主炮', short_secondary: '副炮', short_aa: '防空', short_director: '指挥仪', short_radar: '雷达',
  sec_Bow: '艏段', sec_MidFront: '前中段', sec_MidBack: '后中段', sec_Stern: '艉段',
  barrels: (p) => ['', '单装', '双联', '三联', '四联'][p.n] || `${p.n} 联`, trisK: '{n}k 三角形',
  texChangedN: '改动 {n}', texAllN: '全部 {n}', noChangedTex: '和对比基准相比，没有贴图被改动', usedByN: '{n} 个部件', baseLabel: '对比基准', afterLabel: '改后', sameAsBase: '与基准相同',
  slotShort_a: '固有色', slotShort_n: '法线', slotShort_mg: '金属/光泽', slotShort_ao: 'AO',
  ch_a_rgb: '固有色（不含 alpha）', ch_a_a: 'alpha：栏杆/网格的镂空遮罩', ch_n_r: '法线 X', ch_n_g: '法线 Y',
  ch_mg_r: '光泽度（越白越光滑）', ch_mg_g: '金属度', ch_mg_b: '迷彩遮罩；发光材质里是发光遮罩（发光 = B² × 固有色 × 强度）', ch_ao_r: '环境光遮蔽',
  // 视口
  compare: '对比方式', single: '单视图', split: '分屏', side: '并排', overlay: '叠加', compareWith: '对比基准', baseOrig: '对比原版', basePrev: '对比上一步', opacity: '叠加透明度',
  overlayLowQ: '低画质下不提供叠加（需要多画一遍整帧），请用分屏或并排', overlayLabel: '{a} 叠加 {p}%',
  channel: '通道', ch_final: '最终效果', ch_albedo: '固有色', ch_normal: '法线', ch_emissive: '发光', ch_wire: '线框', ch_mask: '遮罩',
  look: '外观', lookGame: '游戏内', lookGameTip: '按港口实拍校准的光照与色彩响应(默认; 定稿与验收用)', lookStudio: '展台', lookStudioTip: '影棚展台光照: 好看、中性, 但与游戏内颜色有差异',
  environment: '环境', studio: '影棚', studioTip: '中性灰影棚，看颜色最准', ocean: '清水', oceanTip: '海面 + 天空，三种水线模式', darkStudio: '暗色影棚',
  waterMode: '水线模式', wmFull: '整面', wmCut: '剖切', wmOff: '剥离', wmFullTip: '完整海面，水下部分被水遮住', wmCutTip: '朝向镜头的一侧剖开水面，看到水下船体', wmOffTip: '去掉水面，只留水线标记',
  lighting: '光照', lightDay: '白天', lightDusk: '黄昏（港口光照会压暗颜色）', lightNight: '夜晚（看发光）', light_dusk: '黄昏', light_night: '夜晚',
  views: '视角', v_hero: '默认', v_side: '侧视', v_bow: '前斜', v_stern: '后斜', v_top: '俯视', v_port: '港口视角', v_funnel: '烟囱特写', v_custom: '自定义', focusSel: '定位选中部件',
  display: '显示', highlight: '高亮改动', waterline: '水线', camo: '永久迷彩', camoTip: '按 _mg 的 B 通道叠加迷彩底色（游戏里的效果）', glowGain: '发光强度', glowGainTip: '只影响预览显示，不改 mod 文件', resetDisplay: '恢复默认',
  reviewing: '正在回看第 {n} 步 · 最新是第 {h} 步', latest: '回到最新', vpHint: '左键旋转 · 滚轮缩放 · 右键平移 · 单击选中部件',
  // 历史
  tabHistory: '历史', tabInspect: '检查器', tabSim: '模拟', noSteps: '还没有改动', noStepsDesc: '对 AI 说出想要的效果，或者直接运行 wowsmod 命令，每一步都会出现在这里。',
  current: '当前', reverted: '已撤回', baselineMeta: '基线 · 官方 SDK 模型', stepMeta: (p) => `${p.t} 张贴图 · ${p.p} 个部件`,
  details: '展开', collapse: '收起', viewThis: '查看', revertHere: '撤回到这里', revertConfirm: '之后的步骤会从项目里移除（仍可恢复）。撤回到第 {n} 步？', revertDo: '确认撤回', restore: '恢复',
  cmd_import_sdk: '导入 SDK 模型', cmd_tex_recolor: '贴图换色', cmd_tex_channel: '通道编辑', cmd_glow: '添加发光',
  // 检查器
  noSelection: '未选择部件', noSelectionDesc: '在视口或大纲里单击部件查看详情。', props: '属性', group: '分组', section: '船段', model: '模型', module: '模块', caliber: '口径',
  changeState: '状态', bones: '骨骼', bone_yaw: '旋转', bone_pitch: '俯仰', bone_recoil: '后坐', lodTable: 'LOD', lodSrc: '.model 链', lodExtent: '切换距离', verts: '顶点', tris: '三角形',
  materials: '材质', texturesUsed: '使用的贴图', project: '项目', projName: '名称', ship: '船', hullModel: '船体模型', game: '游戏', changedParts: '改动部件', changedTex: '改动贴图', projDir: '目录',
  src_exact: '游戏数据', src_approx: '近似', src_preview: '仅预览', srcGP: '按游戏数据精确：来自 GameParams', srcVisual: '按游戏数据精确：来自 .visual 节点树与蒙皮权重', srcGPtip: '按游戏数据精确：GameParams.{k}',
  // 模拟
  secSail: '航行', telegraph: '车钟', rudder: '舵', simReset: '复位', tel_rev: '倒', tel_stop: '停', tel_q1: '¼', tel_q2: '½', tel_q3: '¾', tel_full: '全',
  tel_rev_long: '倒车', tel_stop_long: '停车', tel_q1_long: '1/4 航速', tel_q2_long: '1/2 航速', tel_q3_long: '3/4 航速', tel_full_long: '全速',
  rud_m1: '左满', rud_m0_5: '左半', rud_0: '正舵', rud_0_5: '右半', rud_1: '右满',
  speed: '航速', kn: '节', heading: '航向', rudderAngle: '舵角', heel: '横倾', maxSpeed: '最高航速', rudderTime: '转舵时间', turnRadius: '回转半径', engineUp: '加速 / 倒车用时', sec: '秒', m: '米',
  heelWaves: '涌浪', waves: '涌浪强度', wakeFoam: '尾迹 / 白沫', approxHeelTip: '近似示意：横倾 ≈ 3.2° × 舵量 × (航速比)²，涌浪是正弦叠加；游戏不公开这部分参数',
  approxWakeTip: '近似示意：尾迹、艏浪、舷侧白沫是预览自绘的效果，不是游戏的粒子系统', approxNote: '航行观感按 GameParams 航速与舵效；时间比例、横倾、尾迹为近似示意', approxShort: '部分近似', hudHide: '收起航行信息', hudShow: '展开航行信息',
  secCamera: '镜头', cam_orbit: '跟随', cam_free: '自由', cam_game: '游戏第三人称', cam_orbit_tip: '相机跟着船走，可旋转缩放', cam_free_tip: '相机固定在世界里，船从镜头前驶过', cam_game_tip: '像游戏一样：镜头绕船，准星瞄准',
  secGuns: '火炮', combatTip: '切换港口 / 战斗状态（C）', statePort: '港口', stateCombat: '战斗', statePortTip: '炮塔归零（数据里没有发现港口/战斗的发光差异）', stateCombatTip: '炮塔跟随目标转动，可以开火',
  aimMouse: '鼠标瞄准', aimMouseHint: '移动指向，单击锁定', aimDist: '目标距离', approxElevTip: '近似示意：抬炮角按目标距离与最大射程线性估算（俯仰范围本身按游戏数据）',
  reloadReal: '真实装填', reloadRealTip: '按游戏数据精确：shotDelay；关掉时装填最多 2 秒，方便看效果',
  salvo: '齐射', single1: '单发', salvoTip: '所有就绪且在射界内的炮塔齐射', singleTip: '下一座就绪炮塔开火', mst_ready: '就绪', mst_rest: '归位', showLess: '收起', showAllCmds: '展开全部 {n} 条命令', mst_turning: '转动中', mst_blocked: '射界外',
  rotSpeed: '旋转速度', horizSector: '水平射界', vertSector: '俯仰范围', reloadTime: '装填', rotLimits: '转速 / 射界 / 俯仰', rotLimitsDesc: 'GameParams 原值；悬停炮塔看数值',
  fx: '炮口焰 / 硝烟 / 后坐', approxFxTip: '近似示意：后坐行程 ≈ 3 倍口径，焰与烟是预览自绘，不是游戏特效', fxDesc: '预览自绘', aaMounts: '防空炮', aaStatic: 'GameParams 没有转速数据，保持静止',
  secEmis: '发光动画', em_mod: '按 mod', em_mod_tip: '使用 mod 材质里的 emissionAnimationMode', em_0: '常亮', em_1: '脉冲', em_2: '时间轴', em_3: '滚动', em_2_tip: '时间轴模式需要 animMap，后端尚未提供，这里用阶梯近似',
  emSpeed: '速度', approxEmisTip: '近似示意：动画速度与幅度常数未从游戏确认', emFormula: '公式', emFormulaTip: '按游戏数据精确（M2_REPORT）：发光 = mg.B² × 固有色 × (emissivePower + animEmissionPower·a(t))',
  secLod: 'LOD 检查', distance: '距离', d_near: '近', d_mid: '中', d_far: '远', lodTint: 'LOD 着色', lodBias: '切换倍率', approxLodTip: '近似示意：游戏还会按画质设置缩放切换距离，这里用一个倍率模拟',
  hiddenShort: '隐藏', camDist: '相机距离', lodRule: '切换距离来自 .model 的 extent', hudLod: '船体 {h} · 主炮 {m}',
  legendExact: '= GameParams / SDK 原始数据', legendApprox: '= 预览自绘或估算',
};

const en: Dict = {
  appName: 'wowsmod preview', docTitle: '{p} · wowsmod preview', language: 'Language', more: 'More', ok: 'OK', cancel: 'Cancel', close: 'Close', retry: 'Retry', auto: 'Auto',
  loadingShort: 'Loading…', tierShort: 'Tier {n}', original: 'Original', stepN: 'Step {n}',
  species_Battleship: 'Battleship', species_Cruiser: 'Cruiser', species_Destroyer: 'Destroyer', species_AirCarrier: 'Carrier', species_Submarine: 'Submarine',
  workspace: 'Workspace', wsReview: 'Review', wsSim: 'Simulate', wsReviewTip: 'Compare changes, inspect textures, review and revert steps', wsSimTip: 'Sail, train and fire guns from game data; check glow animation and LODs', simUnavailable: 'The backend provides no simulation data (/api/v1/sim)', simUnavailableDesc: 'Update the wowsmod backend, or generate sim.json in the mock.',
  stWorking: 'AI is editing…', stOffline: 'Disconnected', gameRunning: 'Game running', gameRunningHint: 'The game client is running; quit it before installing or uninstalling.',
  install: 'Install to game', installed: 'Installed', installUpdate: 'Update game copy', installing: 'Installing', uninstalling: 'Uninstalling', reinstall: 'Reinstall to game', uninstall: 'Uninstall from game',
  notInstalled: 'This mod is not installed in the game.', installInfo: 'The game has step {n} · {files} files', installOutdated: 'There are newer changes; the game still has the older version.',
  installedTitle: 'Installed to game', uninstalledTitle: 'Uninstalled from game', jobFailed: 'The operation did not finish', opFailed: 'The operation did not finish',
  uninstallTitle: 'Uninstall this mod from the game?', uninstallDesc: 'Removes the files wowsmod wrote into the game’s res_mods and restores backups. The project and its history stay untouched; you can install again any time.',
  gameRunningTitle: 'The game is running', gameRunningDesc: 'Close the game client first, then install or uninstall (the game locks its files while running).',
  reconnected: 'Reconnected', reconnectedDesc: 'Changes made while disconnected have been synced.', offlineBar: 'Lost connection to wowsmod, reconnecting. You are seeing the last known state; revert and install are paused.',
  connOnline: 'Local service connected', connOffline: 'Local service unreachable', connConnecting: 'Connecting…', headIs: 'Latest: step {n}', mockServer: 'Mock backend', mockTip: 'Connected to the mock service (real Bourgogne data, same API as API.md)',
  drawsN: '{n} draws', idle: 'Idle (not redrawing)', quality: 'Quality', q_high: 'High', q_mid: 'Med', q_low: 'Low',
  q_high_tip: '2× pixel density, 4× MSAA, 2048 shadows, bloom, full-size textures', q_mid_tip: '1× pixel density, 2× MSAA, 1024 shadows, bloom, 1024 textures', q_low_tip: 'No shadows, bloom, MSAA or overlay; 512 textures. Suits integrated GPUs',
  ldConnect: 'Connecting to wowsmod…', ldProject: 'Reading the project…', ldModel: 'Downloading model', ldParse: 'Parsing model…', texLoading: 'Textures {d}/{n}', texFailTitle: (p) => `${p.n} ${enPl(p.n, 'texture', 'textures')} failed to load`,
  fatalNetTitle: 'Can’t reach the wowsmod preview service', fatalNetDesc: 'Make sure <code>wowsmod preview &lt;project dir&gt;</code> is still running in your terminal, then retry.',
  fatalNoModelTitle: 'Nothing to preview yet', fatalNoModelDesc: 'Import a ship with wowsmod first (the baseline step); the preview will appear automatically.', fatalTitle: 'Preview error', ctxLost: 'The graphics driver reset WebGL. Please reload the page.',
  newStep: 'AI finished step {n}', revertedTo: 'Reverted to step {n}', restored: 'Restored', noReady: 'No turret can fire (reloading or target outside its arc)',
  outline: 'Outliner', textures: 'Textures', nChanged: '{n} changed', nOfChanged: '{c}/{n}', filterParts: 'Filter parts', onlyChanged: 'Changed only', showAll: 'Show all',
  noChangedParts: 'No parts differ from the comparison base', noMatch: 'No matching parts', changedVsBase: 'Changed against the comparison base', unchanged: 'Unchanged', toggleVis: 'Show / hide', focus: 'Frame (or double-click the row)', solo: 'Show only this', deselect: 'Deselect',
  group_hull: 'Hull', group_main: 'Main battery', group_secondary: 'Secondaries', group_aa: 'AA guns', group_director: 'Fire directors', group_radar: 'Radars',
  short_main: 'Main', short_secondary: 'Sec', short_aa: 'AA', short_director: 'Director', short_radar: 'Radar',
  sec_Bow: 'Bow', sec_MidFront: 'Forward mid', sec_MidBack: 'Aft mid', sec_Stern: 'Stern',
  barrels: (p) => ['', 'single', 'twin', 'triple', 'quad'][p.n] || `${p.n}-gun`, trisK: '{n}k tris',
  texChangedN: 'Changed {n}', texAllN: 'All {n}', noChangedTex: 'No textures differ from the comparison base', usedByN: (p) => `${p.n} ${enPl(p.n, 'part', 'parts')}`, baseLabel: 'Base', afterLabel: 'After', sameAsBase: 'Same as base',
  slotShort_a: 'Albedo', slotShort_n: 'Normal', slotShort_mg: 'Metal/gloss', slotShort_ao: 'AO',
  ch_a_rgb: 'Albedo (alpha ignored)', ch_a_a: 'Alpha: cut-out mask for rails and nets', ch_n_r: 'Normal X', ch_n_g: 'Normal Y',
  ch_mg_r: 'Gloss (white = smooth)', ch_mg_g: 'Metalness', ch_mg_b: 'Camo mask; glow mask in emissive materials (glow = B² × albedo × power)', ch_ao_r: 'Ambient occlusion',
  compare: 'Compare', single: 'Single', split: 'Split', side: 'Side by side', overlay: 'Overlay', compareWith: 'Compare against', baseOrig: 'vs original', basePrev: 'vs previous', opacity: 'Overlay opacity',
  overlayLowQ: 'Overlay is off in Low quality (it renders the frame twice); use Split or Side by side', overlayLabel: '{a} overlay {p}%',
  channel: 'Channel', ch_final: 'Final', ch_albedo: 'Albedo', ch_normal: 'Normals', ch_emissive: 'Glow', ch_wire: 'Wireframe', ch_mask: 'Mask',
  look: 'Look', lookGame: 'In-game', lookGameTip: 'Lighting and colour response calibrated to port screenshots (default; use for sign-off)', lookStudio: 'Showroom', lookStudioTip: 'Studio showroom lighting: neutral and flattering, but differs from in-game colour',
  environment: 'Environment', studio: 'Studio', studioTip: 'Neutral grey studio, most faithful colour', ocean: 'Clear water', oceanTip: 'Sea and sky, with three waterline modes', darkStudio: 'Dark studio',
  waterMode: 'Waterline mode', wmFull: 'Full', wmCut: 'Cutaway', wmOff: 'Hidden', wmFullTip: 'Full sea surface; the underwater hull is covered', wmCutTip: 'Water is cut open on the camera side to show the underwater hull', wmOffTip: 'No water surface, waterline marker only',
  lighting: 'Lighting', lightDay: 'Day', lightDusk: 'Dusk (port lighting darkens colours)', lightNight: 'Night (check glow)', light_dusk: 'Dusk', light_night: 'Night',
  views: 'Views', v_hero: 'Default', v_side: 'Side', v_bow: 'Front quarter', v_stern: 'Rear quarter', v_top: 'Top', v_port: 'Port view', v_funnel: 'Funnel close-up', v_custom: 'Custom', focusSel: 'Frame selection',
  display: 'Display', highlight: 'Highlight changes', waterline: 'Waterline', camo: 'Permanent camo', camoTip: 'Blend the camo base colour by _mg blue, as the game does', glowGain: 'Glow intensity', glowGainTip: 'Preview only; does not change mod files', resetDisplay: 'Reset to defaults',
  reviewing: 'Reviewing step {n} · latest is step {h}', latest: 'Back to latest', vpHint: 'Drag to orbit · scroll to zoom · right-drag to pan · click to select a part',
  tabHistory: 'History', tabInspect: 'Inspector', tabSim: 'Simulation', noSteps: 'No changes yet', noStepsDesc: 'Tell the AI what you want, or run wowsmod commands yourself. Every step shows up here.',
  current: 'Current', reverted: 'Reverted', baselineMeta: 'Baseline · official SDK model', stepMeta: (p) => `${p.t} ${enPl(p.t, 'texture', 'textures')} · ${p.p} ${enPl(p.p, 'part', 'parts')}`,
  details: 'Expand', collapse: 'Collapse', viewThis: 'View', revertHere: 'Revert to here', revertConfirm: 'Later steps will be removed from the project (restorable). Revert to step {n}?', revertDo: 'Revert', restore: 'Restore',
  cmd_import_sdk: 'Import SDK model', cmd_tex_recolor: 'Recolour texture', cmd_tex_channel: 'Edit channel', cmd_glow: 'Add glow',
  noSelection: 'Nothing selected', noSelectionDesc: 'Click a part in the viewport or outliner to see details.', props: 'Properties', group: 'Group', section: 'Section', model: 'Model', module: 'Module', caliber: 'Calibre',
  changeState: 'State', bones: 'Bones', bone_yaw: 'yaw', bone_pitch: 'pitch', bone_recoil: 'recoil', lodTable: 'LOD', lodSrc: '.model chain', lodExtent: 'Switch dist.', verts: 'Verts', tris: 'Tris',
  materials: 'Materials', texturesUsed: 'Textures used', project: 'Project', projName: 'Name', ship: 'Ship', hullModel: 'Hull model', game: 'Game', changedParts: 'Changed parts', changedTex: 'Changed textures', projDir: 'Folder',
  src_exact: 'game data', src_approx: 'approx.', src_preview: 'preview', srcGP: 'Exact game data: from GameParams', srcVisual: 'Exact game data: from the .visual node tree and skin weights', srcGPtip: 'Exact game data: GameParams.{k}',
  secSail: 'Sailing', telegraph: 'Telegraph', rudder: 'Rudder', simReset: 'Reset', tel_rev: 'Rev', tel_stop: 'Stop', tel_q1: '¼', tel_q2: '½', tel_q3: '¾', tel_full: 'Full',
  tel_rev_long: 'Reverse', tel_stop_long: 'Stop', tel_q1_long: '1/4 speed', tel_q2_long: '1/2 speed', tel_q3_long: '3/4 speed', tel_full_long: 'Full speed',
  rud_m1: 'Port', rud_m0_5: '½ port', rud_0: 'Mid', rud_0_5: '½ stbd', rud_1: 'Stbd',
  speed: 'Speed', kn: 'kn', heading: 'Heading', rudderAngle: 'Rudder', heel: 'Heel', maxSpeed: 'Max speed', rudderTime: 'Rudder shift', turnRadius: 'Turning radius', engineUp: 'Accel / reverse time', sec: 's', m: 'm',
  heelWaves: 'Swell', waves: 'Swell strength', wakeFoam: 'Wake / foam', approxHeelTip: 'Approximate: heel ≈ 3.2° × rudder × (speed ratio)²; swell is summed sines. The game does not publish these values',
  approxWakeTip: 'Approximate: wake, bow waves and side foam are drawn by the preview, not the game’s particle system', approxNote: 'Speed and turning follow GameParams; time scale, heel and wake are approximate', approxShort: 'Partly approximate', hudHide: 'Collapse sailing info', hudShow: 'Expand sailing info',
  secCamera: 'Camera', cam_orbit: 'Follow', cam_free: 'Free', cam_game: 'Game 3rd person', cam_orbit_tip: 'Camera moves with the ship; orbit and zoom freely', cam_free_tip: 'Camera stays fixed in the world; the ship sails past', cam_game_tip: 'Like the game: camera orbits the ship, crosshair aims',
  secGuns: 'Guns', combatTip: 'Toggle port / combat (C)', statePort: 'Port', stateCombat: 'Combat', statePortTip: 'Turrets at rest (no port/combat glow difference found in the data)', stateCombatTip: 'Turrets track the target and can fire',
  aimMouse: 'Mouse aim', aimMouseHint: 'Point to aim, click to lock', aimDist: 'Target range', approxElevTip: 'Approximate: elevation is estimated linearly from range and max range (the elevation limits themselves are game data)',
  reloadReal: 'Real reload', reloadRealTip: 'Exact game data: shotDelay. When off, reload is capped at 2 s for quicker viewing',
  salvo: 'Salvo', single1: 'Single', salvoTip: 'All ready turrets within their arcs fire', singleTip: 'The next ready turret fires', mst_ready: 'ready', mst_rest: 'at rest', showLess: 'Show less', showAllCmds: 'Show all {n} commands', mst_turning: 'turning', mst_blocked: 'out of arc',
  rotSpeed: 'Traverse', horizSector: 'Horizontal arc', vertSector: 'Elevation', reloadTime: 'Reload', rotLimits: 'Traverse / arcs / elevation', rotLimitsDesc: 'GameParams values; hover a turret',
  fx: 'Flash / smoke / recoil', approxFxTip: 'Approximate: recoil ≈ 3 calibres; flash and smoke are drawn by the preview, not game effects', fxDesc: 'preview-drawn', aaMounts: 'AA guns', aaStatic: 'No traverse data in GameParams; kept static',
  secEmis: 'Glow animation', em_mod: 'From mod', em_mod_tip: 'Use emissionAnimationMode from the mod material', em_0: 'Steady', em_1: 'Pulse', em_2: 'Timeline', em_3: 'Scroll', em_2_tip: 'Timeline mode needs animMap, which the backend does not provide yet; a stepped approximation is used',
  emSpeed: 'Speed', approxEmisTip: 'Approximate: animation speed and amplitude constants are not confirmed from the game', emFormula: 'Formula', emFormulaTip: 'Exact (M2_REPORT): glow = mg.B² × albedo × (emissivePower + animEmissionPower·a(t))',
  secLod: 'LOD check', distance: 'Distance', d_near: 'Near', d_mid: 'Mid', d_far: 'Far', lodTint: 'Tint by LOD', lodBias: 'Switch scale', approxLodTip: 'Approximate: the game also scales switch distances by graphics settings; one multiplier stands in for that',
  hiddenShort: 'Hidden', camDist: 'Camera distance', lodRule: 'switch distances from .model extent', hudLod: 'hull {h} · main {m}',
  legendExact: '= GameParams / SDK data', legendApprox: '= drawn or estimated by the preview',
};

const ru: Dict = {
  appName: 'wowsmod — просмотр', docTitle: '{p} · wowsmod — просмотр', language: 'Язык', more: 'Ещё', ok: 'Понятно', cancel: 'Отмена', close: 'Закрыть', retry: 'Повторить', auto: 'Авто',
  loadingShort: 'Загрузка…', tierShort: 'ур. {n}', original: 'Оригинал', stepN: 'Шаг {n}',
  species_Battleship: 'Линкор', species_Cruiser: 'Крейсер', species_Destroyer: 'Эсминец', species_AirCarrier: 'Авианосец', species_Submarine: 'Подлодка',
  workspace: 'Режим', wsReview: 'Обзор', wsSim: 'Симуляция', wsReviewTip: 'Сравнение правок, текстуры, просмотр и откат шагов', wsSimTip: 'Ход, наведение и стрельба по игровым данным; проверка анимации свечения и LOD', simUnavailable: 'Бэкенд не отдаёт данные симуляции (/api/v1/sim)', simUnavailableDesc: 'Обновите бэкенд wowsmod или сгенерируйте sim.json в mock.',
  stWorking: 'ИИ вносит правки…', stOffline: 'Нет соединения', gameRunning: 'Игра запущена', gameRunningHint: 'Клиент игры запущен: для установки или удаления мода закройте игру.',
  install: 'Установить в игру', installed: 'Установлено', installUpdate: 'Обновить в игре', installing: 'Установка', uninstalling: 'Удаление', reinstall: 'Установить заново', uninstall: 'Удалить из игры',
  notInstalled: 'Мод сейчас не установлен в игру.', installInfo: (p) => `В игре — результат шага ${p.n} · ${p.files} ${ruPl(+p.files, 'файл', 'файла', 'файлов')}`, installOutdated: 'С тех пор появились новые правки, а в игре всё ещё старая версия.',
  installedTitle: 'Установлено в игру', uninstalledTitle: 'Удалено из игры', jobFailed: 'Операция не завершена', opFailed: 'Операция не завершена',
  uninstallTitle: 'Удалить мод из игры?', uninstallDesc: 'Файлы, которые wowsmod записал в res_mods игры, будут удалены, резервные копии восстановлены. Проект и история не изменятся — установить снова можно в любой момент.',
  gameRunningTitle: 'Игра запущена', gameRunningDesc: 'Сначала закройте клиент игры, затем устанавливайте или удаляйте мод (пока игра запущена, её файлы заблокированы).',
  reconnected: 'Соединение восстановлено', reconnectedDesc: 'Изменения, сделанные за это время, синхронизированы.', offlineBar: 'Связь с wowsmod потеряна, переподключаемся. Показано последнее известное состояние; откат и установка временно недоступны.',
  connOnline: 'Локальный сервис подключён', connOffline: 'Локальный сервис недоступен', connConnecting: 'Подключение…', headIs: 'Последний: шаг {n}', mockServer: 'Тестовый бэкенд', mockTip: 'Подключено к тестовому сервису (реальные данные Bourgogne, API как в API.md)',
  drawsN: (p) => `${p.n} ${ruPl(p.n, 'вызов', 'вызова', 'вызовов')}`, idle: 'Простой (без перерисовки)', quality: 'Качество', q_high: 'Выс.', q_mid: 'Сред.', q_low: 'Низ.',
  q_high_tip: 'Плотность пикселей 2×, MSAA 4×, тени 2048, bloom, текстуры в полном размере', q_mid_tip: 'Плотность пикселей 1×, MSAA 2×, тени 1024, bloom, текстуры 1024', q_low_tip: 'Без теней, bloom, MSAA и наложения; текстуры 512. Для встроенной графики',
  ldConnect: 'Подключение к wowsmod…', ldProject: 'Чтение проекта…', ldModel: 'Загрузка модели', ldParse: 'Разбор модели…', texLoading: 'Текстуры {d}/{n}', texFailTitle: (p) => `Не загрузились текстуры: ${p.n}`,
  fatalNetTitle: 'Нет связи с сервисом просмотра wowsmod', fatalNetDesc: 'Убедитесь, что в терминале по-прежнему запущена команда <code>wowsmod preview &lt;папка проекта&gt;</code>, и нажмите «Повторить».',
  fatalNoModelTitle: 'Пока нечего показать', fatalNoModelDesc: 'Сначала импортируйте корабль через wowsmod (базовый шаг) — просмотр появится автоматически.', fatalTitle: 'Ошибка просмотра', ctxLost: 'Драйвер видеокарты сбросил WebGL. Перезагрузите страницу.',
  newStep: 'ИИ завершил шаг {n}', revertedTo: 'Откат к шагу {n} выполнен', restored: 'Возвращено', noReady: 'Ни одна башня не может стрелять (перезарядка или цель вне сектора)',
  outline: 'Структура', textures: 'Текстуры', nChanged: 'изм. {n}', nOfChanged: '{c}/{n}', filterParts: 'Фильтр деталей', onlyChanged: 'Только изменённые', showAll: 'Показать все',
  noChangedParts: 'По сравнению с базой ни одна деталь не изменена', noMatch: 'Нет подходящих деталей', changedVsBase: 'Изменено относительно базы', unchanged: 'Без изменений', toggleVis: 'Показать / скрыть', focus: 'Показать крупно (или двойной щелчок)', solo: 'Показать только это', deselect: 'Снять выделение',
  group_hull: 'Корпус', group_main: 'Главный калибр', group_secondary: 'Вспомогательный калибр', group_aa: 'Зенитки', group_director: 'Директоры', group_radar: 'Радары',
  short_main: 'ГК', short_secondary: 'ПМК', short_aa: 'ПВО', short_director: 'Директор', short_radar: 'Радар',
  sec_Bow: 'Нос', sec_MidFront: 'Середина, носовая', sec_MidBack: 'Середина, кормовая', sec_Stern: 'Корма',
  barrels: (p) => ['', 'одноорудийная', 'двухорудийная', 'трёхорудийная', 'четырёхорудийная'][p.n] || `${p.n}-орудийная`, trisK: '{n} тыс. треуг.',
  texChangedN: 'Изменённые {n}', texAllN: 'Все {n}', noChangedTex: 'По сравнению с базой ни одна текстура не изменена', usedByN: (p) => `${p.n} ${ruPl(p.n, 'деталь', 'детали', 'деталей')}`, baseLabel: 'База', afterLabel: 'После', sameAsBase: 'Как в базе',
  slotShort_a: 'Альбедо', slotShort_n: 'Нормали', slotShort_mg: 'Металл/глянец', slotShort_ao: 'AO',
  ch_a_rgb: 'Альбедо (без альфы)', ch_a_a: 'Альфа: маска вырезов для лееров и сеток', ch_n_r: 'Нормаль X', ch_n_g: 'Нормаль Y',
  ch_mg_r: 'Глянец (белое = гладкое)', ch_mg_g: 'Металличность', ch_mg_b: 'Маска камуфляжа; в светящихся материалах — маска свечения (свечение = B² × альбедо × мощность)', ch_ao_r: 'Фоновое затенение',
  compare: 'Сравнение', single: 'Один вид', split: 'Шторка', side: 'Рядом', overlay: 'Наложение', compareWith: 'Сравнивать с', baseOrig: 'с оригиналом', basePrev: 'с прошлым шагом', opacity: 'Прозрачность наложения',
  overlayLowQ: 'На низком качестве наложение отключено (кадр рисуется дважды) — используйте шторку или вид рядом', overlayLabel: '{a}: наложение {p}%',
  channel: 'Канал', ch_final: 'Итог', ch_albedo: 'Альбедо', ch_normal: 'Нормали', ch_emissive: 'Свечение', ch_wire: 'Сетка', ch_mask: 'Маска',
  look: 'Вид', lookGame: 'Как в игре', lookGameTip: 'Свет и цвет откалиброваны по скриншотам порта (по умолчанию)', lookStudio: 'Витрина', lookStudioTip: 'Студийный свет: нейтрально и красиво, но цвета отличаются от игры',
  environment: 'Окружение', studio: 'Студия', studioTip: 'Нейтральная серая студия — самые точные цвета', ocean: 'Чистая вода', oceanTip: 'Море и небо, три режима ватерлинии', darkStudio: 'Тёмная студия',
  waterMode: 'Режим ватерлинии', wmFull: 'Вода', wmCut: 'Разрез', wmOff: 'Без воды', wmFullTip: 'Сплошная вода, подводная часть скрыта', wmCutTip: 'Вода разрезана со стороны камеры — видна подводная часть', wmOffTip: 'Без поверхности воды, только отметка ватерлинии',
  lighting: 'Освещение', lightDay: 'День', lightDusk: 'Сумерки (портовый свет затемняет цвета)', lightNight: 'Ночь (проверка свечения)', light_dusk: 'Сумерки', light_night: 'Ночь',
  views: 'Ракурсы', v_hero: 'Обзор', v_side: 'Сбоку', v_bow: 'Спереди-сбоку', v_stern: 'Сзади-сбоку', v_top: 'Сверху', v_port: 'Как в порту', v_funnel: 'Труба крупно', v_custom: 'Свой', focusSel: 'Показать выбранное',
  display: 'Отображение', highlight: 'Подсветка изменений', waterline: 'Ватерлиния', camo: 'Постоянный камуфляж', camoTip: 'Подмешивать базовый цвет камуфляжа по синему каналу _mg, как в игре', glowGain: 'Яркость свечения', glowGainTip: 'Только для просмотра, файлы мода не меняются', resetDisplay: 'Сбросить',
  reviewing: 'Просмотр шага {n} · последний — шаг {h}', latest: 'К последнему', vpHint: 'ЛКМ — вращение · колесо — масштаб · ПКМ — сдвиг · щелчок — выбрать деталь',
  tabHistory: 'История', tabInspect: 'Свойства', tabSim: 'Симуляция', noSteps: 'Изменений пока нет', noStepsDesc: 'Опишите ИИ, что хотите получить, или запустите команды wowsmod сами. Каждый шаг появится здесь.',
  current: 'Текущий', reverted: 'Отменён', baselineMeta: 'База · официальная модель SDK', stepMeta: (p) => `${p.t} ${ruPl(p.t, 'текстура', 'текстуры', 'текстур')} · ${p.p} ${ruPl(p.p, 'деталь', 'детали', 'деталей')}`,
  details: 'Подробнее', collapse: 'Свернуть', viewThis: 'Открыть', revertHere: 'Откатить сюда', revertConfirm: 'Последующие шаги будут убраны из проекта (их можно вернуть). Откатить к шагу {n}?', revertDo: 'Откатить', restore: 'Вернуть',
  cmd_import_sdk: 'Импорт модели SDK', cmd_tex_recolor: 'Перекраска текстуры', cmd_tex_channel: 'Правка канала', cmd_glow: 'Добавление свечения',
  noSelection: 'Ничего не выбрано', noSelectionDesc: 'Щёлкните деталь во вьюпорте или в структуре, чтобы увидеть подробности.', props: 'Свойства', group: 'Группа', section: 'Секция', model: 'Модель', module: 'Модуль', caliber: 'Калибр',
  changeState: 'Состояние', bones: 'Кости', bone_yaw: 'поворот', bone_pitch: 'наклон', bone_recoil: 'откат', lodTable: 'LOD', lodSrc: 'цепочка .model', lodExtent: 'Дист. смены', verts: 'Вершины', tris: 'Треуг.',
  materials: 'Материалы', texturesUsed: 'Текстуры', project: 'Проект', projName: 'Название', ship: 'Корабль', hullModel: 'Модель корпуса', game: 'Игра', changedParts: 'Изменено деталей', changedTex: 'Изменено текстур', projDir: 'Папка',
  src_exact: 'игр. данные', src_approx: 'прибл.', src_preview: 'просмотр', srcGP: 'Точно по игровым данным: из GameParams', srcVisual: 'Точно по игровым данным: дерево узлов .visual и веса скиннинга', srcGPtip: 'Точно по игровым данным: GameParams.{k}',
  secSail: 'Ход', telegraph: 'Телеграф', rudder: 'Руль', simReset: 'Сброс', tel_rev: 'Назад', tel_stop: 'Стоп', tel_q1: '¼', tel_q2: '½', tel_q3: '¾', tel_full: 'Полн.',
  tel_rev_long: 'Задний ход', tel_stop_long: 'Стоп машина', tel_q1_long: '1/4 хода', tel_q2_long: '1/2 хода', tel_q3_long: '3/4 хода', tel_full_long: 'Полный вперёд',
  rud_m1: 'Лево', rud_m0_5: '½ лево', rud_0: 'Прямо', rud_0_5: '½ право', rud_1: 'Право',
  speed: 'Скорость', kn: 'уз', heading: 'Курс', rudderAngle: 'Руль', heel: 'Крен', maxSpeed: 'Макс. скорость', rudderTime: 'Перекладка руля', turnRadius: 'Радиус циркуляции', engineUp: 'Разгон / задний ход', sec: 'с', m: 'м',
  heelWaves: 'Волнение', waves: 'Сила волнения', wakeFoam: 'Кильватер / пена', approxHeelTip: 'Приблизительно: крен ≈ 3,2° × руль × (доля скорости)²; волнение — сумма синусоид. Игра эти параметры не раскрывает',
  approxWakeTip: 'Приблизительно: кильватер, носовые волны и пена у бортов нарисованы просмотрщиком, это не игровая система частиц', approxNote: 'Скорость и поворот — по GameParams; масштаб времени, крен и кильватер — приблизительно', approxShort: 'Частично прибл.', hudHide: 'Свернуть данные хода', hudShow: 'Развернуть данные хода',
  secCamera: 'Камера', cam_orbit: 'Следовать', cam_free: 'Свободная', cam_game: 'Как в игре', cam_orbit_tip: 'Камера движется вместе с кораблём, можно вращать и приближать', cam_free_tip: 'Камера неподвижна в мире, корабль проходит мимо', cam_game_tip: 'Как в игре: камера вокруг корабля, прицел по центру',
  secGuns: 'Орудия', combatTip: 'Порт / бой (C)', statePort: 'Порт', stateCombat: 'Бой', statePortTip: 'Башни в походном положении (разницы свечения между портом и боем в данных не найдено)', stateCombatTip: 'Башни следят за целью и могут стрелять',
  aimMouse: 'Наведение мышью', aimMouseHint: 'Наведите, щелчок — зафиксировать', aimDist: 'Дистанция до цели', approxElevTip: 'Приблизительно: угол возвышения оценивается линейно по дистанции и макс. дальности (сами пределы — игровые данные)',
  reloadReal: 'Реальная перезарядка', reloadRealTip: 'Точно по игровым данным: shotDelay. Если выключено — не дольше 2 с, чтобы быстрее смотреть',
  salvo: 'Залп', single1: 'Одиночный', salvoTip: 'Стреляют все готовые башни в своих секторах', singleTip: 'Стреляет следующая готовая башня', mst_ready: 'готова', mst_rest: 'по-походному', showLess: 'Свернуть', showAllCmds: (p) => `Показать все ${p.n} ${ruPl(p.n, 'команду', 'команды', 'команд')}`, mst_turning: 'поворот', mst_blocked: 'вне сектора',
  rotSpeed: 'Скорость поворота', horizSector: 'Гориз. сектор', vertSector: 'Углы возвышения', reloadTime: 'Перезарядка', rotLimits: 'Поворот / сектора / возвышение', rotLimitsDesc: 'Значения GameParams; наведите на башню',
  fx: 'Вспышка / дым / откат', approxFxTip: 'Приблизительно: откат ≈ 3 калибра; вспышка и дым нарисованы просмотрщиком, это не игровые эффекты', fxDesc: 'рисует просмотр', aaMounts: 'Зенитки', aaStatic: 'В GameParams нет скорости поворота — неподвижны',
  secEmis: 'Анимация свечения', em_mod: 'Из мода', em_mod_tip: 'emissionAnimationMode из материала мода', em_0: 'Ровно', em_1: 'Пульс', em_2: 'Шкала', em_3: 'Бег', em_2_tip: 'Режиму «шкала» нужна animMap, бэкенд её пока не отдаёт; используется ступенчатое приближение',
  emSpeed: 'Скорость', approxEmisTip: 'Приблизительно: константы скорости и амплитуды не подтверждены по игре', emFormula: 'Формула', emFormulaTip: 'Точно (M2_REPORT): свечение = mg.B² × альбедо × (emissivePower + animEmissionPower·a(t))',
  secLod: 'Проверка LOD', distance: 'Дистанция', d_near: 'Близко', d_mid: 'Средне', d_far: 'Далеко', lodTint: 'Окраска по LOD', lodBias: 'Множитель смены', approxLodTip: 'Приблизительно: игра ещё масштабирует дистанции смены по настройкам графики; здесь это один множитель',
  hiddenShort: 'Скрыто', camDist: 'Дистанция камеры', lodRule: 'дистанции смены — extent из .model', hudLod: 'корпус {h} · ГК {m}',
  legendExact: '= данные GameParams / SDK', legendApprox: '= нарисовано или оценено просмотрщиком',
};

const DICTS: Record<Lang, Dict> = { zh, en, ru };
let cur: Lang = 'zh';
export function setLang(l: Lang) { cur = DICTS[l] ? l : 'zh'; document.documentElement.lang = cur === 'zh' ? 'zh-CN' : cur; }
export const getLang = () => cur;
export function t(key: string, p: Record<string, any> = {}): string {
  const v = DICTS[cur][key] ?? DICTS.en[key] ?? DICTS.zh[key];
  if (v === undefined) return '';
  if (typeof v === 'function') return v(p);
  return v.replace(/\{(\w+)\}/g, (_, k) => (p[k] !== undefined ? String(p[k]) : ''));
}
export const langName = (l: Lang) => ({ zh: '中文', en: 'English', ru: 'Русский' })[l];
export const missingKeys = () => { const all = new Set([...Object.keys(zh), ...Object.keys(en), ...Object.keys(ru)]); return (['zh', 'en', 'ru'] as Lang[]).map((l) => [l, [...all].filter((k) => !(k in DICTS[l]))]); };

/** 部件显示名：船体 → 船段名；挂点 → 「主炮 1」 */
export function partLabel(p: Part) {
  if (p.group === 'hull') return t('sec_' + (p.section || '')) || p.label;
  const n = (p.hp || p.id).match(/(\d+)$/);
  return `${t('short_' + p.group)} ${n ? n[1] : ''}`.trim();
}
export function partMeta(p: Part) {
  if (p.group === 'hull') return t('trisK', { n: Math.round(p.tris / 1000) });
  if (p.caliber_mm) return `${p.caliber_mm} mm${p.barrels && p.barrels > 1 ? ' ×' + p.barrels : ''}`;
  return (p.model || '').split('_')[0];
}
export function fmtTime(iso: string) {
  const d = new Date(iso); if (isNaN(+d)) return '';
  const loc = cur === 'zh' ? 'zh-CN' : cur === 'ru' ? 'ru-RU' : 'en-GB';
  const today = new Date().toDateString() === d.toDateString();
  return d.toLocaleString(loc, today ? { hour: '2-digit', minute: '2-digit' } : { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}
export const cmdName = (c: string) => t('cmd_' + c) || c;
