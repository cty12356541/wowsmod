# ref.model.animate: 动画物件(model animate)
上级: ref.model
下级: 无
---
`wowsmod model animate <项目> --section Stern --like DeckHouse --mesh x.glb --preset spin|orbit|float --at x,y,z [--rpm 6 --axis vertical|x|z --radius --orbit-rpm --phase --float-amp --period --height --glow-mat 名 --b-max --all-lods]`
产出 `misc/<名>.{geometry,visual,model,seq,ssm}` + 材质, 并把 .ssm 挂到分段 .model 的 `<animations>`。轨道按每 1/4 圈打关键帧、切线=斜率, 线性/Hermite 插值都匀速。核对: `model anim-sample <seq>` 逐秒打印位置/角度。工作台装配时把动画物件放进场景并循环播放(帧采样 ≤240, 超过 60 s 的序列只播前 60 s)。
未实证(待港口): 欧拉角单位/顺序、分段 .model 是否接受 SDK 格式的 `<animations>`; 不行再改原生格式。
