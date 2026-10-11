# workflow.layers.shape.refine.check: 穿模/面数/远近可读
上级: workflow.layers.shape.refine
下级: 无
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
**炮塔外带件**(r31, 每类炮塔都过):
- 挂点随转: 用 `model import --part <HP_…|型号> --like Turret`, 装配后壳材质应出现在该 HP 网格里; 不挂船体。
- 炮口/仰角: 前方炮门与炮管全露; 0→vertSector 最大仰角炮管不碰壳。
- 贴合: 按实测炮塔平面轮廓(去炮管)外扩, 间隙中位 ≈0.02–0.04、外伸 ≤0.04(模型单位, 参考); 超椭圆外扩/喇叭口会读成托盘。
- 回转: 射界内(GameParams horizSector)每 10° 查壳 vs 船体分段/邻炮塔本体+炮管/邻壳; 只计原炮塔本体同姿态不撞的(扣基线)。后排炮管掠过前炮塔顶时不加顶板。
- 造型: 顶板/侧裙/前盾分件, 件缝+倒角; 金/发光只点缀(徽记、短条), 不做沿边整圈条纹。
- 工具: `turret_check.py`(数值), `turret_pose_render.py`(转 90°+最大仰角姿态图; box 预览装配几何无 yaw 骨骼, `turret=` 不生效), `turret_shots.sh`(game-day 近景)。
**apply 后自检**(r32, 必过才可拍/汇报): `scripts/model/robin/verify_apply.ps1 -Proj <dir> -Spec <props_spec>` 退出码=失败数, 须 0 —— 每个件都出现在非 misc 的已加载 `.visual`(`<vertices>` 名匹配)且 mfm 为 emissive fx; Hull/DeckHouse mfm 为 emissive; 任一 `.model` 带 `.ssm` 即告警(动画件可能只挂了骨骼动画、网格不在可见 visual)。再进港口拍一张确认件在画面里, 夜港(黑桃城)确认发光。r32 根因: `model animate` 只把 ssm 挂到船 `.model`, 网格在独立 misc visual 未被加载 → 游戏里翅膀消失, 但 apply 报 ok=True 未拦; 修前先用静态 `model import`。
**完成标准**: 清单全勾; 失败件回 model 修, 不进 accept。
**脚本**: `mesh_budget.py`(已固化); 炮塔件用 `turret_check.py` 自动碰撞; 其余穿模以出图目视为主。
**预览光照**: 工作台 STUDIO 灰光会把乳白/淡紫读成灰、发光不可见 —— 灰光预览只用于剪影/布局, 色彩审美以港口实拍或校准后的预览为准(预览↔港口校准另见对应节点)。
