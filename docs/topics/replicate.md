# replicate: 定制复刻(有参考图/视频)

目标: 让游戏里的船和参考图"同角度看起来像"。先读 `wowsmod help design`(分区表、发光规则、自检清单)。

## 1 参考拆解(写成文字, 不动文件)
多张/多角度截图逐张记: 底色、块面色(亮度级差)、发光色(常偏白青, 不是饱和青)、发光在哪(舷侧/炮塔/炮管/上层建筑棱线/甲板边)、线走向(斜切/沿船长/环形)、各部件分区(见 design 分区表)。水线以下看不见的不做。

## 2 同角度对齐
`wowsmod render <项目> --engine soft --view side --width 4096 --json -o base.png`
JSON 的 `frame` 给世界→像素映射(`像素 = origin_px + Y*y_axis_px + Z*z_axis_px`)。在 base.png 上画世界网格, 量出水线(y≈0)、舷高、炮塔与炮管的 z/y 位置。

## 3 设计图(同框、按世界坐标画)
按参考在与 base.png 同尺寸的透明 PNG 上画: 块面用一种键色(如 #808080), 发光线/环用另一种(#00FFFF)。形状按参考的走向画, 不要用规则网格条纹。

## 4 投影进 UV(贴合结构)
- 舷侧: `wowsmod bake-ref <项目> --ref design.png --material "Hull$|Hull_skinned$" --like <Hull_a> --view side --width 4096 --mirror-x --key "#00FFFF:0.2=lines.png" --key "#808080:0.1=blocks.png" -o ref_uv.png`(左右舷 UV 不共用时必须 --mirror-x)
- 上层建筑棱线: `wowsmod lineart <项目> --material DeckHouse --like <DeckHouse_a> --crease 70 --width 3 --box=x0,y0,z0,x1,y1,z1`(覆盖率 2–4% 为宜; >7% 太花)
- 按世界位置选整块部件(如主炮炮管): `wowsmod lineart … --material FGM049 --fill --box=…`
- 炮塔侧面图案: 同样 bake-ref, `--material FGM049`(所有同型炮塔共用 UV)

## 5 分部件应用(每步一条命令, 都记进 edits/)
底色 `tex recolor <_a> --to-color` → 块面 `tex recolor <_a> --mask blocks.png` → 清 `_mg` B(`tex channel … --channel b --set 0`)→ `glow --mask lines.png --mfm <部件>`。同一 _mg 多个遮罩要一次给齐(多个 --mask 取并集), 否则后一次 glow 会清掉前一次。

## 6 对比与迭代
同角度 `render --engine soft --yaw 35 --pitch 14` 与参考并排; 装进游戏进港用固定机位截图再并排。每轮列出差距(形状/亮度/缺失元素)只改一两项。
停止条件(任一满足即停): 参考里列出的元素都已出现且位置大体一致; 或已迭代 3 轮; 或剩余差距需要工具做不到的能力(写进报告)。
