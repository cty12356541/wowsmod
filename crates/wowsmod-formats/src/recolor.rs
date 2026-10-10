//! 贴图换色 / 遮罩 / 发光通道编辑(纯 CPU)。
//!
//! 换色在 OKLab 感知均匀空间里做: 目标色模式下把选区的"平均颜色"平移到目标色,
//! 而每个像素相对平均值的亮度起伏(细节、脏污、阴影、接缝)按比例保留, 不是填纯色。
use crate::color::*;
use crate::texture::Image;
use rayon::prelude::*;

/// 单通道浮点遮罩(0..1), 与贴图同尺寸。
#[derive(Clone, Debug)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub v: Vec<f32>,
}

impl Mask {
    pub fn filled(width: u32, height: u32, val: f32) -> Mask {
        Mask { width, height, v: vec![val; (width * height) as usize] }
    }
    /// 从图像的 R 通道(若有透明度则乘以 A)取遮罩, 双线性缩放到目标尺寸。
    pub fn from_image(img: &Image, width: u32, height: u32) -> Mask {
        let src: Vec<f32> = img.px.chunks_exact(4).map(|p| p[0] as f32 / 255.0 * (p[3] as f32 / 255.0)).collect();
        let m = Mask { width: img.width, height: img.height, v: src };
        m.resized(width, height)
    }
    pub fn resized(&self, width: u32, height: u32) -> Mask {
        if width == self.width && height == self.height {
            return self.clone();
        }
        let mut out = Mask::filled(width, height, 0.0);
        let sx = self.width as f32 / width as f32;
        let sy = self.height as f32 / height as f32;
        out.v.par_chunks_mut(width as usize).enumerate().for_each(|(y, row)| {
            let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (self.height - 1) as f32);
            let y0 = fy.floor() as u32;
            let y1 = (y0 + 1).min(self.height - 1);
            let ty = fy - y0 as f32;
            for (x, o) in row.iter_mut().enumerate() {
                let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (self.width - 1) as f32);
                let x0 = fx.floor() as u32;
                let x1 = (x0 + 1).min(self.width - 1);
                let tx = fx - x0 as f32;
                let g = |xx: u32, yy: u32| self.v[(yy * self.width + xx) as usize];
                let a = g(x0, y0) * (1.0 - tx) + g(x1, y0) * tx;
                let b = g(x0, y1) * (1.0 - tx) + g(x1, y1) * tx;
                *o = a * (1.0 - ty) + b * ty;
            }
        });
        out
    }
    /// UV 矩形(u0,v0,u1,v1, 0..1, 纹理左上为原点, 与 D3D 一致)置为 val(取最大)。
    pub fn add_uv_rect(&mut self, r: [f32; 4], val: f32) {
        let (w, h) = (self.width as f32, self.height as f32);
        let x0 = (r[0].min(r[2]) * w).floor().max(0.0) as u32;
        let x1 = (r[0].max(r[2]) * w).ceil().min(w) as u32;
        let y0 = (r[1].min(r[3]) * h).floor().max(0.0) as u32;
        let y1 = (r[1].max(r[3]) * h).ceil().min(h) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * self.width + x) as usize;
                self.v[i] = self.v[i].max(val);
            }
        }
    }
    pub fn invert(&mut self) {
        self.v.iter_mut().for_each(|x| *x = 1.0 - *x);
    }
    pub fn multiply(&mut self, o: &Mask) {
        let o = o.resized(self.width, self.height);
        self.v.iter_mut().zip(&o.v).for_each(|(a, b)| *a *= *b);
    }
    pub fn union(&mut self, o: &Mask) {
        let o = o.resized(self.width, self.height);
        self.v.iter_mut().zip(&o.v).for_each(|(a, b)| *a = a.max(*b));
    }
    /// 形态学膨胀 r 像素(方形核, 可分离)。用于给 UV 岛加出血边, 避免 mip/过滤时边缘漏色。
    pub fn dilate(&mut self, r: u32) {
        if r == 0 {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let r = r as usize;
        let mut tmp = vec![0f32; w * h];
        tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let src = &self.v[y * w..(y + 1) * w];
            for x in 0..w {
                let a = x.saturating_sub(r);
                let b = (x + r).min(w - 1);
                row[x] = src[a..=b].iter().cloned().fold(0.0, f32::max);
            }
        });
        let tmp2 = &tmp;
        self.v.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let a = y.saturating_sub(r);
            let b = (y + r).min(h - 1);
            for x in 0..w {
                let mut m = 0f32;
                for yy in a..=b {
                    m = m.max(tmp2[yy * w + x]);
                }
                row[x] = m;
            }
        });
    }
    /// 盒式模糊(两遍可分离, 半径 r), 用于羽化边缘。
    pub fn blur(&mut self, r: u32) {
        if r == 0 {
            return;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let r = r as usize;
        let mut tmp = vec![0f32; w * h];
        tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let src = &self.v[y * w..(y + 1) * w];
            let mut pre = vec![0f32; w + 1];
            for x in 0..w {
                pre[x + 1] = pre[x] + src[x];
            }
            for x in 0..w {
                let a = x.saturating_sub(r);
                let b = (x + r).min(w - 1);
                row[x] = (pre[b + 1] - pre[a]) / (b + 1 - a) as f32;
            }
        });
        let tmp2 = &tmp;
        let cols: Vec<Vec<f32>> = (0..w)
            .into_par_iter()
            .map(|x| {
                let mut pre = vec![0f32; h + 1];
                for y in 0..h {
                    pre[y + 1] = pre[y] + tmp2[y * w + x];
                }
                (0..h)
                    .map(|y| {
                        let a = y.saturating_sub(r);
                        let b = (y + r).min(h - 1);
                        (pre[b + 1] - pre[a]) / (b + 1 - a) as f32
                    })
                    .collect()
            })
            .collect();
        for (x, c) in cols.iter().enumerate() {
            for (y, v) in c.iter().enumerate() {
                self.v[y * w + x] = *v;
            }
        }
    }
    pub fn coverage(&self) -> f64 {
        self.v.iter().map(|&x| x as f64).sum::<f64>() / self.v.len().max(1) as f64
    }
    pub fn to_image(&self) -> Image {
        let mut img = Image::new(self.width, self.height);
        for (p, &m) in img.px.chunks_exact_mut(4).zip(&self.v) {
            let g = (m.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            p.copy_from_slice(&[g, g, g, 255]);
        }
        img
    }
}

/// 按像素颜色挑选(HSL, 软边缘)。各项 None 表示不限制。
#[derive(Clone, Debug, Default)]
pub struct Selector {
    /// HSL 饱和度范围 0..1
    pub sat: Option<(f32, f32)>,
    /// HSL 亮度范围 0..1
    pub light: Option<(f32, f32)>,
    /// 色相范围(度), 允许跨 360(如 330..30)
    pub hue: Option<(f32, f32)>,
    /// 排除的色相范围(度); 只排除有一定饱和度的像素(灰色不受影响)
    pub exclude_hue: Vec<(f32, f32)>,
    /// 软边宽度(饱和度/亮度单位; 色相为该值×360 度)
    pub soft: f32,
}

fn soft_range(x: f32, lo: f32, hi: f32, s: f32) -> f32 {
    if s <= 0.0 {
        return if x >= lo && x <= hi { 1.0 } else { 0.0 };
    }
    let a = ((x - (lo - s)) / s).clamp(0.0, 1.0);
    let b = (((hi + s) - x) / s).clamp(0.0, 1.0);
    a.min(b)
}

impl Selector {
    pub fn is_empty(&self) -> bool {
        self.sat.is_none() && self.light.is_none() && self.hue.is_none() && self.exclude_hue.is_empty()
    }
    pub fn weight(&self, rgb: [u8; 3]) -> f32 {
        if self.is_empty() {
            return 1.0;
        }
        let hsl = rgb_to_hsl(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0);
        let mut w = 1f32;
        if let Some((a, b)) = self.sat {
            w = w.min(soft_range(hsl[1], a, b, self.soft));
        }
        if let Some((a, b)) = self.light {
            w = w.min(soft_range(hsl[2], a, b, self.soft));
        }
        let s = self.soft * 360.0;
        let h = hsl[0];
        let in_hue = |a: f32, b: f32| if a <= b { soft_range(h, a, b, s).max(soft_range(h + 360.0, a, b, s)).max(soft_range(h - 360.0, a, b, s)) } else { soft_range(h, a, 360.0 + s, s).max(soft_range(h, -s, b, s)) };
        // 灰色像素无色相: 饱和度很低时色相判定淡出
        let chroma = (hsl[1] / 0.08).clamp(0.0, 1.0);
        if let Some((a, b)) = self.hue {
            w = w.min(in_hue(a, b) * chroma);
        }
        for &(a, b) in &self.exclude_hue {
            w = w.min(1.0 - in_hue(a, b) * chroma);
        }
        w
    }
}

/// 换色参数
#[derive(Clone, Debug)]
pub struct Recolor {
    /// 目标色(sRGB)。Some 时做"保留细节的整体换色"
    pub to_color: Option<[u8; 3]>,
    /// 目标色模式下亮度起伏保留系数(1 = 原样保留相对亮度细节)
    pub detail: f32,
    /// 目标色模式下原有色度偏差的保留系数(0 = 色调完全统一, 1 = 保留原有色斑)
    pub chroma_detail: f32,
    /// 目标色模式下是否把选区平均亮度移到目标色亮度(否则保留原亮度)
    pub match_lightness: bool,
    /// 色相旋转(度, OKLCh)
    pub hue_shift: f32,
    /// 饱和度(色度)乘数
    pub sat_mul: f32,
    /// 亮度乘数(OKLab L)
    pub light_mul: f32,
    /// 亮度加量(OKLab L)
    pub light_add: f32,
    /// 总强度 0..1
    pub strength: f32,
}

impl Default for Recolor {
    fn default() -> Self {
        Recolor { to_color: None, detail: 1.0, chroma_detail: 0.35, match_lightness: true, hue_shift: 0.0, sat_mul: 1.0, light_mul: 1.0, light_add: 0.0, strength: 1.0 }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RecolorStats {
    /// 有效选区(权重和 / 像素数)
    pub selected_fraction: f64,
    /// 选区平均 OKLab(换色前)
    pub mean_oklab_before: [f32; 3],
    pub mean_srgb_before: [u8; 3],
    pub mean_srgb_after: [u8; 3],
    pub changed_pixels: usize,
}

fn lab_to_srgb8(lab: [f32; 3]) -> [u8; 3] {
    let l = oklab_to_linear(lab);
    [0, 1, 2].map(|i| (linear_to_srgb(l[i].clamp(0.0, 1.0)) * 255.0 + 0.5).clamp(0.0, 255.0) as u8)
}

/// 计算每像素选区权重 = mask × selector × strength
pub fn selection(img: &Image, mask: Option<&Mask>, sel: &Selector, strength: f32) -> Vec<f32> {
    let m = mask.map(|m| m.resized(img.width, img.height));
    img.px
        .par_chunks_exact(4)
        .enumerate()
        .map(|(i, p)| {
            let mw = m.as_ref().map(|m| m.v[i]).unwrap_or(1.0);
            if mw <= 0.0 {
                return 0.0;
            }
            (mw * sel.weight([p[0], p[1], p[2]]) * strength).clamp(0.0, 1.0)
        })
        .collect()
}

/// 对 img 就地换色(alpha 不变)。
pub fn recolor(img: &mut Image, weights: &[f32], rc: &Recolor) -> RecolorStats {
    let lut = srgb_lut();
    let n = (img.width * img.height) as usize;
    // 选区统计(加权平均 OKLab)
    let (sw, sl, sa, sb) = img
        .px
        .par_chunks_exact(4)
        .zip(weights.par_iter())
        .filter(|(_, &w)| w > 0.0)
        .map(|(p, &w)| {
            let lab = linear_to_oklab([lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]]);
            (w as f64, lab[0] as f64 * w as f64, lab[1] as f64 * w as f64, lab[2] as f64 * w as f64)
        })
        .reduce(|| (0.0, 0.0, 0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3));
    let mean = if sw > 0.0 { [(sl / sw) as f32, (sa / sw) as f32, (sb / sw) as f32] } else { [0.0; 3] };
    let target = rc.to_color.map(|c| linear_to_oklab([lut[c[0] as usize], lut[c[1] as usize], lut[c[2] as usize]]));
    let (hs, hc) = (rc.hue_shift.to_radians().sin(), rc.hue_shift.to_radians().cos());
    let (chg, a0, a1, a2, a3) = img
        .px
        .par_chunks_exact_mut(4)
        .zip(weights.par_iter())
        .map(|(p, &w)| {
            if w <= 0.0 {
                return (0usize, 0f64, 0f64, 0f64, 0f64);
            }
            let lab = linear_to_oklab([lut[p[0] as usize], lut[p[1] as usize], lut[p[2] as usize]]);
            let mut o = lab;
            if let Some(t) = target {
                let lt = if rc.match_lightness { t[0] } else { mean[0] };
                o[0] = lt + (lab[0] - mean[0]) * rc.detail;
                o[1] = t[1] + (lab[1] - mean[1]) * rc.chroma_detail;
                o[2] = t[2] + (lab[2] - mean[2]) * rc.chroma_detail;
            }
            // 色相旋转 + 色度缩放
            let (a, b) = (o[1], o[2]);
            o[1] = (a * hc - b * hs) * rc.sat_mul;
            o[2] = (a * hs + b * hc) * rc.sat_mul;
            o[0] = (o[0] * rc.light_mul + rc.light_add).clamp(0.0, 1.0);
            let nl = [o[0] * w + lab[0] * (1.0 - w), o[1] * w + lab[1] * (1.0 - w), o[2] * w + lab[2] * (1.0 - w)];
            let c = lab_to_srgb8(nl);
            let ch = (c != [p[0], p[1], p[2]]) as usize;
            p[0] = c[0];
            p[1] = c[1];
            p[2] = c[2];
            let wf = w as f64;
            (ch, wf, nl[0] as f64 * wf, nl[1] as f64 * wf, nl[2] as f64 * wf)
        })
        .reduce(|| (0, 0.0, 0.0, 0.0, 0.0), |x, y| (x.0 + y.0, x.1 + y.1, x.2 + y.2, x.3 + y.3, x.4 + y.4));
    let mean_after = if a0 > 0.0 { [(a1 / a0) as f32, (a2 / a0) as f32, (a3 / a0) as f32] } else { [0.0; 3] };
    RecolorStats {
        selected_fraction: sw / n.max(1) as f64,
        mean_oklab_before: mean,
        mean_srgb_before: lab_to_srgb8(mean),
        mean_srgb_after: lab_to_srgb8(mean_after),
        changed_pixels: chg,
    }
}

/// 发光通道编辑参数(_mg 的 B 通道)
#[derive(Clone, Debug)]
pub struct GlowEdit {
    /// 发光强度 0..1(着色器里按 B² 使用, 这里写入 sqrt(强度))
    pub intensity: f32,
    /// 遮罩外的 B 是否保留原值(默认清零: 原贴图 B 通道并非发光遮罩, 不清会到处发光)
    pub keep_outside: bool,
    /// 遮罩满值处的 B 上限(0..255, 默认 255)。低值(25–50)= 大面积微光: B 按比例缩放, 软边保留
    pub b_max: u8,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GlowStats {
    pub glow_fraction: f64,
    pub b_nonzero_before: f64,
    pub b_nonzero_after: f64,
}

/// 写 _mg 的 B 通道: B = b_max·sqrt(intensity·mask)(遮罩外清零或保留)。R/G/A 不动。
pub fn write_glow_channel(mg: &mut Image, mask: &Mask, g: &GlowEdit) -> GlowStats {
    let m = mask.resized(mg.width, mg.height);
    let n = m.v.len().max(1) as f64;
    let before = mg.px.chunks_exact(4).filter(|p| p[2] > 8).count() as f64 / n;
    mg.px.par_chunks_exact_mut(4).zip(m.v.par_iter()).for_each(|(p, &w)| {
        let w = w.clamp(0.0, 1.0);
        let val = ((g.intensity.clamp(0.0, 1.0) * w).sqrt() * g.b_max as f32 + 0.5) as u8;
        if g.keep_outside {
            p[2] = p[2].max(val);
        } else {
            p[2] = val;
        }
    });
    let after = mg.px.chunks_exact(4).filter(|p| p[2] > 8).count() as f64 / n;
    GlowStats { glow_fraction: m.coverage(), b_nonzero_before: before, b_nonzero_after: after }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn img_noise(w: u32, h: u32) -> Image {
        let mut im = Image::new(w, h);
        let mut s = 12345u32;
        for p in im.px.chunks_exact_mut(4) {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = (s >> 24) as i32 - 128;
            let base = [150i32, 140, 120];
            for i in 0..3 {
                p[i] = (base[i] + n / 4).clamp(0, 255) as u8;
            }
            p[3] = 255;
        }
        im
    }
    #[test]
    fn recolor_to_color_keeps_detail() {
        let mut im = img_noise(64, 64);
        let orig = im.clone();
        let w = vec![1.0; 64 * 64];
        let st = recolor(&mut im, &w, &Recolor { to_color: Some([0x1f, 0x4e, 0x79]), ..Default::default() });
        // 平均颜色接近目标
        for i in 0..3 {
            assert!((st.mean_srgb_after[i] as i32 - [0x1f, 0x4e, 0x79][i] as i32).abs() <= 3, "{:?}", st.mean_srgb_after);
        }
        // 细节保留: 原图亮度高的像素换色后仍然更亮
        let lum = |p: &[u8]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        let mut agree = 0;
        let mut tot = 0;
        for i in 0..(64 * 64 - 1) {
            let (a, b) = (&orig.px[i * 4..i * 4 + 3], &orig.px[i * 4 + 4..i * 4 + 7]);
            if lum(a).abs_diff(lum(b)) > 20 {
                tot += 1;
                let (c, d) = (&im.px[i * 4..i * 4 + 3], &im.px[i * 4 + 4..i * 4 + 7]);
                if (lum(a) > lum(b)) == (lum(c) > lum(d)) {
                    agree += 1;
                }
            }
        }
        assert!(tot > 100 && agree as f64 / tot as f64 > 0.95, "{agree}/{tot}");
    }
    #[test]
    fn mask_ops() {
        let mut m = Mask::filled(16, 16, 0.0);
        m.add_uv_rect([0.25, 0.25, 0.5, 0.5], 1.0);
        assert_eq!(m.v.iter().filter(|&&x| x > 0.5).count(), 16);
        m.dilate(1);
        assert_eq!(m.v.iter().filter(|&&x| x > 0.5).count(), 36);
        m.blur(1);
        assert!(m.v[0] == 0.0 && m.v[(6 * 16 + 6) as usize] > 0.9);
        let r = m.resized(32, 32);
        assert_eq!(r.v.len(), 1024);
    }
    #[test]
    fn glow_channel() {
        let mut mg = Image::new(4, 1);
        mg.px = vec![10, 20, 250, 255, 10, 20, 250, 255, 10, 20, 0, 255, 10, 20, 0, 255];
        let mut m = Mask::filled(4, 1, 0.0);
        m.v[1] = 1.0;
        m.v[2] = 0.25;
        let mut mg2 = mg.clone();
        let st = write_glow_channel(&mut mg, &m, &GlowEdit { intensity: 1.0, keep_outside: false, b_max: 255 });
        assert_eq!([mg.px[2], mg.px[6], mg.px[10], mg.px[14]], [0, 255, 128, 0]);
        assert_eq!([mg.px[0], mg.px[1], mg.px[3]], [10, 20, 255]);
        assert!((st.b_nonzero_before - 0.5).abs() < 1e-9);
        // B 上限: 满值处 = 40, 半值(sqrt .25)处 = 20
        write_glow_channel(&mut mg2, &m, &GlowEdit { intensity: 1.0, keep_outside: false, b_max: 40 });
        assert_eq!([mg2.px[2], mg2.px[6], mg2.px[10], mg2.px[14]], [0, 40, 20, 0]);
    }
    #[test]
    fn selector() {
        let s = Selector { sat: Some((0.0, 0.2)), soft: 0.0, ..Default::default() };
        assert_eq!(s.weight([128, 128, 128]), 1.0);
        assert_eq!(s.weight([200, 50, 50]), 0.0);
        let h = Selector { hue: Some((330.0, 30.0)), soft: 0.0, ..Default::default() };
        assert_eq!(h.weight([200, 50, 50]), 1.0);
        assert_eq!(h.weight([50, 200, 50]), 0.0);
        let x = Selector { exclude_hue: vec![(0.0, 60.0)], soft: 0.0, ..Default::default() };
        assert_eq!(x.weight([150, 100, 60]), 0.0); // 木色
        assert_eq!(x.weight([100, 110, 130]), 1.0); // 蓝灰
        assert_eq!(x.weight([120, 120, 120]), 1.0); // 灰
    }
}
