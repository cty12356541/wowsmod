# workflow.layers.ref.teardown: 拆原 mod 包(只读)
上级: workflow.layers.ref
下级: 无
---
**目标**: 只读拆出原包做了哪些改动。**输入**: 公开 mod 包(不安装, 不复制进仓库)。**完成标准**: 改动文件清单 + 每类改动对应到层 3–6。
`python scripts/ref/pack_teardown.py <包目录> --sdk <官方 SDK 船目录> --out teardown_auto.md`, 再人工写 TEARDOWN.md 回答:
①加载方式与安装说明 ②替换/新增文件(整船重导出还是只换贴图) ③额外模型挂法: 分段 visual 新渲染集 / 独立 .model 经 `<animations>`→.ssm→.seq / 挂点; 尺寸位置 ④LOD 与 crack/patch 是否保留 ⑤贴图尺寸/格式/mip, 是否官方 UV ⑥发光: 着色器、emissivePower、`_mg` B 最大值与占比 ⑦材质标识是否改名 ⑧特效/粒子 ⑨目标服适用性 ⑩垃圾文件。
原包只作拆解与最终对照: 不装进游戏、不拷成品进项目、不进仓库。
