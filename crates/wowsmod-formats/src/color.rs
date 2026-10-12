//! 颜色空间: sRGB ↔ 线性, 线性 RGB ↔ OKLab(Björn Ottosson, 公有领域/MIT 公式), HSL。
#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
#[inline]
pub fn linear_to_srgb(l: f32) -> f32 {
    let l = l.max(0.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// 0..255 sRGB 查表 → 线性
pub fn srgb_lut() -> [f32; 256] {
    let mut t = [0f32; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = srgb_to_linear(i as f32 / 255.0);
    }
    t
}

pub fn linear_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_46 * c[0] + 0.536_332_55 * c[1] + 0.051_445_995 * c[2];
    let m = 0.211_903_5 * c[0] + 0.680_699_5 * c[1] + 0.107_396_96 * c[2];
    let s = 0.088_302_46 * c[0] + 0.281_718_85 * c[1] + 0.629_978_7 * c[2];
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

pub fn oklab_to_linear(c: [f32; 3]) -> [f32; 3] {
    let l = c[0] + 0.396_337_78 * c[1] + 0.215_803_76 * c[2];
    let m = c[0] - 0.105_561_346 * c[1] - 0.063_854_17 * c[2];
    let s = c[0] - 0.089_484_18 * c[1] - 1.291_485_5 * c[2];
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// sRGB(0..1) → HSL(h 0..360, s 0..1, l 0..1)
pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> [f32; 3] {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h * 60.0, s, l]
}

/// 解析 "#RRGGBB" / "RRGGBB" / "r,g,b"(0..255)
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let v = u32::from_str_radix(t, 16).ok()?;
        return Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
    }
    let p: Vec<_> = s.split(',').map(|x| x.trim().parse::<u8>()).collect();
    if p.len() == 3 && p.iter().all(|x| x.is_ok()) {
        return Some([*p[0].as_ref().unwrap(), *p[1].as_ref().unwrap(), *p[2].as_ref().unwrap()]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oklab_roundtrip() {
        for c in [[0.2f32, 0.5, 0.9], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [0.7, 0.1, 0.05]] {
            let b = oklab_to_linear(linear_to_oklab(c));
            for i in 0..3 {
                assert!((b[i] - c[i]).abs() < 1e-4, "{c:?} {b:?}");
            }
        }
        let w = linear_to_oklab([1.0, 1.0, 1.0]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3);
    }
    #[test]
    fn colors() {
        assert_eq!(parse_color("#1F4E79"), Some([0x1f, 0x4e, 0x79]));
        assert_eq!(parse_color("10, 20,30"), Some([10, 20, 30]));
        let h = rgb_to_hsl(1.0, 0.0, 0.0);
        assert!((h[0] - 0.0).abs() < 1e-3 && (h[1] - 1.0).abs() < 1e-3);
    }
}
