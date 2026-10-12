# learn.makers.emissive-pnf: 发光与 PnF
上级: learn.makers
下级: 无
---
**发光**: `.mfm` 把 `PBS_ship.fx` → `PBS_ship_emissive.fx`(skinned 对 skinned), 加 `emissivePower`(约 0–5, 官方太空皮常见 ~0.5; 我们默认偏保守)。发光色 = `_a` 该处颜色。
**`_mg` 语义切换**(莱服官方皮肤帖要点):
- 普通皮肤: B≈迷彩可涂区(粉/蓝会被迷彩盖)。
- emissive 开后: B≈发光遮罩(要发光处写白/高值, 其余清蓝)。**不能同时**「要官方迷彩叠在上」又「要自绘发光」——社区选其一, 或卸永久迷彩。
**强度策略**: 大面积用低 B(微光), 小件/徽记才满 B; 流萤拆解: 船体 B max≈49/255。本工具 `glow --power` 见 `ref.glow`。
**PnF 安装**: `bin/<最大数字>/res_mods/` 下 `PnFModsLoader.py`(可空) + `PnFMods/<mod>/Main.py` 内 `API_VERSION` + `contentSdk.registerShipMod('船ID')`。浩舰/SEA-group README 同此。更新后 res_mods 会被清, 需备份迁目录(`ref.migrate`)。
**对本工具**: `glow` 会改 `_mg` B、切 mfm、改 visual 标识为 `SHIPMAT_EMISSIVE_*`; 层 5 验收港口夜景亮度。
