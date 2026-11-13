# ref.glow: 发光步骤与能力边界
上级: ref
下级: 无
---
**机制**：支持的船材质用 `_mg` B 作发光遮罩，mfm 需 emissive shader，visual 材质 identifier 也需匹配；发光色来自漫反射。普通迷彩与发光对 B 的用途不同，不要给指定发光区最后再清零。

**先确认选区与材质**：`wowsmod model list --project <项目>` 查分段/材质，必要时导出 glb；用 mask 叠加图看范围，不能照抄另一条船的分段名。
```
wowsmod mask --glb <分段.glb> --material <材质正则> --like <贴图_a.dds> -o mask.png --overlay check.png
wowsmod glow --project <项目> --mg <贴图_mg.dds> --diffuse <贴图_a.dds> --color "#2E9BFF" --mask mask.png --mfm <材质名> --power <本项目试验值>
wowsmod render <项目> --compare
```
PowerShell 的 `#` 色号必须加引号。先用局部遮罩小样，记录遮罩强度、power、色号与 look；由参考和昼夜可读性调整，不把某个 power 当通用安全值。

**已验证/实验/未知**：历史原船材质有港口成功记录，适用范围依船/构建；工作台提亮只证明预览。自定义外带件的节点、identifier、法线/几何可能影响发光，C2 中翅膀与光环结果不一致，尚无通用修复。不能默认改为 Hull 节点就有效；逐件核验装配、文件、注册、日夜多角画面，未做写未验证。历史数值与限制见 `workflow.changelog`。
