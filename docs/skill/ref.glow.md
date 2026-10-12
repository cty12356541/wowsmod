# ref.glow: 发光命令
上级: ref
下级: 无
---
原理:`_mg` 的 B 通道作为发光遮罩/强度;同时把对应 .mfm 切到 emissive 着色器,并把 .visual 里引用它的材质标识改成 `SHIPMAT_EMISSIVE_PBS_*`(不改标识引擎就不会用发光着色器)。
发光颜色等于该处漫反射颜色,所以 `--diffuse` 加 `--color` 会顺带把发光区染色。

## 步骤(示例: 某战列舰烟囱, 已在港口实测; 名字按你的船替换)
```
wowsmod export "content/gameplay/<国家>/ship/<舰种>/<船体模型>/<船体模型>_MidBack.geometry" -o <工作目录>/MidBack.glb
wowsmod mask --glb <工作目录>/MidBack.glb --material DeckHouse_skinned --like <船体模型>_DeckHouse_a.dds -o m.png --overlay check.png   # 看选区
wowsmod glow --project <项目> --mg <船体模型>_DeckHouse_mg.dds --diffuse <船体模型>_DeckHouse_a.dds --color #2E9BFF --glb <工作目录>/MidBack.glb --material DeckHouse_skinned --mfm <船体模型>_DeckHouse_skinned --power 0.25
```
- 示例船的烟囱在 MidBack 分段、材质 `DeckHouse_skinned`; 别的船分段与材质名不同。
- 其他船:用 `wowsmod find "*<船体模型>*.geometry"` 找分段,用 `wowsmod model list --project <项目>` 看渲染集和材质,再用 mask 叠加图确认选区。具体哪个部件对应哪种材质要实际查看,**不要猜**。
- `--power`:emissivePower,0.25 已经很亮;官方太空皮肤用的是 0.5。
- 如果之前做过 recolor,DeckHouse 的 `_mg` B 不要清零,glow 会自己重写。
- 检查:`wowsmod render <项目> --compare`(发光区在图里会提亮),最终以进港效果为准。
