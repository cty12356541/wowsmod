# M2 报告(进行中 — 断点续作记录)

## 进度(最新在上)
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
