# learn.makers.texture: 贴图痛船
上级: learn.makers
下级: 无
---
**社区做法**: 解包或 ModsSDK 取出船体/上层/炮 `_a/_mg/_n`(偶有 `_ao`) → 在**原 UV 布局**上重画 → DDS 写回同名同尺寸路径 → `res_mods` 覆盖或进 PnF 项目。
**通道**:
- `_a`: 主视觉(没有迷彩时看到的样子); 主题纹样/徽章/插画级色块画在这里。
- `_mg`: 金属/光泽 + **B 通道管迷彩遮罩**(粉/蓝区会被官方迷彩盖住; 自定义 Logo/图案区要把 B 清掉或改成非迷彩色, 否则进港一穿迷彩就糊掉)。开 emissive 后 B 改义为发光遮罩(见 `learn.makers.emissive-pnf`)。
- `_n`: 法线, 一般少动; `_ao` 遮蔽可选。
**路径**(旧文常见 `content/gameplay/<国>/ship/<舰种>/textures`; 现多用 SDK 船目录下同名 dds)。国别文件夹名大小写按客户端。
**分辨率**: 船体常见 4096²; **务必带完整 mip**(`.dds`+分级或工具生成)。社区包无 mip 会远处闪烁。
**对本工具**: `import-sdk` → `ship-color` / `tex recolor` / `bake-ref` / `tex composite` → 迷彩区 `tex channel … b --set 0` → 层 4 完成标准: 关发光仍是那套涂装。命令细节 `ref.recolor` / `workflow.layers.paint`。
**主题**: 角色只贡献配色与纹样; 在官方 UV 上画, 不另展 UV(除非走模型改 UV 流程, 见莱服 UV FAQ)。
**作者实践**: 茄宝洋芋公开直播教学= **PS 分层画官方 UV**(底板 `_a` + 字牌/立绘圆标/色带, 组与混合模式)→ DDS → PnFMods; 夙岚同属 UV 主笔(笔刷流线或大色块)。见 `e2e/learn/C_bili_makers.md`。
