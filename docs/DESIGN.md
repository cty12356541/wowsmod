# wowsmod 设计说明与格式发现(M1)

本文档记录 M1 实现中依据的格式事实。每一条都注明证据来源; 没有验证过的写"不确定"。
客户端版本: 莱服 Мир кораблей 26.10.0.0.8867689(`D:\Korabli`, bin\8867689)。

## 1. 架构

```
crates/wowsmod-formats   纯库: idx / pkg / Oodle 容器 / geometry / assets.bin / GameParams / GLB / 健康检查
crates/wowsmod           CLI(clap): doctor / find / extract / export / check / geom
scripts/*.ps1            可重跑的验收脚本(输出到 out\, 不入库)
testdata/                集成测试样本(不入库, scripts/fetch_samples.ps1 生成)
```

发布构建: MSVC 工具链 + `+crt-static`(`.cargo/config.toml`), `dumpbin /dependents` 只依赖
kernel32/ntdll/advapi32/bcryptprimitives/api-ms-win-core-synch, 无 vcruntime/ucrt, 可单文件分发。

## 2. idx / pkg

- idx V40: 225 个 idx, 342,823 个文件条目(doctor 实测)。解析器移植自 wows-toolkit(MIT, landaire), 见 `idx.rs` 文件头署名。
- 每个条目的 `compression_info`(cinfo) 只有两种取值(全量统计):
  - `0x6` 且 `size == unpacked_size`: 原样存储(218,404 个);
  - `0x7_0000_0006`: 莱服 Oodle 容器(124,419 个)。
  - 旧版 deflate(size != unpacked 且非 7)在 26.10 里没有出现, 保留 miniz_oxide 兜底。
- idx 里的 crc32 是**解压后**数据的 crc32(fakegame 125/125, 实机验收全部一致)。extract 默认校验。

### 2.1 莱服 Oodle 容器(M1 修正)

```
+0x00 u64 header_len = 0x30 + 4*nb
+0x08 u32 compressor   (Oodle 枚举: 8 Kraken, 9 Mermaid, 11 Selkie, 12 Hydra, 13 Leviathan)
+0x0C u32 = 1          (含义不确定)
+0x10 u64 unpacked_size
+0x18 u64 packed_size
+0x20 u32 block_count  (= ceil(unpacked / block_size))
+0x24 u32 block_size   (0x40000)
+0x28 .. 0x37          (未知, 观察到为 0; 不确定)
+0x38 nb × u32         每块压缩大小(之和 = packed_size)
+0x38+4*nb             压缩流, 条目总长 = 0x38 + 4*nb + packed_size
```

- **每块独立解码**到 `out[i*block_size ..]`。连续解码在 3 块以上时会失败(M0 只处理了单块条目)。
- 解析时做全部一致性校验(块数、header_len、块大小之和、条目长度), 不符即报错, 不猜。
- 替换点: `compression::OodleBackend` trait + `set_oodle_backend()`。本仓库不含任何实现, 运行时由用户注册(见 docs/OODLE.md)。

### 2.2 贴图 BC7Prep(M1 验收中发现)

部分 BC7 贴图在 pkg 中是 Oodle Texture "BC7Prep" 重排形式, extract 默认还原为标准 BC7。见第 7 节。

## 3. geometry

布局见 `<WORK_DIR>\m0\...\MODELS.md`(M0)。M1 新增结论:

- **往返逐字节一致**(18 个样本, 含 6 个带退化三角形的文件, 18/18): 解析→序列化, 以及
  解码→重新编码→序列化 均与原文件一致。
  - 顶点编码: meshopt-rs 自带编码器与游戏输出不一致; 移植 meshoptimizer 1.3 的 v0 顶点编码器
    (含 "favor consistent bit selection" 平局规则)后逐字节一致(`vertex_codec.rs`, MIT 署名)。
  - 索引编码 v1(meshopt-rs `experimental`): 编码器按 FIFO 状态轮换三角形顶点; 游戏数据解码出的顺序是编码器的不动点,
    但退化三角形(如 219,219,220)再编码会被再次轮换 → 对不一致的三角形依次尝试另两种轮换作为输入, 直到解码 == 原顺序。
  - 对任意新网格(非编码器产出), 某些三角形三种轮换都无法原样复现: 接受"轮换等价"(绕序不变), 并通过
    `encode_indices_report` 报告个数。这种情况只会出现在 M2 导回新网格时, 不影响游戏原始数据。
- 顶点格式步长(26.10): `xyznuvtpc`=24, `xyznuvrpc`=24, `xyznuvpc`=20, `xyznuviiiwwtpc`=32(单元测试)。
- **法线是无符号 UNORM8**: `n = b/127.5 − 1`。平均长度 0.99; 按 wows-toolkit 的有符号 int8 解释得到 1.13–1.37(错误)。
- **坐标/绕序**: 导出到 glTF 时 Z 取反, 同时三角形改为 (a, c, b)。证据: 不反转时 "面法线 · 顶点法线" 同向率 0.01%,
  反转后 99.99%。wows-toolkit 的说法与此相反(错误)。
- 少数模型整体绕序与法线相反(例如副炮 FGS024/FGS052: 同向率 0%, 有向体积为负, 而顶点法线大多朝外)。
  原因不确定(可能依赖引擎侧的镜像变换或双面材质)。导出时**逐 primitive** 检测, 多数相反则整体翻转该 primitive,
  并在 glTF `extras.winding_flipped = true` 记录, 供 M2 导回时还原; `--keep-winding` 可关闭。

## 4. 配对(为什么 M0 的 GLB 破面)

一个 geometry 有多个 vertices/indices mapping, **不能按数组下标两两配对**。正确做法:

- mapping id = `murmur3_32("<Node>Shape.vertices" / "<Node>Shape.indices", seed 0)`。
- 来源 1: SDK 文本 `.visual` 的 `<renderSet>`(node 名 → murmur3)。
- 来源 2: `content/assets.bin`(位于 system_data_0001.pkg)的 VisualPrototype:
  blob 2, 记录 0x40 字节, geometry selfId @+0x20, renderSet 数 @+0x30, 相对偏移 @+0x38;
  RenderSet 0x50 字节: +0 vmap id, +4 imap id, +0x20 mfm selfId, +0x30 bboxMin, +0x40 bboxMax(f32×3)。
- 每个 geometry 被多个 visual 引用(主 visual + lods/*_lod1..3; 整船根 geometry 只被 `_Full_lod4` 引用,
  其主 visual 没有渲染集)。`--lod` 选择; 主 visual 没有渲染集时自动改用带渲染集的 visual 并给出警告。
- **独立校验**: assets.bin 里每个渲染集自带包围盒, 与"被索引引用的顶点"的包围盒逐个比较
  (相对误差 1e-3)。正确配对时全部一致(黎塞留 43/43、勃艮第见验收报告), 这是与健康检查无关的第二条证据。
- 交叉验证: 勃艮第 SDK `.visual`(M0 实验中游戏生成)与 assets.bin 两条来源得到的 (vmap, imap, 材质) 集合一致,
  见 `scripts/accept_visual_xcheck.ps1`。
- 兜底: 都没有时用启发式(按索引最大值与顶点数匹配), 结果标记 ambiguous, `ok=false`。

## 5. 健康检查(取代 tools/glb_check.py)

- 越界(OOB): 索引 ≥ 该 primitive 顶点数。
- 长边三角形(仅参考): 最长边 > 0.5 × primitive 包围盒 3D 对角线。真实游戏几何里本来就有(甲板长条、舭龙骨、钢索、整块甲板四边形)。
- **尖刺**: 长边三角形, 且其"面法线 · 顶点法线之和"的符号与该 primitive 多数三角形相反,
  且面法线与顶点法线**不**严格共线(|cos| < 0.95)。面积为 0 的三角形不计。
  - 以多数为基准: 兼容整体反绕序的模型(副炮)。
  - 共线但反向的长三角形只计入 `backface_long_triangles`(参考): 它位于顶点法线描述的平面内, 只是绕序反了;
    FSB017 MidBack 的 C002_Razlom 贴花里有 1 个这样的原始三角形。
  - 没有法线时退化为 glb_check 规则: 长边三角形占比 > 0.5% 视为尖刺。
- 旧 glb_check.py 的缺陷: 用"最大轴长"而不是对角线, 且只检查 2 条边。
- 灵敏度: naive(数组下标)配对时, 黎塞留 Bow 47,672 个越界三角形、89 个尖刺; 18 样本里所有多 mapping 文件都被判为不通过。

## 6. GameParams(船只检索)

- `content/GameParams_py3.data`: 整体字节反转 → zlib → Python pickle 协议 2。
- 自写最小 pickle 读取器(`pickle.rs`, 不执行代码; 26.10 用到的操作码全部实现, 未知操作码报错)。
- 顶层 `(dict 参数ID → GPData, set)`; 舰船 `typeinfo.type == "Ship"`, 共 1165 条。
  船体模型取自 `*_Hull` 模块的 `model`; 其他挂点(HP_*)的 `model` 为炮塔/防空/雷达等。
- 与 Python `pickle` 解出的结果逐条对比: 1165/1165 条的 index、level、船体模型、其他模型完全一致。
- 例: 勃艮第 = 参数 `PFSB510_Bourgogne`, 船体模型 `FSB025_Bourgogne_1945`(不是 FSB510 目录); 黎塞留 = `PFSB108_Richelieu`, `FSB003_Richelieu_1944`。
- 本地化名来自 `bin\<build>\res\texts\ru\LC_MESSAGES\global.mo` 的 `IDS_<参数代号去掉前缀 P>`。
- "整船文件集" = 所有模型目录下的文件 + 同级 `textures/` 中以模型代号开头的贴图(例如 `battleship/textures/FSB025_*`)。
  这是按命名约定的归集, 不是从材质引用推出的, 可能漏掉共享贴图(例如 common/ 下的通用材质), 不确定。

## 7. 纹理: .dd0–.dd2 与 BC7Prep

- M0 实验结论(保持): 改贴图必须连同 `.dd0/.dd1/.dd2` 一起提供(游戏按需加载高分辨率 mip 链), 只改 `.dds` 不生效或混用。
- M1 新发现: 部分 BC7 贴图(DX10 头, DXGI 格式 98 BC7_UNORM; 文件偏移 40 即 `dwReserved1[2]` = 1)在 pkg 中存放的是
  **BC7Prep** 形式(RAD Oodle Texture 的"便于压缩的重排", 不是有损变换)。pkg 中的字节 crc32 与 idx 一致, 即这就是游戏原始数据;
  pfsunpack 解包时会把它还原成标准 BC7 并把偏移 40 清零。首轮 extract 验收 429 个文件里 23 个不一致全是这种贴图。
- 没有找到公开的 BC7Prep 规范或开源解码器(网络检索), 游戏目录里也没有 oo2tex 之类的 DLL。
  `wowsmod-formats::bc7prep` 是用(原始, pfsunpack 输出)成对样本**黑盒逆向**得到的纯 Rust 实现:
  - 载荷 = 48 字节头 `u32 version(=0x7bc), u32 flags, u32 counts[10]` + 按模式 0..9 依次的记录组 + `ceil(N/2)` 字节模式图
    (每块一个 nibble, 低 nibble 在前)。模式 m 的第 k 个块对应模式 m 组的第 k 条记录。总块数 N = Σcounts, 输出 16N 字节。
  - 模式 0..7: 每块 16 字节"规范化记录"(字段重新排列: 去掉模式位, 端点按通道/端点对齐到字节或 16/24 位边界,
    分区号与 p-bit 挪到端点之后, 索引原样)。flags 第 m 位 = 1 时该模式的记录按字节拆成两个平面
    (第一平面宽度: 模式 0/1/5/6 为 8, 模式 2/3/7 为 12, 模式 4 为 6)。
  - 模式 4/5 的记录按"未旋转"的 RGBA 顺序存端点(旋转位单独存), 模式 5 的 7 位颜色端点存为 `v<<1`, 模式 4 的 6 位分量
    存高 5 位 + 最低位单独放在 bit0/bit1。
  - flags 第 16 位 = 颜色去相关(在 2000 个随机贴图里出现于 7 个, 多为粒子/带 alpha 的贴图; flags 如 0x100c0/0x100f0):
    端点存为 G、R−G、B−G(模位宽), alpha 原样。模式 0/1/2/3 位置不变只换含义; 模式 4/5/6/7 的排列与非去相关形式不同
    (见 `bc7prep.rs::unprep_block_decorrelated` 注释); 模式 5 旋转 2 时 G 的最低位藏在 R−G 字节最低位;
    纯色记录存为 (G, R−G, B−G, A)。模式 2 + 去相关是全量扫描时才发现的(54 个文件), 取其中 33 个与 pfsunpack 对比后确认。
  - 模式 9 = 纯色块, 只存 RGBA8(4 字节), 还原为固定写法的模式 5 块: 旋转 0, 颜色索引全为 1, alpha 索引全为 0,
    A0 = alpha, A1 = 0, 每个颜色分量 v 用 7 位端点 `(v>>1, v<128 ? (v+1)>>1 : (v-1)>>1)`(在 547,782 个纯色块上无一例外)。
  - 每个字段的位映射见 `bc7prep.rs::unprep_block`。单测含正向变换的随机往返和纯色插值校验; 集成测试 `bc7prep_matches_pfsunpack`
    用真实样本逐字节比较; `scripts/accept_textures.ps1` 大批量对比并统计覆盖到的模式/旋转组合。
  - 未见过的情形明确报错而不是猜: version ≠ 0x7bc、模式 8 计数 > 0、flags 含 0..7 与 16 以外的位。
  - 不确定之处: 若某模式/旋转组合从未在真实数据中出现(见 M1_REPORT 的覆盖统计), 其映射是按同模式其他组合的规律推断的。
- `extract` 默认还原(与 pfsunpack 一致), `--raw` 保留 pkg 原始字节。M4 往回打包时要么写标准 BC7 且偏移 40 = 0
  (游戏是否接受未 prep 的贴图: **M2 已实测接受**, 见第 9 节), 要么实现正向 BC7Prep(不需要)。

## 8. M4 需要处理的问题(TODO)

1. **SDK 占位文件崩溃**: 26.10 客户端 `extractSources` 导出的 .geometry/.dds/.splash 全是 35–44 字节的占位 XML,
   用占位文件 `registerShipMod` 会导致客户端崩溃(M0 exp2a)。M4 打包时必须用 pkg 中的真实文件填充(参考 m0\ingame\tools\fill_stubs.py)。
2. **贴图需要 .dd0–.dd2**: 替换贴图必须同时生成 .dds + .dd0/.dd1/.dd2; 并决定写标准 BC7(偏移 40 = 0)还是正向 BC7Prep(见第 7 节), 需实测游戏是否接受前者。
3. 导回 geometry 时还原 `extras.winding_flipped` 的 primitive 绕序, 并重新计算 assets.bin 中渲染集包围盒是否需要同步(不确定)。

## 9. M2: 贴图写回、换色、发光、项目与安装

### 9.1 分卷与编码
- `.ddN` = 第 N 级单 mip 的独立 DDS; `.dds` = 从第 K 级起的完整链, 头部 reserved1[0]=K, reserved1[1]='QHDD', 偏移 40 = 1 表示 BC7Prep。
  全量 24,040/24,042 组合并→拆分逐字节还原; K 基本符合"最大边 > 512 的 mip 各成一卷", 但写回时一律**沿用原贴图的 K**。
- 编码: block_compression 0.10(MIT)纯 CPU, BC1/3/4/5/7; mip 用盒式滤波(`_a` 在 sRGB 线性化后平均, 其余线性)。
- 游戏接受普通 DXT1 与普通 BC7(偏移 40 = 0, 不做 BC7Prep), 见 M2_REPORT 第 4 节。

### 9.2 换色(recolor)
- OKLab 空间: 选区(遮罩 × 饱和度/亮度/色相软选择)的加权平均色移到目标色, 每个像素保留与均值的亮度差(detail)和部分色度差(chroma_detail)。
- 遮罩来源: PNG、UV 矩形、GLB(按材质/网格正则/包围盒选三角形光栅化到 UV, 带保守像素与膨胀)。

### 9.3 _mg 通道(M2 结论)
- R = 光泽度, G = 金属度, **B = 迷彩/涂装遮罩**: B 高的区域显示 camouflages.xml 配色而不是 `_a`。
  只改 `_a` 时船体侧面不变色就是这个原因; `tex channel --channel b --set 0` 清零后 `_a` 才显示。是否兼作贴花遮罩: 不确定。
- 在 `PBS_ship_emissive*.fx` 中, 发光 = B² × 漫反射色 × g_emissionTint(按实例由引擎给) × emissivePower(+ 动画项)。
  g_emissionTint 着色器默认 1.0, 普通船也不是 0(M2 补完时实测)。
- **着色器族由 .visual 的材质标识选择**(如 `SHIPMAT_PBS_DeckHouse_skinned` → `SHIPMAT_EMISSIVE_PBS_DeckHouse_skinned`; assets.bin 有 1287 个 SHIPMAT_* 标识), 只改 .mfm 的 fx 不生效。
  `glow` 默认同时改标识(`--keep-identifier` 关闭)。EMISSIVE 族不做迷彩混合, 所以发光区与迷彩区的 B 冲突随之消失。
- .mfm 属性确实被读取(diffuseMap 指向纯红贴图的对照实验)。

### 9.3b 港口观感补偿
- 港口光照+色调映射对暗色按通道非线性(低 R 压成 0、B 放大)。`port-albedo` 用两点标定的线性模型 截图 = a·(贴图 − t) 反推贴图色; 只对白天光照标定, 港口光照每次启动可能不同。

### 9.4 mod 项目格式(format_version 1)
```
mod.toml          [mod] name/version/description/game_build/ship/format_version; [install] mode = loose | sdk
edits/NNN_<cmd>.json   每条编辑命令: argv、时间、工具版本、输入 sha256、遮罩/统计、输出文件
assets/res_mods/...    改后的资源(sdk 模式在 PnFMods/<Mod>/ 下, 允许保留 SDK 占位文件)
.install/         manifest.json(已安装清单)、backup/、history/ —— 不入版本库
```
### 9.5 install --dev / uninstall
- 安装时自动: 占位文件从游戏包补齐(同名多份优先 content/gameplay/)、BC7 去 prep、补齐 .ddN(并检查 K 与原版一致)、sdk 模式补 PnFModsLoader.py。
- 已存在的文件先备份; 清单逐条写入、写后校验。卸载只删哈希一致的文件, 还原备份, 删除自己创建的空目录。

## 10. M3: geometry 编码与模型编辑

- **映射名**:id = murmur3(名字);`X`→`XShape`,已以 `Shape<数字>` 结尾的原样使用。LOD 族名 = 去掉 `Shape\d+`、`_lodN`、结尾的 `_lod`。
- **编码**:按模板渲染集的顶点格式打包(法线/切线 4 字节压缩、UV half/float、骨骼字节取逐顶点模板)。索引按原编码方式;装甲/碰撞块原样保留;没有名字的映射原样拷贝。
- **空间**:船体渲染集多数是节点局部空间,MidBack 烟囱 3 个是蒙皮世界空间(Root/Chimney)。部件变换:骨骼只吸收平移和旋转,缩放写进顶点;世界空间渲染集顶点乘 T,其余乘 W·T·W'⁻¹。
- **挂点**:move 用 W'=W·T(分段空间);scale/rotate 用 L'=Tr(−p)·Op·Tr(p)·L。港口证实引擎会采用挂点矩阵里的缩放。
- **LOD**:编辑作用于同一 geometry 的所有 LOD 族;Blender 导回时,改动族的 lod1–3 指向 LOD0 网格;lod4 不动。
- **编辑记录**:`edits/NNN_model_*.json`(参数可重放 + 改前/改后 sha256 + `.undo` 备份),提供 undo / replay。
- **港口验收脚本**:`scripts/ingame/m3_game.ps1`。
