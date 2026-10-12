//! 模型动画: .ssm(状态机)/.seq(序列)的生成、解析与采样。
//!
//! 结构(来自对公开 mod 的只读拆解, 引擎语义未全部实证):
//! - 分段 `.model` 的 `<animations><animation>…ssm</animation></animations>` 引用状态机;
//! - `.ssm`: `<params>`(触发器 no_action / idle)+ `<node>`(sequence 节点: resource=.seq, isLoop, transition);
//! - `.seq`: `<duration>`(秒) + 多个 `<sequenceObject>`; 第一个 = 使用该 seq 的模型(Root), 其余是被驱动的模型
//!   (resource=.model), 轨道 position / rotationEuler / scale(vector3 关键帧, 带 inTangent/outTangent)、
//!   visibility(bool)、attach(挂到另一个对象, 在 [startTime, startTime+duration] 内跟随其变换)。
//!
//! 采样约定(本工具的假设, 需港口实证): vector3 关键帧之间用 Hermite 插值, 切线单位 = 值/秒;
//! rotationEuler 单位为度, 分量 = (roll 绕 z, pitch 绕 x, yaw 绕 y 竖直), 即原包注释里"绕 Z 轴(垂直方向)"。
//! 生成时用"逐段关键帧 + 切线 = 斜率", 这样不论引擎按 Hermite 还是线性插值, 结果都一致(匀速)。
use crate::error::{Error, Result};
use crate::visual::{compose, mat_mul, parse_xml, Elem, M4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub t: f64,
    pub v: [f64; 3],
    pub tin: [f64; 3],
    pub tout: [f64; 3],
}

#[derive(Debug, Clone, Default)]
pub struct SeqObject {
    pub name: String,
    pub resource: String,
    pub position: Vec<Key>,
    pub rotation: Vec<Key>,
    pub scale: Vec<Key>,
    pub visibility: Vec<(f64, bool)>,
    /// (开始, 持续, 目标对象名)
    pub attach: Vec<(f64, f64, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct Seq {
    pub duration: f64,
    pub objects: Vec<SeqObject>,
}

fn txt(e: Option<&Elem>, src: &str) -> String {
    e.map(|x| x.text(src)).unwrap_or_default()
}
fn v3(s: &str) -> [f64; 3] {
    let f: Vec<f64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    [f.first().copied().unwrap_or(0.0), f.get(1).copied().unwrap_or(0.0), f.get(2).copied().unwrap_or(0.0)]
}

pub fn parse_seq(src: &str) -> Result<Seq> {
    let root = parse_xml(src)?;
    let duration: f64 = txt(root.child("duration"), src).parse().map_err(|_| Error::format("seq", "duration 不是数字"))?;
    let mut objects = Vec::new();
    for so in root.children_named("sequenceObject") {
        let mut o = SeqObject { name: txt(so.child("name"), src), resource: txt(so.child("resource"), src), ..Default::default() };
        for tr in so.children_named("track") {
            let name = txt(tr.child("name"), src);
            for fr in tr.children_named("frame") {
                let t: f64 = txt(fr.child("startTime"), src).parse().unwrap_or(0.0);
                let val = txt(fr.child("value"), src);
                match name.as_str() {
                    "position" | "rotationEuler" | "scale" => {
                        let k = Key { t, v: v3(&val), tin: v3(&txt(fr.child("inTangent"), src)), tout: v3(&txt(fr.child("outTangent"), src)) };
                        match name.as_str() {
                            "position" => o.position.push(k),
                            "rotationEuler" => o.rotation.push(k),
                            _ => o.scale.push(k),
                        }
                    }
                    "visibility" => o.visibility.push((t, val.trim().eq_ignore_ascii_case("true"))),
                    "attach" => o.attach.push((t, txt(fr.child("duration"), src).parse().unwrap_or(duration), val.trim().to_string())),
                    _ => {}
                }
            }
        }
        objects.push(o);
    }
    if objects.is_empty() {
        return Err(Error::format("seq", "没有 sequenceObject"));
    }
    Ok(Seq { duration, objects })
}

/// Hermite 采样; 区间外取端点值
pub fn sample_keys(keys: &[Key], t: f64, default: [f64; 3]) -> [f64; 3] {
    match keys.len() {
        0 => return default,
        1 => return keys[0].v,
        _ => {}
    }
    if t <= keys[0].t {
        return keys[0].v;
    }
    let last = keys[keys.len() - 1];
    if t >= last.t {
        return last.v;
    }
    let i = keys.windows(2).position(|w| t >= w[0].t && t < w[1].t).unwrap_or(0);
    let (a, b) = (keys[i], keys[i + 1]);
    let dt = (b.t - a.t).max(1e-9);
    let s = (t - a.t) / dt;
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    std::array::from_fn(|c| h00 * a.v[c] + h10 * dt * a.tout[c] + h01 * b.v[c] + h11 * dt * b.tin[c])
}

/// rotationEuler(度; x=roll 绕 z, y=pitch 绕 x, z=yaw 绕 y)→ 游戏坐标旋转(度, 绕 x,y,z), 供 compose 用
pub fn euler_to_game(e: [f64; 3]) -> [f64; 3] {
    [e[1], e[2], e[0]]
}

/// 采样某对象在时刻 t 的局部变换(相对 Root)与可见性; 处理 attach(跟随目标对象)
pub fn sample_object(seq: &Seq, idx: usize, t: f64) -> (M4, bool) {
    fn local(o: &SeqObject, t: f64) -> M4 {
        let p = sample_keys(&o.position, t, [0.0; 3]);
        let r = sample_keys(&o.rotation, t, [0.0; 3]);
        let s = sample_keys(&o.scale, t, [1.0; 3]);
        compose(p, euler_to_game(r), s)
    }
    let o = &seq.objects[idx];
    let mut m = local(o, t);
    for (st, du, target) in &o.attach {
        if t >= *st && t <= st + du {
            if let Some(ti) = seq.objects.iter().position(|x| &x.name == target) {
                if ti != idx {
                    m = mat_mul(&m, &sample_object(seq, ti, t).0);
                }
            }
        }
    }
    let vis = o.visibility.iter().filter(|(vt, _)| *vt <= t).last().map(|x| x.1).unwrap_or(true);
    (m, vis)
}

// ---------------------------------------------------------------- 生成

#[derive(Debug, Clone)]
pub struct AnimSpec {
    /// 卫星模型的位置(相对 Root 模型, 游戏坐标)
    pub at: [f64; 3],
    /// 自转: 每分钟转数(正 = 从上往下看逆时针), 轴 0=roll 1=pitch 2=yaw(竖直)
    pub spin_rpm: f64,
    pub spin_axis: usize,
    /// 环绕: 半径(>0 启用)、每分钟圈数; 圆心 = at, 卫星初始在圆心 +x 方向(相位 phase_deg 绕竖直轴)
    pub orbit_radius: f64,
    pub orbit_rpm: f64,
    pub phase_deg: f64,
    /// 上下浮动: 振幅(>0 启用)与周期(秒)
    pub float_amp: f64,
    pub float_period: f64,
    /// 初始姿态(rotationEuler 度)与缩放
    pub base_rot: [f64; 3],
    pub scale: f64,
}

fn f3(v: [f64; 3]) -> String {
    format!("{:.6} {:.6} {:.6}", v[0], v[1], v[2])
}

fn track_v3(name: &str, keys: &[Key], ind: &str) -> String {
    let mut s = format!("{ind}<track>\n{ind}\t<name>\t{name}\t</name>\n{ind}\t<type>\tvector3\t</type>\n");
    for k in keys {
        s += &format!(
            "{ind}\t<frame>\n{ind}\t\t<startTime>\t{:.4}\t</startTime>\n{ind}\t\t<value>\t{}\t</value>\n{ind}\t\t<inTangent>\t{}\t</inTangent>\n{ind}\t\t<outTangent>\t{}\t</outTangent>\n{ind}\t\t<tangentMode>\t0\t</tangentMode>\n{ind}\t</frame>\n",
            k.t,
            f3(k.v),
            f3(k.tin),
            f3(k.tout)
        );
    }
    s + &format!("{ind}</track>\n")
}

/// 匀速旋转关键帧: 每 1/4 圈一帧, 切线 = 角速度(度/秒), 覆盖 [0, duration]
fn spin_keys(base: [f64; 3], axis: usize, rpm: f64, duration: f64) -> Vec<Key> {
    let w = rpm * 6.0; // 度/秒
    let mut tan = [0.0; 3];
    tan[axis] = w;
    if rpm.abs() < 1e-9 {
        return vec![Key { t: 0.0, v: base, tin: [0.0; 3], tout: [0.0; 3] }];
    }
    let step = (15.0 / rpm.abs()).min(duration); // 1/4 圈的秒数
    let n = (duration / step).ceil().max(1.0) as usize;
    (0..=n)
        .map(|i| {
            let t = (i as f64 * step).min(duration);
            let mut v = base;
            v[axis] += w * t;
            Key { t, v, tin: tan, tout: tan }
        })
        .collect()
}

/// 正弦浮动(竖直 y): 每 1/4 周期一帧, 切线 = 导数
fn float_keys(base: [f64; 3], amp: f64, period: f64, duration: f64) -> Vec<Key> {
    let w = 2.0 * std::f64::consts::PI / period;
    let n = ((duration / period) * 4.0).round().max(4.0) as usize;
    (0..=n)
        .map(|i| {
            let t = (i as f64 * period / 4.0).min(duration);
            let mut v = base;
            v[1] += amp * (w * t).sin();
            let d = [0.0, amp * w * (w * t).cos(), 0.0];
            Key { t, v, tin: d, tout: d }
        })
        .collect()
}

/// 选一个能让各周期都闭合的时长(秒): 各周期的最小公倍数(近似, 上限 600 s)
pub fn loop_duration(periods: &[f64]) -> f64 {
    let ps: Vec<f64> = periods.iter().copied().filter(|p| p.is_finite() && *p > 1e-6).collect();
    if ps.is_empty() {
        return 60.0;
    }
    let base = ps.iter().cloned().fold(0.0, f64::max);
    for k in 1..=600 {
        let d = base * k as f64;
        if d > 600.0 {
            break;
        }
        if ps.iter().all(|p| ((d / p) - (d / p).round()).abs() < 0.02) {
            return d;
        }
    }
    base
}

/// 生成 .seq 文本。`model_res`: 卫星 .model 资源路径; `center_res`: 圆心对象用的(隐藏)模型资源
pub fn seq_text(stem: &str, model_res: &str, center_res: &str, sp: &AnimSpec) -> (String, f64) {
    let mut periods = vec![];
    if sp.spin_rpm.abs() > 1e-9 {
        periods.push(60.0 / sp.spin_rpm.abs());
    }
    if sp.orbit_radius > 0.0 && sp.orbit_rpm.abs() > 1e-9 {
        periods.push(60.0 / sp.orbit_rpm.abs());
    }
    if sp.float_amp > 0.0 {
        periods.push(sp.float_period);
    }
    let dur = loop_duration(&periods);
    let ind = "\t\t";
    let obj = |name: &str, res: &str, body: &str| -> String {
        format!("\t<sequenceObject>\n\t\t<name>\t{name}\t</name>\n\t\t<type>\tmodel\t</type>\n\t\t<editorOnly>\n\t\t\t<activated>\ttrue\t</activated>\n\t\t\t<solo>\tfalse\t</solo>\n\t\t</editorOnly>\n\t\t{}\n{body}\t</sequenceObject>\n",
            if res.is_empty() { "<resource />".to_string() } else { format!("<resource>\t{res}\t</resource>") })
    };
    let still = |v: [f64; 3]| vec![Key { t: 0.0, v, tin: [0.0; 3], tout: [0.0; 3] }];
    let vis = |b: bool| format!("{ind}<track>\n{ind}\t<name>\tvisibility\t</name>\n{ind}\t<type>\tbool\t</type>\n{ind}\t<frame>\n{ind}\t\t<startTime>\t0.0000\t</startTime>\n{ind}\t\t<value>\t{b}\t</value>\n{ind}\t</frame>\n{ind}</track>\n");
    let mut s = format!("<{stem}.seq>\n\t<duration>\t{dur:.4}\t</duration>\n");
    s += &obj("Root", "", "\t\t<track />\n");
    let orbit = sp.orbit_radius > 0.0;
    // 卫星自身: 位置(环绕时相对圆心)、自转、浮动
    let mut sat_pos = if orbit { [sp.orbit_radius, 0.0, 0.0] } else { sp.at };
    if orbit {
        let ph = sp.phase_deg.to_radians();
        sat_pos = [sp.orbit_radius * ph.cos(), 0.0, -sp.orbit_radius * ph.sin()];
    }
    let pos_keys = if sp.float_amp > 0.0 { float_keys(sat_pos, sp.float_amp, sp.float_period, dur) } else { still(sat_pos) };
    let rot_keys = spin_keys(sp.base_rot, sp.spin_axis, sp.spin_rpm, dur);
    if orbit {
        let mut body = track_v3("position", &still(sp.at), ind);
        body += &track_v3("rotationEuler", &spin_keys([0.0; 3], 2, sp.orbit_rpm, dur), ind);
        body += &track_v3("scale", &still([1.0; 3]), ind);
        body += &vis(false);
        s += &obj("center", center_res, &body);
    }
    let mut body = track_v3("position", &pos_keys, ind);
    body += &track_v3("rotationEuler", &rot_keys, ind);
    body += &track_v3("scale", &still([sp.scale; 3]), ind);
    body += &vis(true);
    if orbit {
        body += &format!("{ind}<track>\n{ind}\t<name>\tattach\t</name>\n{ind}\t<type>\tattach\t</type>\n{ind}\t<frame>\n{ind}\t\t<startTime>\t0.0000\t</startTime>\n{ind}\t\t<duration>\t{dur:.4}\t</duration>\n{ind}\t\t<value>\tcenter\t</value>\n{ind}\t</frame>\n{ind}</track>\n");
    }
    s += &obj("satellite", model_res, &body);
    s += &format!("\t<events />\n</{stem}.seq>\n");
    (s, dur)
}

/// 生成 .ssm 文本: no_action / idle 两个触发器, 两个 sequence 节点都循环播放同一个 seq(进港即动)
pub fn ssm_text(stem: &str, seq_res: &str) -> String {
    let tr = |to: &str, blend: f64, trig: &str| format!("\t\t<transition>\n\t\t\t<to>\t{to}\t</to>\n\t\t\t<blendDuration>\t{blend:.4}\t</blendDuration>\n\t\t\t<allowRootMotion>\tfalse\t</allowRootMotion>\n\t\t\t<conditions>\n\t\t\t\t<condition>\n\t\t\t\t\t<type>\ttrigger\t</type>\n\t\t\t\t\t<name>\t{trig}\t</name>\n\t\t\t\t</condition>\n\t\t\t</conditions>\n\t\t</transition>\n");
    let node = |name: &str, blend: f64| format!("\t<node>\n\t\t<name>\t{name}\t</name>\n\t\t<type>\tsequence\t</type>\n\t\t<resource>\t{seq_res}\t</resource>\n\t\t<isLoop>\ttrue\t</isLoop>\n\t\t<loop>\t-1\t</loop>\n{}{}\t</node>\n", tr("no_action", blend, "no_action"), tr("loop", blend, "idle"));
    format!(
        "<{stem}.ssm>\n\t<params>\n\t\t<param>\n\t\t\t<name>\tno_action\t</name>\n\t\t\t<type>\ttrigger\t</type>\n\t\t</param>\n\t\t<param>\n\t\t\t<name>\tidle\t</name>\n\t\t\t<type>\ttrigger\t</type>\n\t\t</param>\n\t</params>\n{}{}</{stem}.ssm>\n",
        node("no_action", 0.0),
        node("loop", 0.3)
    )
}

/// 解析 .ssm, 返回各 sequence 节点引用的 .seq 资源(去重, 保持顺序)
pub fn ssm_sequences(src: &str) -> Result<Vec<String>> {
    let root = parse_xml(src)?;
    let mut out: Vec<String> = Vec::new();
    for n in root.children_named("node") {
        let r = txt(n.child("resource"), src);
        if !r.is_empty() && !out.contains(&r) {
            out.push(r);
        }
    }
    Ok(out)
}

/// .model 里 `<animations>` 引用的 .ssm(兼容原包里同时出现 `<animations>…</animations>` 与 `<animations />`)
pub fn model_animations(src: &str) -> Vec<String> {
    let Ok(root) = parse_xml(src) else { return vec![] };
    root.children_named("animations").flat_map(|a| a.children_named("animation").map(|x| x.text(src)).collect::<Vec<_>>()).filter(|s| !s.is_empty()).collect()
}

/// 往 .model 文本里加一个 `<animation>`(已有则不重复); 返回新文本
pub fn model_add_animation(src: &str, ssm_res: &str) -> Result<String> {
    if model_animations(src).iter().any(|a| a == ssm_res) {
        return Ok(src.to_string());
    }
    let root = parse_xml(src)?;
    let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("\t\t<animation>\t{ssm_res}\t</animation>{nl}");
    // 已有非空 <animations>: 插在结束标签前
    if let Some(a) = root.children_named("animations").find(|a| a.inner.1 > a.inner.0) {
        let pos = a.inner.1;
        let before = src[..pos].trim_end_matches(['\t', ' ']);
        return Ok(format!("{before}{}{line}\t{}", if before.ends_with('\n') { "" } else { nl }, &src[pos..]));
    }
    // 空 <animations /> 替换
    if let Some(a) = root.children_named("animations").next() {
        return Ok(format!("{}<animations>{nl}{line}\t</animations>{}", &src[..a.start], &src[a.end..]));
    }
    // 没有: 插在根元素结束前
    let pos = root.inner.1;
    Ok(format!("{}\t<animations>{nl}{line}\t</animations>{nl}{}", &src[..pos], &src[pos..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> AnimSpec {
        AnimSpec { at: [1.0, 0.5, -2.0], spin_rpm: 10.0, spin_axis: 2, orbit_radius: 0.0, orbit_rpm: 0.0, phase_deg: 0.0, float_amp: 0.0, float_period: 4.0, base_rot: [0.0; 3], scale: 1.0 }
    }
    #[test]
    fn spin_is_uniform() {
        let (t, d) = seq_text("a", "PnFMods/M/S/misc/a.model", "", &spec());
        assert!((d - 6.0).abs() < 1e-9);
        let s = parse_seq(&t).unwrap();
        assert_eq!(s.objects.len(), 2);
        for k in 0..=12 {
            let tt = k as f64 * 0.5;
            let r = sample_keys(&s.objects[1].rotation, tt, [0.0; 3]);
            assert!((r[2] - 60.0 * tt).abs() < 1e-3, "t={tt} yaw={}", r[2]);
        }
        let (m, vis) = sample_object(&s, 1, 0.0);
        assert!(vis);
        assert!((m[3][0] - 1.0).abs() < 1e-9 || (m[0][3] - 1.0).abs() < 1e-9);
    }
    #[test]
    fn orbit_float_and_attach() {
        let mut sp = spec();
        sp.spin_rpm = 0.0;
        sp.orbit_radius = 0.5;
        sp.orbit_rpm = 6.0;
        sp.float_amp = 0.1;
        sp.float_period = 5.0;
        let (t, d) = seq_text("o", "m", "c", &sp);
        assert!((d - 10.0).abs() < 1e-9, "{d}");
        let s = parse_seq(&t).unwrap();
        assert_eq!(s.objects.len(), 3);
        assert_eq!(s.objects[1].visibility, vec![(0.0, false)]);
        assert_eq!(s.objects[2].attach[0].2, "center");
        // 浮动: 1/4 周期处 +振幅
        let p = sample_keys(&s.objects[2].position, 1.25, [0.0; 3]);
        assert!((p[1] - 0.1).abs() < 1e-6);
        // 与圆心的距离恒为半径(水平面)
        for k in 0..10 {
            let (m, _) = sample_object(&s, 2, k as f64);
            let (c, _) = sample_object(&s, 1, k as f64);
            let pick = |m: &M4| if m[3][0].abs() + m[3][2].abs() > 1e-12 { [m[3][0], m[3][2]] } else { [m[0][3], m[2][3]] };
            let (a, b) = (pick(&m), pick(&c));
            let r = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
            assert!((r - 0.5).abs() < 1e-6, "t={k} r={r}");
        }
    }
    #[test]
    fn ssm_and_model_hook() {
        let x = ssm_text("a", "PnFMods/M/S/misc/a.seq");
        assert_eq!(ssm_sequences(&x).unwrap(), vec!["PnFMods/M/S/misc/a.seq".to_string()]);
        let m = "<S.model>\n\t<castsShadow>\ttrue\t</castsShadow>\n</S.model>\n";
        let m2 = model_add_animation(m, "P/a.ssm").unwrap();
        assert_eq!(model_animations(&m2), vec!["P/a.ssm".to_string()]);
        let m3 = model_add_animation(&m2, "P/b.ssm").unwrap();
        assert_eq!(model_animations(&m3), vec!["P/a.ssm".to_string(), "P/b.ssm".to_string()]);
        assert_eq!(model_add_animation(&m3, "P/b.ssm").unwrap(), m3);
        let e = "<S.model>\n\t<animations />\n</S.model>";
        assert_eq!(model_animations(&model_add_animation(e, "P/c.ssm").unwrap()), vec!["P/c.ssm".to_string()]);
        parse_xml(&m3).unwrap();
    }
}
