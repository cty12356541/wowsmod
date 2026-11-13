# Robin / 知更鸟 P5、P5.1、P6、C2 历史实验归档

归档日期：2026-11-13。来源提交：ca6fab244c5a56697b7d8b88c449dc5e6cece9c2。下方保留精修前节点原文，供追溯；其中旧命令、阈值、默认授权与完成结论不再作为当前通用指引。当前入口为 `wowsmod help workflow.changelog`，对应[历史索引节点](../skill/workflow.changelog.md)。

## 范围与证据边界

- 关联船：旧修订档案指向勃艮第（SDK 名 FSB510_Bourgogne）；来源见[较早档案](../design-changelog-archive.md)的 r3、r35–r37。不能据此推定其他船支持。
- 客户端构建：这些节点没有逐项记录构建号，标记为未知；r/P/T/C 名称是迭代代号，不是客户端版本。26.10/26.11 均不能由这里推定。
- 港口：原文可确认某些夜景使用黑桃城；P2 v6、P5.1、C2 并未在每条记录中给出港口、画质和时段，缺项保留为未知。
- 历史已观察：特定船/迭代的贴图马赛克、日夜发光差异、部分静态装配可见、预览与实拍色差；只说明记录里的场景。
- 实验：C2 的 DeckHouse→Hull 节点/identifier 调整对翅膀与光环结果不同；不能升格为通用发光修复。P5/P6 配色占比、羽翼厚度和亮度数字均是该设计的尝试。
- 未解决：自定义部件发光的角度/法线/几何关联；动画 misc visual 可能未被游戏加载；没有当前构建实机证据的 26.11 动画能力；夜景分区色差与真实 SSAO。
- 训练房：2026-10-14 的 Robin_FX 原版对照属于历史授权，不授权后续任务开游戏或进入训练房。港口烟色与战斗烟色不一致。
- 文件哈希校验、registerShipMod 注册成功、截图可见与动画随时间运动是独立证据；本归档没有补做任何安装或实机验证。

## 精修前节点原文

### 来源：docs/skill/workflow.changelog.md

````text
# workflow.changelog: skill 修订记录
上级: workflow
下级: 无
---
每轮: 日期/层/差距/根因/补法/效果。更早行见 `docs/design-changelog-archive.md`。

| 日期 | 层 | 差在哪 | 根因 | 怎么补 | 效果 |
|---|---|---|---|---|---|
| 10-12 r41 | 3 | T3/P5 侧裙远距仍偏平、无真烘焙 AO/法线、分件不够立体 | 面板仅图集弱 AO+浅倒角; import 写扁 _n/_ao; 几何无凸框/铆钉 | T4(P5.1 浮翼底): 更大倒角/更深缝/外凸框+几何铆钉; 图集加深 AO+生成 prop_n/prop_ao 并 tex import 进壳; 金/发光短缝克制; 回转/仰角/邻炮 0 | 见 p5_1/turret_T4 并排; 碰撞 0 |
| 10-12 r40d | 7 | 金/peri/水/外带件仍偏离港口; 夜无同设计掩码 | 全局曲线不够; 无真 SSAO | fit_extras 重拟合 contactAO0.45/goldHue14/periHue6/waterOpa1.45; P5.1 三联; night 粗对黑桃城 | studio15.6→game6.87; gold33→12 peri14→4.7; 真SSAO仍无 |
| 10-12 r39 | 6 | 航迹想跟主题换色 | foam*/foam_wide 实拆为数据贴图(R0/G 遮罩/B.5), 无 RGB | skill+MODDABILITY 标颜色不可改; 主题 wake enable_if 永不启用; 训练房只拍对照 | 航迹不改色 |
| 10-12 r34 | 3/5/7 | ①炮塔壳港口读成平涂盒子+后下黑缝 ②发光读成白漆 ③艏翼与舯翼风格不一 ④原壳小突起打碎色块 | ①整圈套壳+单像素纯色 UV, 缝隙被港口 SSAO 压黑; 工作台环境光强/无 SSAO 没暴露 ②发光色近白 #B8E8FF、power 0.35 ③艏翼另写厚刃 blade() ④从未盘点原壳 | ①T3: 中上带局部板/封缝/去前板/面板细节区/顶板点缀; wysiwyg 记残差交预览 ②#58B8FF + power 0.7 ③wing_fan 复用 plume, check 加共享造型语言规则 ④`model prune` + 新节点 shape.refine.clean; apply_p4 末尾 verify_apply 闸门 | T3 回转/仰角 0 碰撞; 港口见报告 |
| 10-12 r38 | 3/5 | P6 抬白把 peri 改青灰+浅 navy → 发闷; 用户否决大肩甲, 要在 P5 上微调 | 把「抬白」做成「降辅色饱和」 | P5.1: 回悬浮+LS0.88; peri 回 #A8C2F2; navy 细靴仍饱和 #243A78; violet 更清 #8B70E0; 发光线/羽轴/power 略提(0.82); 面板保持深 AO | 预览白≈38%/发光 3.7%; 港口见报告 |
````

### 来源：docs/skill/workflow.layers.paint.color-logic.md

````text
# workflow.layers.paint.color-logic: 配色逻辑(量化+分区)
上级: workflow.layers.paint
下级: 无(工具: `palette.py`、`color_logic.py`、`part_colors.py`、`role_share.py`、`port-albedo`)
---
**定位(参照不钉死)**: 标杆评分/role_share 只给参考区间与差距方向; 主题语言、配色比例、图案可以自有设计, 偏离时写理由。
**何时**: 层 2 语言卡 / 层 4 底色前; 对标例子或主题立绘已齐。勿只抄色号、勿整图糊取色。
**方法(硬)**: **角色本色优先, 搭配逻辑来自标杆** — 主/辅/点缀 hex 必须来自角色部位取色(发/衣/环/翅/饰); 分区占比、L*跨度、主色块面积、暗色压边、发光线比例抄标杆 `role_share`, 不抄标杆色相。
**步骤**:
1. **部位取色**: `part_colors.py extract` 对立绘裁发/衣/环/翅/饰; 禁止背景色进主辅。
2. **标杆角色占比**: `role_share.py` 标杆关键帧 → 白底/主色块/暗色/发光/中灰%。
3. **推导板**: `part_colors.py derive` 立绘部位→船体分区(船壳/水线/甲板/上层/炮塔/外带); 主识别色做主色块(15–25%), 暗色≥15%, L_span≥50, 发光≤5%。
4. **港口**: 各屏上色 `port-albedo` → `--to-color`。港口 G/B 增益 > R: 金/暖色必须全补偿(否则发黄绿), 暖白半补偿; 以港口实拍复核(r21)。
5. **对标**: 同角并排 + `role_share` 我方; 中灰>30% 或主色块≈0 → 不过。`map.benchmark` 配色维≥3。
**完成标准**: 有部位板+推导图+分区表+port-albedo; 选定理由含「本色来源+标杆占比」。
**自评**: 去掉点缀还能认出角色本色吗? 是否粉脏灰糊成一色、L 拉不开?
**船体图案**: 学标杆原则(大块/硬边/分区/发光走结构边), 图案语言用角色自有母题(例: 知更鸟「追光」光束斜向主件); 不用细条纹(港口 mip 后碎)。
**明度占比自查**(同角预览, 先加水面遮船底): 角色本色的亮色(乳白/浅主色)应占船体可见面主导, 深色只做靴带/尖楔/点缀; 甲板也算大面(深甲板会把俯视整体拉暗)。数值看 meanL/暗像素%, 只作趋势, 标杆是照片不可直接比。
**分块与发光主线自查**: 船体每半船 ≤3–4 块大硬边块(多了远景变碎); 至少一条连续发光主线沿结构贯穿, 并把视线引向视觉中心(例: 舯部折 V 指向翼根, 上舷楼接到主件根部)。
**港口提亮教训(P5)**: 港口 G/B 增益+背光面压暗, 让 periwinkle/royal 读成饱和深紫并压过白(T3c 实拍: 白 11%, 主色 43% 其中深 28%; 标杆 d_002: 白 32%+浅灰 25%, 主色 9%)。对策: 主色改清透浅色(港口期望色经 `port-albedo`/`COLMODE=port` 反算), 深色只做小硬楔/水线细带, 多面共用纹素(deck)用浅色, 甲板是俯视大面要冷白。**验收用实拍**: `ship_mask.py`(GrabCut, 标杆帧用 `--poly`) → `role_share.py --mask --split-main 55`, 同角比标杆; 盒内预览偏亮只作趋势。

**港口白占比验收(P6)**: 斜前(a1 类)≥35%、船艏(v1 类)≥45%(同遮罩同机位 `role_share`); 点缀偏青(#58B8FF 系)少偏紫 peri; 水下靴带用浅灰蓝细带+水线发光, 勿留大块深蓝抢白。白天发光占比目标贴近标杆(~7%), 主线加粗+翼根/炮塔分缝发光点。

**抬白时保持辅色饱和(P5.1)**: 收深蓝靴/收面积可以抬白占比, 但不要把 peri 改成青灰低饱和(P6 教训: 整体发闷)。鲜明度用贴图/预览的 chroma 分布与实拍对比验收; 发光加粗要克制(略提 power/线宽即可)。
````

### 来源：docs/skill/workflow.layers.paint.coverage.md

````text
# workflow.layers.paint.coverage: 贴图层完成标准 + 部件覆盖清单
上级: workflow.layers.paint
下级: 无(工具: `role_share.py`、`bench_pair.py`、`port_shot.py`)
---
**输入**: 定稿色板(color-logic)、标杆同角帧、已导入贴图的项目。
**部件覆盖清单(逐项打勾, 每项写所用色+证据视角)**: 船壳干舷 / 水线靴带 / 水下 / 主甲板(含 DeckHouse 材质的甲板铺板) / 上层各层 / 烟囱 / 主炮塔(顶/侧/座圈) / **炮管**(常在 FGM UV 的圆柱展开矩形里或 C035) / 副炮 / 高炮 / 指挥仪 / 雷达 / 格栅 C004/C005/C011(SDK 占位用 `--from-game`) / 网 / 锚。任一项仍是原版灰 → 不过。
**完成标准(全满足)**:
1. 清单无原版灰; `_mg` B 已清。
2. 同角 6 视(舷斜高/舯近/艏甲板/俯视/舷侧/夜)`bench_pair` 并排, 配色/图案/覆盖三维各 ≥3(5 分制)。
3. `role_share`(去背景 `--bg-auto`, 近景我方 `ours_bg≈14`): 白 15–35%、主色块 15–40%、暗 15–50%、中灰_未归类 ≤25%; 发光 0 允许(发光层另算), 但要在汇报里写。
4. **港口实拍**白天 ≥2 角度(无 UI): 各区实测色与目标 ΔE 不离谱, 金不发绿、白不发薄荷; 近景无台阶锯齿。
5. 工作台与港口都无大块错色楔/白三角(多为甲板 UV 被背面三角复用或格栅色)。
**分级阈值(role_share 分类)**: 发光 L≥78 且 C≥10 冷色; 暗 L<32; 白 L≥70 且 C<14; 点缀 C≥25 点缀色相; 主 C≥20 主色相; 余为中灰。
**自评**: 白天港口一眼认得出是这个角色吗? 有没有一个部件仍是原版灰?
````

### 来源：docs/skill/workflow.layers.glow.md

````text
# workflow.layers.glow: 层 5 发光与材质
上级: workflow.layers
下级: 无(命令细节: `wowsmod help ref.glow`)
---
**目标**: 只在设计指定处发光, 强度与面积受控。
**输入**: §3 发光行(位置/颜色/面积上限)、层 4 的贴图与键色遮罩。
**要点**: 发光 = `_mg` B 遮罩 × 漫反射色; 一张 `_mg` 的遮罩一次给齐(多个 `--mask` 并集)。遮罩来自层 4 键色或 `lineart --crease/--boundary/--contour/--rings/--fill`; 不复用旧版本遮罩。
**两种风格**: 大面积微光(B≈25–50: `--b-max 40`)/ 局部强光(小面积满值)。不要把大面积做成满值, 会整船发白。发光色宜近白浅色核心; emissivePower 0.3–0.4 起试。
**船体发光线**(知更鸟 r22): 画师索引图里设 `glow` 色号(弧块金边内侧一条、甲板金线组内外各一、上层裙线/烟囱环下), 同时输出 `*_glow.png` → `glow --mg <Hull_mg> --mask hull_glow.png --mfm <Hull.mfm> --mfm <Hull_skinned.mfm> --power 0.5`。发光线挨着金线而**不是**金线本身(发光色=漫反射色, 金发光成橙糊)。
**发光色要与主色块色相分开**: 主色 periwinkle 时发光用天青, 薰衣草发光既读不出也被 role_share 记作主色/白。
**发光色要够饱和**(r34): 近白天青 #B8E8FF 在港口夜景读成「白漆」; 改 #58B8FF(饱和天青)+ emissivePower 0.7(船体与外带件同值), 夜景读主题色、白天也看得出。只换发光线色用 `scripts/paint/glow_recolor.py <_a> <glow遮罩> <#色> <out>`, 外带件图集发光列同色(`prop_atlas.py`)。
**额外模型材质**: 薄片/头发/衣物开 `doubleSided`, 镂空用 alphaTest(`model import` 自动随 glTF)。
**外带件夜景描边**(知更鸟 P5.1): 不发光的外带件(翼/光环/披风等)在港口夜景会随整体去饱和变成灰白剪影, 只剩船体发光线可读。关键外带件给**克制的发光描边**: 只取外缘细带(羽片两侧外缘约 7% 宽, 不铺满整面), 强度**低于船体主发光线**(遮罩值 0.5–0.7 × 主线, 同 emissivePower; 金环/金饰 ≤0.35-0.55, 发光色=漫反射金→暖金, 再高就成橙黄糊团)。描边色与船体发光线同一色相族(#58B8FF), 用道具专用 `_a`/遮罩, 不改共享图集。工具 `scripts/model/robin/prop_glow_rim.py` → `tex import <wing_atlas> --like <prop>_a.dds` + `glow --mg <prop>_mg.dds --mask <rim遮罩> --mfm <prop>.mfm`。验收: 白天前后差 <1% 像素, 夜景 `channel=emissive` 能看到羽缘线。
**外带件挂点与发光**(知更鸟 C2/halo 港口实测): 外带件挂 `*_DeckHouse` 节点 + `SHIPMAT_EMISSIVE_PBS_DeckHouse` 时改发光无效。renderSet `<node>` 改分段船体节点(如 `MidFront`)、identifier 改 `SHIPMAT_EMISSIVE_PBS_Hull`(全部 LOD)后, 翅膀夜景侧面转亮白; 光环同法仍暗(材质贴图与翅膀同构 → 疑法线/几何)。外带件默认挂船体节点, 每件夜景单独验收; 亮度随角度变, 「被照亮」≠「自发光」。
**完成标准**: 只有清单指定处发光; 发光面积在设计上限内; 白天看得出、夜间不糊成一片(工作台夜景 + 港口)。
**自评**: 关掉发光层 4 仍成立吗? 发光有没有抢掉人物/立绘的主体地位?
- 新增道具(rb_*)夜间不自发光: 法线/绕序/水密已验证无误; _mg B=255 亦无效(已回滚)。
````

### 来源：docs/skill/workflow.layers.verify.wysiwyg.md

````text
# workflow.layers.verify.wysiwyg: 所见即所得(游戏内预览)
上级: workflow.layers.verify
下级: 无
---
**目标**: 定稿在工作台「游戏内」look 里完成, 港口只做确认, 不靠反复进港试色。
**两种 look**: `game`(默认, 按港口实拍拟合: 强压缩曲线 + 蓝天青海环境光 + 左舷前上方的太阳)/ `studio`(浅色清透影棚, 用于展示)。UI 工具栏「游戏内 | 展台」; 出图用 `wowsmod render <项目> --look game-day|game-night|studio`。
**流程**: 配色/发光在 `--look game-day` 下判断 → 夜景用 `game-night` 粗看 → 定稿后进港确认(`verify.port`)→ 三联(旧/游戏内/港口)进设计书 §5。
**机位对齐港口**: `--query "view=custom&az=<度>&el=<度>&dist=<模型单位>&fov=<度>&tgt=x,y,z"`(az 0=船首, 正=右舷)。
**重新校准**(换港口/时段/游戏版本, 或有了新实拍): 见 `scripts/preview/calib/CALIBRATION.md`。
1. 在 `regions_*.json` 加一个 shot: 港口图 + 像素框(+色相过滤), 工作台机位与色类。
2. `fit_game_look.py --spec <json> --work <dir> basis` → `fit` → `apply --write <前端>/src/engine/gamelook.json` → 可选 `fit_extras.py`(gold×peri 网格)→ 重建前端与 release → `eval` → `triptych.py`。
3. 写 ΔE2000 前后(如实写, 不挑区域), 并记一行 changelog。
**已知残差**(game-day, P2 v6, 15 区; **r40d 均值 6.87** / studio 15.64; 原校准 7.5):
- 金色: `goldHue`≈14 后 low/gold ~12、high/gold ~6(原 33/17)→ 定稿金仍以港口为准。
- 深紫/peri: `periHue`≈6 后 peri ~3–6; royal 阴影仍偏亮(ΔE 7–9)。贴图 peri 可略偏品红。
- 受光白顶略灰(ΔE ~7)。
- 水: `waterOpa`≈1.45 抬近场不透明度(仍见水下轮廓)。
- 夜景: `sat`≈0.30(r41; 原 0.06 让道具/涂装全灰), 自发光像素豁免去饱和, 发光线/发光道具夜里保持原色; 判断夜景发光请用中/高画质(低画质无泛光)。**未做分区 ΔE**(无同设计夜景掩码)。
- 水面(`env=water`)水线有接触处理: 水下沉入水体色、水线湿带、细浪线; 只是预览补偿, 不进贴图。
- **外带件明暗**: `contactAO`≈0.45 近似朝下/掠射/水线接触(补港口 SSAO/檐下); 真·屏幕空间 AO 仍未做。判断外带件明暗仍以港口近景为准。
- **r42**(Robin p5_1 港口实拍, `regions_r42.json` 26 区, 机位近似 + 颜色类分区): r41 13.47 → **11.36**。day emisK 2→0(港口日景 mg 发光不可见: 青/金细线、光环、翅膀的白天“发光”是预览假象); night sat 0.65 / emisK 4 / sunI 3.0(港口夜船体紫/藏青保色, 背景去饱和)。残差: 日景白偏灰、紫/藏青偏浅(只对 Robin 拟合会把 p2v6 拉到 10.3, 未采纳); 判断日景可读性以港口为准, 细线类(青/金)在港口日景几乎洗没。新机位做分区: 复制 `regions_r42.json` 的 shot, 框 + 颜色类即可, 不必像素对齐; 跑 `r42_regional.py` 和 p2v6 回归(两边都报)。
**完成标准**: 定稿图标明 look; 与港口差异只剩上面这些残差, 或已写进 changelog。
````

### 来源：docs/skill/workflow.layers.verify.port.md

````text
# workflow.layers.verify.port: 港口实证步骤
上级: workflow.layers.verify
下级: 无
---
**目标**: 游戏内港口同角度证据(昼+夜), 不进战斗/训练房。**完成标准**: 舷侧/艏斜/艉斜/人物近景各一张, 夜景一组, `installed --verify` 完好, 游戏已关, 用户港口设置已还原。
1. 游戏关闭 → `wowsmod install <项目>`(`--dry-run` 先看)→ `wowsmod installed --verify`。
2. 截图与操作用 `python scripts/port/port_shot.py`(Windows): `launch --exe <游戏>\\<启动 exe>` → 等 60–90 s 进港 → `shot a.png --thumb a.jpg --clean`(按住右键时游戏会隐藏港口界面, 截图无 UI)→ 换角度 `orbit DX 0 --shot b.png`(拖动中截, 同样无 UI)/ `zoom N`。`--thumb` 缩略图给 AI 看。
3. 夜景: 港口时段是港口自带的, 不随机。先 `shot --thumb` 记下当前港口(用户默认), 再 `port <港口名>`(`ports` 列出名字与夜景港口; 固化的「头像下拉 → 选择港口 → 格子 → 保存修改」)→ 拍同一组角度 → **`port <用户原港口>` 还原**。坐标按 16:9、界面 100% 标定, 不同时先看缩略图核对。调色港口(如黑白城市)评审时注明。
4. 日志: `profile/python.log` 有 `registerShipMod … done`, 无新错误。动画物件拍 2 张相隔数秒的截图核对转速/方向。
5. `port_shot.py close` 关游戏 → `wowsmod uninstall <项目>`; 只有用户要求保留时才留在 res_mods, 并在汇报写明。
6. 亮度客观量: 截图里船体区域 ≥250 的像素占比与 p99 写进评审(>2% 且集中在船上算过曝)。
````

### 来源：docs/skill/workflow.layers.shape.refine.layout.md

````text
# workflow.layers.shape.refine.layout: 辅助布局(原则/参考区间/自问)
上级: workflow.layers.shape.refine
下级: 无(脚本: `scripts/model/sil_pair.py`、`layout_measure.py`、`robin/box_preview.py`)
---
**定位**: 帮你想清布局, **不钉死**。标杆只提炼原则和参考区间; 允许并鼓励偏离, 写明理由即可。
**原则**: 1) 视觉中心只一个, 其余分主/次/点缀三级; 2) 大件沿船长有节奏(强-弱/渐强), 重心不偏到一端; 3) 尺度与船体成关系(不压船也不弱到看不见); 4) 左右对称或有意的不对称; 5) 远景剪影一眼读出主题; 6) 件与件留呼吸, 不互相抢。
**参考区间**(布洛妮娅标杆实测, 仅参考): 主翼组梢距中线≈2.5–3.5 半宽; 竖向顶≈桅顶; 挂点≈0.5–0.6 船长; 艏饰展≈1–1.6 船宽; 光环 R≈0.03 船长。
**步骤**: 量标杆提原则 → 出 **≥2 个有主题意图的方案**(写意图/主次/重心) → 按下列自问打分选一个 → `box_preview.py` 或工作台同角出图 → `sil_pair.py` 剪影+彩色并排 → 迭代。
**自问**(每版答): 视觉中心明确吗? 大件互相抢吗? 远景剪影读得出主题吗? 压船吗? 太弱吗? 节奏/重心合理吗? 偏离标杆的理由?
**翼/羽类主件造型自问**: 是否从单点放射(→像竖扇)? 有没有骨架让羽根沿骨错开? 上/中/下是否分层且明度拉开? 羽面是否倾斜(俯视与侧视都有面积)? 有无厚度(厚覆羽/骨管, 远景羽厚≥羽宽 1/4)? 是否向外展开而非只向上? 根部有没有衔接件(肩甲/斜撑/整流板)接到舷楼结构, 而不是悬空插着?
**羽毛感**: 羽片要弯弧+根窄+末端收尖+长短渐变(初级长→次级短→覆羽圆头), 层间重叠; 等宽厚长条=船桨。金只走前缘/羽轴(细且渐细), 横向金条多了会抢。**衔接**: 用贴墙壁板+流线套筒/护腕+细斜撑接舷楼, 不用大球(臃肿)。
**翼类挂点/片厚经验(参考, 非门槛)**: 翼根挂在舷楼中下部(甲板以上约 舷楼高 40–60%)比贴舰桥顶更稳、更像「从船里长出」; 翼尖仍高于甲板并避开副炮/炮塔回转包络(先做碰撞检查)。羽片厚 ≈ 羽宽 8–10%、尾端再薄, 远看轻盈; 前缘金/羽轴细管(r≈0.01)保证远距可读; 片是闭合透镜体就无背面法线问题。
**大体量咬合船体(P6 原则, 参考)**: 标杆好看核心是外带件从船身「长出来」、成片有厚度; 悬浮羽翼易读成挂饰。优先「装甲板根部 + 羽翼末梢」: 贴舷楼肩甲(白实色收窄板, 勿用重 AO 面板以免港口读成灰方块)+ 小套筒/斜撑(无大球) + 同一 plume 羽扇; 翼展再大一档使俯视剪影明显变宽; 艏饰用同一生成器参数族、尺度小于主翼。碰撞验收: 主/副炮回转+仰角、烟囱均为 0(`wing_clash`/`turret_check`); 肩甲与舷楼可浅咬合(结构感), 不计入武器碰撞。量化: 俯视剪影宽度比、外带件面积占比(参考区间)。
**挂点撞武器回转区 → 可选「悬浮外移」**(参考, 非门槛): 整件外移出舷(间隙≈0.3–0.6 / 舷宽 2–5% 船长), 才能放得更低; 根部用小发光悬浮核心(角色光环语言)代替实体连接, 可加渐变光点做弱关联(勿用实体杆); 外移会放大翼展, 同步缩羽/加后掠, 翼展/舷宽≈3–3.5×; 可配 `model animate --preset float` 慢浮(幅小, 4–6 s)。取舍: 失去「从船里长出」的结构感, 换来低挂点与零碰撞; 碰撞检查需含炮管仰角高度。
**反例**: 只按「远景可读」放行(r22); 把「布局问题大」读成「件太大」一味缩删(r23); 照抄标杆位置造型(r24 纠正)。
````

### 来源：docs/skill/workflow.layers.shape.refine.uv.md

````text
# workflow.layers.shape.refine.uv: UV 与贴图烘焙
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 已 Apply 的 glb 网格; 分区色板(`paint.color-logic`)。
**做法**: 1) UV 按硬边拆岛, 大面优先 texel; 2) 底色/镶边/发光遮罩分通道或分贴图; 3) 需要细节时从高模 bake normal/AO 到低模; 4) 发光遮罩给 `glow`/`mfm`。
**完成标准**: UV 无严重拉伸; 镶边/发光在贴图上可读; 材质名对齐项目 mfm; 港口色走 `port-albedo`。
**道具图集法**(推荐, 知更鸟实测): 一张 1024 共享图集 8 列×128px(渐变羽/白/periwinkle/紫/深蓝/金/发光…, `prop_atlas.py`) + 同版发光遮罩; Blender 材质名 `rb_<列>` → 导出前把 UV 压进该列、合并单材质; 所有件 `import --texture atlas --glow glow`。改色只重出图集, 不动网格。
**不替代**: 船体原版 UV 大面仍用 `tex recolor/composite`; 本节点只管**新网格**。
**图集必须翻转安全**(r35, 实拍教训): Blender→glTF→游戏链路 v 是否翻转没法从盒内预览确认; T3 把纯色放 v=0.25、面板细节放上半, 进游戏后纯色件(艇饰白羽、炮塔纯色面)采到了细节区 → 灰/紫块+同心圆+青色短划的马赛克, 面板反而采到纯色(显平)。规则: 纯色一律取 **v=0.5** 中带; 有方向的细节放上带并在下带**上下镜像**一份(`prop_atlas.py` T3b), 这样翻不翻都一样。改图集布局后先在港口拍一张近景验。
**图集与 glb 同批次(P5)**: apply 必须从 glb 同目录取图集(`$R\props\prop_atlas.png`); 旧脚本取 `$R\prop_atlas.png`, 换了 glb 没换图集, T3b 修复进游戏仍是旧图集马赛克。改图集后解出已装 `_a.dds` 核对布局。羽片渐变也要翻转安全(关于 v=0.5 对称)。
````

### 来源：docs/skill/workflow.layers.shape.refine.check.md

````text
# workflow.layers.shape.refine.check: 穿模/面数/远近可读
上级: workflow.layers.shape.refine
下级:
- workflow.layers.shape.refine.check.turret: 炮塔外带件(壳/装甲板)检查时
---
**输入**: 已 hook 的项目; 设计尺度。
**检查**:
1. **面数**: `python scripts/model/mesh_budget.py <glb…>` 各件与合计过预算。
2. **穿模**: `render --warm` day side+stern+overhead; 翼根/环座/罩壳不扎进甲板舰桥; 炮管扫掠无挡。
3. **远近**: fq 缩略图能认主外带; 近景有倒角镶边(非光坨)。
4. **LOD**: 远距物件仍在(勿只留近 LOD)。
**出图**: 工作台 `focus=<部件>` 时 `dist` 无效, 近景用 `fill`(>1 拉近, 舯近景 MidFront 1.9)。Read 工具按路径缓存图 → 新图换新文件名再看。
**打分**: `role_share.py --bg-corners`(船贴画边时四边 k-means 会把船色当背景剔掉, 同图分数乱跳)。
**布局**(见 `shape.refine.layout`): 已答布局自问(视觉中心/互抢/压船或太弱/剪影读主题); 标杆仅参考。r22 根因: 只看远景可读就过线。
**炮塔外带件**: 每类炮塔都过 `workflow.layers.shape.refine.check.turret` 清单(随转/仰角/贴合/回转/造型/碰撞几何)。
**同一 mod 的同类外带件共享造型语言**(r34): 艏翼/舯翼/尾饰等同类件必须用同一生成器与参数族(如 `wing_fan()` 复用主翼 `plume()`: 同厚宽比、弯弧、收尖、分层、金前缘+发光羽轴、同色同发光色), 只按主次缩放; 不许各写一套(旧艏翼用厚透镜 `blade()`, 与舯翼风格不统一)。碰撞用 `prop_clash.py`(对无外带件 glb, 改前改后数对比)。
**apply 后自检**(r32, 必过才可拍/汇报): `scripts/model/robin/verify_apply.ps1 -Proj <dir> -Spec <props_spec>` 退出码=失败数, 须 0 —— 每个件都出现在非 misc 的已加载 `.visual`(`<vertices>` 名匹配)且 mfm 为 emissive fx; Hull/DeckHouse mfm 为 emissive; 任一 `.model` 带 `.ssm` 即告警(动画件可能只挂了骨骼动画、网格不在可见 visual)。再进港口拍一张确认件在画面里, 夜港(黑桃城)确认发光。r32 根因: `model animate` 只把 ssm 挂到船 `.model`, 网格在独立 misc visual 未被加载 → 游戏里翅膀消失, 但 apply 报 ok=True 未拦; 修前先用静态 `model import`。
**完成标准**: 清单全勾; 失败件回 model 修, 不进 accept。
**脚本**: `mesh_budget.py`(已固化); 炮塔件用 `turret_check.py` 自动碰撞; 其余穿模以出图目视为主。
**预览光照**: 工作台 STUDIO 灰光会把乳白/淡紫读成灰、发光不可见 —— 灰光预览只用于剪影/布局, 色彩审美以港口实拍或校准后的预览为准(预览↔港口校准另见对应节点)。
````

### 来源：docs/skill/workflow.layers.shape.refine.check.turret.md

````text
# workflow.layers.shape.refine.check.turret: 炮塔外带件清单
上级: workflow.layers.shape.refine.check
下级: 无
---
**何时**: 给炮塔加壳/装甲板/点缀后, 进港前。
**目标**: 随转、炮口/仰角、贴合、回转都不穿帮; 港口近景不读成盒子。
- 挂点随转: 用 `model import --part <HP_…|型号> --like Turret`, 装配后壳材质应出现在该 HP 网格里; 不挂船体。
- 炮口/仰角: 前方炮门与炮管全露; 0→vertSector 最大仰角炮管不碰壳。
- 贴合: 按实测炮塔平面轮廓(去炮管)外扩, 间隙中位 ≈0.02–0.04、外伸 ≤0.04(模型单位, 参考); 超椭圆外扩/喇叭口会读成托盘。
- 回转: 射界内(GameParams horizSector)每 10° 查壳 vs 船体分段/邻炮塔本体+炮管/邻壳; 只计原炮塔本体同姿态不撞的(扣基线)。后排炮管掠过前炮塔顶时不加顶板。
- 造型: 顶板/侧裙/前盾分件, 件缝+倒角; 金/发光只点缀(徽记、短条), 不做沿边整圈条纹。
- 立体感(T4): 远距仍能读出分件(缝深+大倒角+凸框/铆钉几何或强法线); 炮塔壳须有烘焙 AO/_n(或图集等效面板 AO+法线), 白为主、peri/royal 分区鲜明; 禁止整块套住的盒子感。
- 不整圈套住原炮塔(T3): 只做中上带局部装甲板(下沿抬离炮座/后悬, 前脸不做板), 内缘插入本体封缝(港口 SSAO 会把 0.01 级缝压成黑线); 面板走图集「面板细节区」(倒角明暗/底部 AO/铆钉/中缝), 纯色单像素 UV 在港口读成塑料盒。
- 碰撞几何: `turret_check.py` 用**无外带件**的整船 glb(如 p2 `geometry.glb`); 已 import 过外带件的项目缓存会把旧件并进分段/炮塔, 基线失真。
- 工具: `turret_check.py`(数值), `turret_pose_render.py`(转 90°+最大仰角姿态图; box 预览装配几何无 yaw 骨骼, `turret=` 不生效), `turret_shots.sh`(game-day 近景)。
**完成标准**: 每类炮塔全勾; `turret_check.py` 回转/仰角/邻炮碰撞 0; 港口近景(侧/斜/俯)复核。
````

### 来源：docs/skill/workflow.layers.shape.refine.clean.md

````text
# workflow.layers.shape.refine.clean: 原壳清理/降噪
上级: workflow.layers.shape.refine
下级: 无
---
**何时**: 新涂装/发光主线做完后, 港口近景看到原船壳上的小突起(舷窗盖、通风口、小舱口、锚链/线缆、小管件)把色块和发光线打碎; 或用户说「原壳细节太杂」。
**目标**: 不改设计本身, 只让色块与发光主线连续; 挂点/炮塔/LOD 不受影响。
**输入**: 项目(贴图层已定)、无外带件的整船 `geometry.glb`(工作台缓存; 别用已带外带件的项目缓存)、世界坐标烘焙 `*_pos.npz` 与画师色号图 `*_idx.png` / `*_glow.png`。
**盘点**(只读):
1. `wowsmod model prune --project <P> --renderset '^(Bow|MidFront|MidBack|Stern)$' --facing side|up --max-size 0.08 --dry-run` → 每个 LOD 渲染集的游离小连通块数/三角数, LOD0 全部列出(中心/对角线/三角数)。
2. 排序: `scripts/model/clutter_rank.py <pos.npz> <idx.png> <glow.png> <dry-run.json>...` → 每块包围盒内贴图像素的色号种类数与是否含发光; `texels=0` = 该块 UV 不在世界坐标涂装覆盖内(颜色与所在色块无关, 必然打碎色块) → 最优先。
3. 统计用 `scripts/model/clutter_inventory.py`(Blender 读 glb, 按尺寸/舷侧/朝上分类)。
**删/压/融判断**:
- 删(`model prune`): 游离(不与船壳共顶点)、对角线 < 0.08(约 1 m 内)、在舷侧色块面上(`--facing side`)或主甲板(`--facing up --box=…,y<0.75`)。只退化三角形, 顶点缓冲/节点/挂点不变; 各 LOD 按同条件独立判定。
- 不删: 与船壳焊接的(连通块大)、上层建筑上的功能件(测距仪/指挥仪/炮)、栏杆网(Net/wire 渲染集)、任何挂点附近会穿帮成空洞的件。
- 融(贴图): 删不了、又在发光线/色块边界上的 → 在 `_a` 上按周围色块色覆盖该块 UV, 必要时 `_ao` 局部提亮(`tex channel --mul`), 让它「读成色块的一部分」。
**命令**: `wowsmod model prune --project <P> --renderset <正则> [--part|--section] --max-size <d> [--box x0,y0,z0,x1,y1,z1 ...] --facing side|up|any [--side-band 0.06] [--dry-run]`(通用, 不写死船名)。
**验收**: ① dry-run 与实际删除数一致、LOD0–3 都有处理(LOD2/3 少是正常); ② 工作台同机位前后图(对比时两边都用同类 step: 改模型后预览会切「全模型」显示, 测距仪等深色件会变浅, 不是 prune 造成的); ③ 港口白天同角前后近景, 色块/发光线更连续, 无空洞/漏光; ④ `turret_check`/挂点不受影响。
**完成标准**: 盘点表(按打碎程度排序) + 删除清单 + 港口前后对比图写进设计书; 残余不能删的项写明为何保留。
**融(r35/P5)**: 舷窗圆点/流锈主要在 `_a` 明暗(重涂保留原图亮度) → `despeckle.py <a> <out> --mask hull_side_mask.png --thr 7`(只换比周围暗、同色相、饱和底上的小斑; 发光/金/白底铭牌不动); 遮罩由 paint 按法线产出, 不写死 UV 行带。凹凸在 `_n` → `tex channel Hull_n --channel r/g --set 128 --mask hull_side_mask.png` 只压舷侧, 甲板木纹保留。apply 参数 `Despeckle=1 FlatN=1`(流水线内执行)。验收: 港口同角前后, 舷窗消失。
````

### 来源：docs/skill/workflow.layers.shape.refine.hook.md

````text
# workflow.layers.shape.refine.hook: 导回挂点与动画
上级: workflow.layers.shape.refine
下级: 无
---
**输入**: 合格 glb; design 挂点表。
**做法**:
```
wowsmod model remove --project <p> --renderset <旧名>   # 必要时
wowsmod model import --project <p> wing.glb --name wing_p2 --scale …
wowsmod model animate --project <p> --name turret_shell --orbit …  # 仅随炮件
```
**规则**: renderset 名唯一(删后同名仍占) → **确定性重建**: 每轮 robocopy /MIR 贴图定稿目录→新目录, 按 `props_spec.json` 逐件 import(`scripts/model/robin/win/apply_p3.ps1`)。静件 `--section MidFront --like '^MidFront_DeckHouse$'`; 随炮件 `--part <完整部件名> --like Turret`(该型号全部实例都有)。
**坐标**: 游戏 y 上、艏 +z; `model export` 的 glb 是艏 −z(取反)。Blender 里 X=x, Y=艏 z, Z=上, glTF 导出往返正确。
**完成标准**: `model list` 见齐名; 无人体材质; 日侧图不挡炮管回转包络。
````

### 来源：docs/skill/workflow.layers.fx.md

````text
# workflow.layers.fx: 层 6 特效(全局贴图, 随主题)
上级: workflow.layers
下级: 无
---
**目标**: 炮口焰/开火烟/烟囱烟/航迹与主题、涂装、外带件同步设计, 且在工作台可见。
**原则(主题同步)**: 贴图全局共享(所有船) → 只做保亮度轻偏色, 不加亮/不加不透明度; 借发光主线色相但不发光; 炮口焰保暖色(开火语义), 烟 alpha ≤ 原版, 不动曳光/着火; 按船的主题感交给船模。
**可改性(莱服)**:
| 特效 | 贴图 | 结论 |
|---|---|---|
| 烟囱烟 | `particles/animated/Smoke_*` | 可行(港口实证) |
| 炮口焰 | `MuzzleFlashes_*` | 需训练房实测 |
| 开火烟 | `trails/shot_smoke`、`shot_hat`(RGB 白, 换色=乘色) | 需训练房实测 |
| 航迹 | `trails/foam*` 是数据贴图(R0/G 遮罩/B .5), 无 RGB 颜色 | **颜色不可改**(已实拆确认); 本主题不改航迹 |
发射器参数在 `content/assets.bin` 不可改; 按船只能动挂点位置(.visual), GP 绑定不改。
**工具**: `fx find --kind` → `fx ship-effects <船> [--gp]`(挂点→粒子) → `fx init <项目> --from 主题.json` → `fx apply <项目>`(跳过数据贴图; 缺 particles pkg 报 missing)。主题 = 项目 `fx.json` 的 `preview`(tint/strength/keep/alpha), 工作台与换色同一公式。特效单独建 loose 项目, 不混进船 mod。
**预览**: `render --look game-day|game-night --query "env=water&speed=full&warm=20"`(航迹+烟囱烟, 烟从 HP_FX_Smoke_* 冒) / `"…&fire=main&fireT=0.09"`(炮口焰; 0.6 看开火烟); `fx=vanilla` 出原版对照。无骨骼几何用估计炮口。
**验证**: 训练房已授权(炮火/开火烟/航迹): 原版与主题各一轮同机位连拍; 结束关游戏, 只删特效文件。航迹颜色以贴图实拆为准(foam*=数据), 训练房只拍对照证明观感未改。模式下拉点 Join 钮内侧 teal 船标(~1397,40@2560x1440), 勿点红钮右外侧灰条。
**训练房实测**(2026-10-14, Robin_FX robin-qingge 66 文件 vs 原版, 同机位连拍): 炮口焰仍暖橙(主题允许), 开火烟与烟囱烟在战斗中均为**黑褐色, 与原版肉眼无差**; 乘色换色对这些烟不可见(在战斗内烟色可能由 ramps/数据贴图驱动)。下一步查 `particles/ramps` 与 Smoke_*_LM/MVEA 的作用, 未证实前设计卡里烟色项标「未生效」。港口里 Smoke_* 换色可见(早期绿烟实证)与战斗内不一致。脚本: fx_training.ps1 需同时加载 common.ps1 + m3_game.ps1(Zoom/RelDrag), PS5.1 下用 UTF-8 读入(`[scriptblock]::Create((gc -Raw -Encoding UTF8 f))`)。
**完成标准**: 设计卡(色/形/强度/呼应/克制/可读性) + 可改性表 + 原版对照图; 列出全局受影响贴图; 未实测项写明。
- 烟/炮口色: *_LM=方向光照图, *_MVEA=运动矢量+发光; 色相为 assets.bin 内浮点参数, 无贴图杠杆。
````

