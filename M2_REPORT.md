# M2 报告: 贴图与发光(定稿 2026-10-10)

客户端 26.10.0.0.8867689(D:\Korabli), 测试船: 勃艮第 FSB510(船体模型 FSB025_Bourgogne_1945)。
游戏截图: `out\m2_ingame\shots\`(全分辨率 2560×1440 + `_small`), 副本在 box `/workspace/m2/ingame/`。

## 验收总表

| # | 项目 | 结果 | 证据 |
|---|---|---|---|
| 0 | Hydra 极小末块解压失败 | **已修复**, 全量 342,823 条 0 失败; 与 pfsunpack 抽样 463/463 一致 | 见下"进度"0 节 |
| 1a | DDS 读写 BC1/BC3/BC5/BC7 解码+编码, mip | 完成; BC1 46.3 dB / BC7 55–56 dB | texture.rs, 单测 |
| 1b | .dd0/.dd1/.dd2 分卷拆分/合并 | 规则弄清; 全量 24,040/24,042 组逐字节往返 | out\m2_tex\dd_volume_scan.json |
| 1c | 是否需要写回 BC7Prep | **不需要**: 游戏接受普通 DDS/普通 BC7(见第 4 节) | 往返实验 + 三次深海蓝安装 |
| 2 | tex export/import/info/rewrite/recolor/channel, mask, glow, mod init/import-sdk/status, install --dev, uninstall(均 --json) | 完成 | 命令实现节 |
| 3a | 勃艮第整船深海蓝(保留细节)进港可见 | **通过**(第二版起整船变蓝, 含船体与炮塔) | 截图见第 5 节 |
| 3b | 烟囱/上层建筑发蓝光 | **未生效**(原因定位到一半, 见第 6 节) | funnel_p4_vs_p30.png |
| 3c | DDS 读→写往返游戏内无差异 | **通过** | 02 vs 06 |
| 3d | uninstall 完全复原 | **通过**(4 次安装/卸载, 每次 res_mods 只剩 readme.txt, SHA-256 与备份一致) | uninstall_*.json |

## 第 4 节 往返实验(DDS 读→写, loose res_mods)
- 项目 `out\m2_projects\rt_loose`: Hull_a / Hull_mg / DeckHouse_a / DeckHouse_mg 经 `tex export`→PNG→`tex import --quality high` 重新编码(BC1, 含 .dd0–.dd2);
  Hull_n / DeckHouse_n 用 `tex rewrite` 去掉 BC7Prep 写成普通 BC7(偏移 40 = 0)。共 24 个文件 `install --dev` 进 res_mods。
- 港口截图 `02_base_port.png`(原版) 与 `06_rt_port.png`(往返版) 目视无差异; 像素差平均 15.6, 主要来自水面与云的动画(船体区域无结构性差异)。python.log 无贴图报错。
- 卸载: 删除 24 个文件、6 个目录, res_mods 与备份逐文件一致。
- 不足: 没有做"把 _n 换成纯色"的正向对照来证明普通 BC7 的 _n 确实被加载。旁证: M0 exp2c 与本阶段三次 SDK 安装都把 _n 去 BC7Prep 后装入, 港口法线细节(铆钉、焊缝)正常(见 cmp_stern_detail.png)。
- 结论: 游戏接受普通 DXT1 与普通 BC7(偏移 40 = 0), **不需要实现正向 BC7Prep**; 写回时沿用原贴图的分卷级数 K。

## 第 5 节 深海蓝(#1E3F66, 保留细节)
- 项目构建脚本 `scripts\m2_build_projects.ps1`(SDK 路线, mod 名 DeepSeaBourgogne): 对 Hull/DeckHouse 以及法系主副炮、指挥仪、雷达共 16 张 `_a` 执行
  `tex recolor --to-color #1E3F66 --exclude-hue 0:65 --soft 0.04`(OKLab 中把选区平均色移到目标色, 保留亮度起伏与 35% 原有色斑; 排除木甲板、锈迹、船底红)。
- **第一版(deepsea)问题**: 上层建筑变蓝, 但船体侧面和炮塔仍是浅灰(只是略暗)。
  - 原因: `_mg` 的 B 通道是**迷彩/涂装遮罩**。B=1 的区域显示的是 camouflages.xml 的配色(勃艮第 `camo_permanent_1`, colorSchemeFRABourgogne color0 = 0.239/0.255/0.267 浅灰), 而不是 `_a`。
    证据: Hull_mg 的 B 恰好覆盖船体侧板整条 UV 区(out\m2_ingame\camo\, box `/workspace/m2/orig/Hull_mg_ch.png`); 上层建筑在第一版里变蓝, 正是因为 glow 把 DeckHouse_mg 的 B 清成了 0。
    (M2 早期把 B 记作"dBuffer 贴花混合遮罩"——现在看它至少承担迷彩遮罩的作用, 是否还兼作贴花遮罩: 不确定。)
  - 港口里勃艮第当前是否"装备"了这个永久迷彩, 或无迷彩时也套 color0: 不确定; 但清零 B 后效果确定。
- **修复**: 新命令 `wowsmod tex channel <贴图> --channel b --set 0 [遮罩参数]`; 构建脚本对每张换色的 `_a` 同名 `_mg` 清零 B(DeckHouse_mg 由 glow 处理)。
  第二版 deepsea2: 整船(船体、炮塔、副炮、指挥仪)均为深海蓝, 铆钉/锈迹/水线/木甲板保留(`12_deepsea2_port.png`, `16b_stern_close.png`)。
- 代价: 清零 B 后这艘船不再显示任何迷彩配色(本 mod 的意图就是替换涂装, 可接受); 若要保留迷彩, 应改 camouflages.xml 配色——属于游戏数据, 本阶段未做。
- 安装: 541 个文件(项目 450 + 游戏补齐 90 + 生成 PnFModsLoader.py 1), 21 个目录; python.log: `registerShipMod DeepSeaBourgogne`, 28 个模型被替换, 无错误。

## 第 6 节 发光(未生效)
- 遮罩: MidBack GLB 中材质 DeckHouse_skinned 的部件 = 烟囱主体(752 三角形, DeckHouse 贴图 3.1%, 与其他部件共用 UV 0.09%), 遮罩叠图 box `/workspace/m2/t1/funnel_overlay.png`。
- 改动: `glow` 写 DeckHouse_mg.B = 255·sqrt(强度)(烟囱内)、0(烟囱外); 烟囱区域 _a 染成 #2E9BFF; `FSB025_Bourgogne_1945_DeckHouse_skinned.mfm` 的 fx 换成 `PBS_ship_emissive_skinned.fx`, emissivePower=4。
- 结果: 港口白天, 烟囱呈中性浅灰(像迷彩 color0), 不发蓝光(`17_crop.png`)。第三版 deepsea3 把 emissivePower 提到 30 并加 `emissionAnimationMode=1`(正弦脉冲)、`animEmissionPower=30`,
  (注: 通过 `powershell -File` 传数组参数 -MfmSet 时两项被拼成一个字符串, deepsea3 的 .mfm 里 emissionAnimationMode/animEmissionPower 两条属性是手工修正的, 文件见 out\m2_projects\deepsea3; 脚本该参数需改为分号分隔或直接调用 glow --mfm-set, 待修)
  同一机位 4 张连拍(0.7 s 间隔)烟囱亮度 172–174, 与 power 4 时(190)和原版(158)相比没有随 power 增大的变化 → **发光没有生效**。(`funnel_p4_vs_p30.png`, `cmp_side_before_after.png`)
- 已知/推断原因(均不确定):
  1. 着色器静态发光项乘了 `g_emissionTint`(引擎按实例给的常量)。若普通外观下引擎给 0, 改 .mfm 的 power 无论多大都是 0——与观察一致。官方发光皮肤可能通过外观/皮肤配置让引擎给出非零 tint, 来源未找到(camouflages.xml 中没有 emission/glow 字段)。
  2. 烟囱区 B=1 同时触发迷彩遮罩, 所以烟囱显示迷彩灰而非 _a 的蓝色; 即使发光生效, 底色也会被迷彩替换(发光颜色仍取 _a, 理论上是蓝色)。
  3. 没有验证 SDK 路线下 .mfm 的 fx 修改是否真的被加载(python.log 没有相关日志, 截图无法区分)。
- 下一步建议(M2 未完成项, 留给后续): ① 反查 g_emissionTint 的来源(着色器常量缓冲区 + 官方发光皮肤 FSB020_France_Space 的 .visual/外观配置); ② 做正向对照: 把 fx 换成明显不同的着色器确认 .mfm 改动生效; ③ 夜间港口/战斗回放环境下观察(规则不允许进战斗, 未做)。

## 第 7 节 项目格式与 install/uninstall
- 项目 = `mod.toml`([mod] name/version/description/game_build/ship/format_version, [install] mode = loose|sdk) + `edits/NNN_命令.json`(每条命令的参数、时间、输入 sha256、输出) + `assets/res_mods/...` + `.install/`(清单与备份, 已 gitignore)。
- `install --dev`: SDK 占位文件从游戏包补齐(同名多份时优先 content/gameplay/)、BC7 去 prep、补齐 .ddN 分卷(并检查分卷完整、K 与原版一致), sdk 模式生成空 PnFModsLoader.py; 已有文件先备份; 清单逐条写入并写后校验。
- `uninstall`: 只删除哈希与清单一致的文件(被用户改过的保留并报告)、还原备份、删除创建的空目录、清单移入 history。
- 实测: rt_loose 24 文件; deepsea / deepsea2 / deepsea3 各 541 文件、21 目录; 4 次卸载后 res_mods 均只剩 `readme.txt`(SHA-256 0157BF75…2F3F = 备份清单)。

## 第 8 节 许可证提醒
- block_compression 0.10(MIT)、regex、toml 等均 MIT/Apache。
- **oozextract**: crate 声明 MIT, 但它是 ooz(GPL-3.0)的移植——许可证存在风险, M7 发布前需定论(可选: 首次运行下载/自研解压)。Cargo.toml 注释仍为"待定"。

## 港口操作记录(遵守 M0 规则)
- 只在港口, 未进战斗、未花费、未领取奖励(首启的每日奖励界面用 Esc 关闭)。点击前核对: 屏幕 2560×1440, DPI-aware, 启动器矩形 257,110,2049,1230, "ИГРАТЬ"(570,1145), 退出确认"是"(1185,712) 每次先截图确认对话框再点。
- 港口视角旋转用左键按住 + 相对位移(SetCursorPos 绝对移动无效); 只改了相机, 不改任何设置。
- 共启动 6 次: 原版(01–02)、往返 rt_loose(06)、深海蓝 v1(08)、v2(11–17)、v3(20–21)、卸载后原版(24–26, 同机位改前对照)。每次 Alt+F4 退出后先 uninstall 并核对 res_mods, 再装下一个。

## 关键截图(box)
- `/workspace/m2/ingame/cmp_front_before_after.png` 默认机位 改前/改后
- `/workspace/m2/ingame/cmp_stern_detail.png` 船尾特写 改前/改后(细节保留)
- `/workspace/m2/ingame/cmp_camo_fix_v1_v2.png` v1(船体仍灰) vs v2(清零迷彩遮罩)
- `/workspace/m2/ingame/cmp_side_before_after.png`、`funnel_p4_vs_p30.png` 侧面/烟囱(发光未生效)
- `/workspace/m2/ingame/raw/02_base_port.png` vs `06_rt_port.png` 往返无差异
- `/workspace/m2/ingame/raw/24_restored_port.png` 卸载后复原

## 进度记录(最新在上)
- [x] 港口验收(14:00–14:35): deepsea2 修复迷彩遮罩后整船变蓝; deepsea3 验证发光未生效; 卸载后原版对照; res_mods 复原。

- [x] 命令实现(提交 24c5c44, 盒子与 Windows 构建 + 42 个单元测试通过):
  - `tex info/export/import/rewrite/recolor`、`mask`、`glow`、`mod init/import-sdk/status`、`install --dev`、`uninstall`, 全部支持 `--json`。
  - 盒子实测(本地贴图): tex rewrite 对 DeckHouse_a/Hull_n/Hull_mg/DeckHouse_ao 的 .dds+.dd0–2 与原文件逐字节一致; export→import(BC1)PSNR 45.1 dB, 头部 128 字节相同。
  - 烟囱遮罩: MidBack 的 rs3(材质 DeckHouse_skinned)即烟囱主体, 752 个三角形, 覆盖 DeckHouse 贴图 3.1%, 与非 skinned DeckHouse 部件共用 UV 仅 0.09%。
  - 换色 #1E3F66 + `--exclude-hue 0:65`(保留木甲板/锈迹/船底红): 选区平均色精确落到目标色, 明暗细节保留(预览 /workspace/m2/t1/rc_hull_preview.png)。
- [x] 发光研究(着色器反汇编, 证据 out\m2_glow\dxbc\*.asm):
  - 官方发光皮肤(如 FSB020_France_Space_1957_*_emissive.mfm)用 shader 6 = PBS_ship_emissive, 参数 emissivePower=0.5, animMap=default_emission_timeline.dds。
  - PBS_ship_emissive 像素着色器(b22): 静态发光 = mg.B² × 漫反射颜色(线性) × g_emissionTint[实例](引擎按实例给, 值不确定) × tint.w × emissivePower;
    动画项 emissionAnimationMode(0 无 /1 正弦脉冲 /2 animMap 时间线 /3 滚动遮罩)× animEmissionPower; emissionColorMode 0=漫反射色 1=maskColor1→2 渐变 2=向 maskColor2 混合。
    默认值: emissivePower=2.0, 两个 mode=0。→ 发光颜色来自 _a(漫反射)本身, 位置/强度来自 _mg 的 B 通道; .mfm 只控制开关(fx)与强度。
  - _mg 通道: R=光泽度(gloss), G=金属度(metallic), B=dBuffer 贴花混合遮罩(PBS_ship 与 emissive 版都用), 在 emissive 版中 B² 同时是发光遮罩。勃艮第原 B 通道约 22–28% 像素 >200, 直接换 shader 会大面积发光, 所以 glow 命令要重写 B。
- [x] 贴图编解码: texture.rs(block_compression 0.10 MIT, 纯 CPU): BC1/2/3/4/5/7 解码, BC1/3/4/5/7 编码(BC1 含三色+透明块), mip 生成(sRGB 感知盒式), PNG 读写。
  - 解码交叉验证: BC7 与 texture2ddecoder 逐像素一致; BC1 RGB 只有 ±1 舍入差(插值舍入实现差异), 透明黑像素 alpha=0 符合规范。
  - 重编码质量(PSNR vs 原图解码): 勃艮第 Hull_a(BC1 4096²) 46.3 dB, 0.1 s; Hull_n(BC7) 55–56 dB(Normal 档 1024² 约 1 s)。
- [x] 分卷规则 + dds.rs: `.ddN` = 第 N 级单 mip 独立 DDS; `.dds` = 第 K 级起的完整链, 头部 reserved1[0]=K、reserved1[1]='QHDD'; reserved1[2]=1 表示 BC7Prep。
  - 全量扫描(examples/dd_volume_scan, out\m2_tex\dd_volume_scan.json): 52,867 组贴图, 24,042 组有分卷; 其中 24,040 组 合并→拆分 逐字节还原;
    K 与"最大边 > 512 的 mip 各成一卷"规则吻合 24,040/24,042, 例外 2 个(LBR034_n K=1 而规则给 2; RSB039_Frunze_1940_camo1 头部 K=4 但只有 3 个分卷文件)。
    另有 1,077 组 >512 的贴图不分卷(合法, 分卷是可选的)。→ 写回时沿用原贴图的 K。
- [x] 0. Hydra 极小末块解压失败修复
  - 根因: oozextract 0.5.5 的 tANS 位读取器在流末尾一次预读 4 字节(`tans.rs:61` → `pointer.rs` `get_le_bytes`),
    原版 C ooz 依赖缓冲区尾部留白, Rust 版带边界检查越界即 `OozError`(verbose_errors 报 `oob Input[17]..21`)。受影响的是压缩后只有约 25–30 字节的块。
  - 修复: vendor/oozextract(0.5.5, Cargo.toml 声明 MIT)+ `[patch.crates-io]`; `get_le_bytes/get_be_bytes` 越界部分补 0。见 vendor/oozextract/WOWSMOD_PATCH.md。
  - 验证: ① 两个最小样本(25B→128B, 26B→148B)输出与 pfsunpack 逐字节一致;
    ② 全量扫描 342,823 个条目 / 130.6 GB 全部解压且 crc32 与 idx 一致, 0 失败(out\m2_oodle_scan\full.json);
    ③ scripts\accept_oodle_tiny.ps1: 含 <64B 小块的条目 6,155 个, 取其中最小块 ≤32B 的全部 403 个(含 M1 失败的 LMY404/Lightning*/LNT052 等)+ 随机 60 个, 与 pfsunpack SHA-256 比较 463/463 一致。
