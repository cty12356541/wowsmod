//! ModsSDK 明文 `.visual` / `.model` (BigWorld 风格 XML) 的读取与"外科手术式"编辑。
//!
//! 只依赖这几种结构(26.10 SDK 实测):
//! ```text
//! <X.visual>
//!   <node><identifier> Scene Root </identifier><transform><row0>..</row0>..<row3>..</row3></transform> <node>...</node>* </node>
//!   <renderSet> <treatAsWorldSpaceObject/>? <node> 骨骼名 </node>* <geometry> <vertices> XShape.vertices </vertices>
//!       <primitive> XShape.indices </primitive> <primitiveGroup> 0 <material><identifier/><mfm/></material></primitiveGroup>
//!   </geometry></renderSet>*
//!   <boundingBox><min> x y z </min><max> x y z </max></boundingBox>
//! </X.visual>
//! ```
//! 矩阵: row0..row3 为行向量约定(row3 = 平移), 世界矩阵 W(node) = L(node) · W(parent)。
//! 编辑时只替换对应文本区间, 其余字节(缩进、制表符、注释)保持原样。
use crate::error::{Error, Result};

pub type M4 = [[f64; 4]; 4];

pub const IDENTITY: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub fn mat_mul(a: &M4, b: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

pub fn transpose(a: &M4) -> M4 {
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = a[j][i];
        }
    }
    r
}

/// 通用 4x4 求逆(高斯消元); 奇异时返回 None
pub fn inverse(a: &M4) -> Option<M4> {
    let mut m = [[0.0f64; 8]; 4];
    for i in 0..4 {
        for j in 0..4 {
            m[i][j] = a[i][j];
        }
        m[i][4 + i] = 1.0;
    }
    for c in 0..4 {
        let p = (c..4).max_by(|&x, &y| m[x][c].abs().partial_cmp(&m[y][c].abs()).unwrap())?;
        if m[p][c].abs() < 1e-12 {
            return None;
        }
        m.swap(c, p);
        let d = m[c][c];
        for j in 0..8 {
            m[c][j] /= d;
        }
        for r in 0..4 {
            if r != c {
                let f = m[r][c];
                for j in 0..8 {
                    m[r][j] -= f * m[c][j];
                }
            }
        }
    }
    let mut r = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = m[i][4 + j];
        }
    }
    Some(r)
}

/// 行向量约定下的点变换 p' = [p,1]·M
pub fn xform_point(m: &M4, p: [f64; 3]) -> [f64; 3] {
    let mut r = [0.0; 3];
    for j in 0..3 {
        r[j] = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j];
    }
    r
}

/// 行向量约定下的方向变换(不含平移)
pub fn xform_dir(m: &M4, p: [f64; 3]) -> [f64; 3] {
    let mut r = [0.0; 3];
    for j in 0..3 {
        r[j] = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j];
    }
    r
}

/// 由 平移/欧拉角(度, 依次绕 X、Y、Z)/缩放 组成行向量约定矩阵: M = S · Rx · Ry · Rz · T
pub fn compose(translate: [f64; 3], rot_deg: [f64; 3], scale: [f64; 3]) -> M4 {
    let (sx, cx) = rot_deg[0].to_radians().sin_cos();
    let (sy, cy) = rot_deg[1].to_radians().sin_cos();
    let (sz, cz) = rot_deg[2].to_radians().sin_cos();
    // 行向量约定: v' = v·R; 绕 X 轴
    let rx: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, cx, sx, 0.0], [0.0, -sx, cx, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let ry: M4 = [[cy, 0.0, -sy, 0.0], [0.0, 1.0, 0.0, 0.0], [sy, 0.0, cy, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let rz: M4 = [[cz, sz, 0.0, 0.0], [-sz, cz, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let s: M4 = [[scale[0], 0.0, 0.0, 0.0], [0.0, scale[1], 0.0, 0.0], [0.0, 0.0, scale[2], 0.0], [0.0, 0.0, 0.0, 1.0]];
    let mut t = IDENTITY;
    t[3][0] = translate[0];
    t[3][1] = translate[1];
    t[3][2] = translate[2];
    mat_mul(&mat_mul(&mat_mul(&mat_mul(&s, &rx), &ry), &rz), &t)
}

// ---------------------------------------------------------------- 极简 XML 树

#[derive(Debug, Clone)]
pub struct Elem {
    pub tag: String,
    /// 整个元素(含起止标签)在文本中的字节区间
    pub start: usize,
    pub end: usize,
    /// 内部内容区间(起始标签之后到结束标签之前)
    pub inner: (usize, usize),
    pub children: Vec<Elem>,
}

impl Elem {
    pub fn child(&self, tag: &str) -> Option<&Elem> {
        self.children.iter().find(|c| c.tag == tag)
    }
    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Elem> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
    }
    /// 直接文本(去掉子元素后)并 trim
    pub fn text<'a>(&self, src: &'a str) -> String {
        let mut s = String::new();
        let mut p = self.inner.0;
        for c in &self.children {
            s.push_str(&src[p..c.start]);
            p = c.end;
        }
        s.push_str(&src[p..self.inner.1]);
        s.trim().to_string()
    }
}

/// 解析 BigWorld XML(无属性、无 CDATA; 支持 <!-- --> 注释与 <x/> 空元素)
pub fn parse_xml(src: &str) -> Result<Elem> {
    let b = src.as_bytes();
    let mut stack: Vec<Elem> = vec![Elem { tag: String::new(), start: 0, end: src.len(), inner: (0, src.len()), children: vec![] }];
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with("<!--") {
            let e = src[i..].find("-->").ok_or_else(|| Error::format("xml", "注释未闭合"))?;
            i += e + 3;
            continue;
        }
        if src[i..].starts_with("<?") {
            let e = src[i..].find("?>").ok_or_else(|| Error::format("xml", "声明未闭合"))?;
            i += e + 2;
            continue;
        }
        let close = src[i..].find('>').ok_or_else(|| Error::format("xml", format!("偏移 {i} 处标签未闭合")))? + i;
        let raw = &src[i + 1..close];
        if let Some(name) = raw.strip_prefix('/') {
            let name = name.trim();
            let mut el = stack.pop().ok_or_else(|| Error::format("xml", "多余的结束标签"))?;
            if el.tag != name {
                return Err(Error::format("xml", format!("结束标签 </{name}> 与 <{}> 不匹配(偏移 {i})", el.tag)));
            }
            el.inner.1 = i;
            el.end = close + 1;
            stack.last_mut().ok_or_else(|| Error::format("xml", "结构错误"))?.children.push(el);
        } else if raw.ends_with('/') {
            let name = raw.trim_end_matches('/').trim().to_string();
            stack.last_mut().unwrap().children.push(Elem { tag: name, start: i, end: close + 1, inner: (close + 1, close + 1), children: vec![] });
        } else {
            let name = raw.trim().to_string();
            stack.push(Elem { tag: name, start: i, end: 0, inner: (close + 1, 0), children: vec![] });
        }
        i = close + 1;
    }
    if stack.len() != 1 {
        return Err(Error::format("xml", format!("有 {} 个元素未闭合", stack.len() - 1)));
    }
    let mut doc = stack.pop().unwrap();
    if doc.children.len() != 1 {
        return Err(Error::format("xml", format!("顶层应只有 1 个元素, 实际 {}", doc.children.len())));
    }
    Ok(doc.children.remove(0))
}

fn parse_floats(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|x| x.parse::<f64>().ok()).collect()
}

// ---------------------------------------------------------------- .visual

#[derive(Debug, Clone, serde::Serialize)]
pub struct VNode {
    pub name: String,
    pub parent: Option<usize>,
    pub local: M4,
    pub world: M4,
    #[serde(skip)]
    pub transform_span: Option<(usize, usize)>,
    #[serde(skip)]
    pub span: (usize, usize),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VRenderSet {
    pub nodes: Vec<String>,
    pub vertices: String,
    pub primitive: String,
    pub material_identifier: String,
    pub mfm: String,
    pub world_space: bool,
    #[serde(skip)]
    pub span: (usize, usize),
}

impl VRenderSet {
    /// 渲染集基名: "TurretShape.vertices" → "Turret"
    pub fn base_name(&self) -> String {
        self.vertices.trim_end_matches(".vertices").trim_end_matches("Shape").to_string()
    }
    /// 去掉 _lodN 后缀的部件名(用于 LOD 间对应)
    pub fn family(&self) -> String {
        let b = self.base_name();
        let t = b.trim_end_matches(|c: char| c.is_ascii_digit());
        let b = if t.len() < b.len() && t.ends_with("Shape") { t.trim_end_matches("Shape").to_string() } else { b };
        let f = strip_lod(&b);
        f.strip_suffix("_lod").map(str::to_string).unwrap_or(f)
    }
}

/// "Turret_Gun9_lod1" → "Turret_Gun9"
pub fn strip_lod(s: &str) -> String {
    if let Some(p) = s.rfind("_lod") {
        if s[p + 4..].chars().all(|c| c.is_ascii_digit()) && p + 4 < s.len() {
            return s[..p].to_string();
        }
    }
    s.to_string()
}

#[derive(Debug, Clone)]
pub struct Visual {
    pub text: String,
    pub root_tag: String,
    pub nodes: Vec<VNode>,
    pub render_sets: Vec<VRenderSet>,
    pub bbox: Option<([f64; 3], [f64; 3])>,
    bbox_span: Option<(usize, usize)>,
}

fn read_matrix(src: &str, t: &Elem) -> M4 {
    let mut m = IDENTITY;
    for r in 0..4 {
        if let Some(row) = t.child(&format!("row{r}")) {
            let v = parse_floats(&row.text(src));
            for c in 0..4.min(v.len()) {
                m[r][c] = v[c];
            }
        }
    }
    m
}

impl Visual {
    pub fn parse(text: &str) -> Result<Visual> {
        let root = parse_xml(text)?;
        let mut nodes = Vec::new();
        fn walk(src: &str, e: &Elem, parent: Option<usize>, pw: &M4, out: &mut Vec<VNode>) {
            let name = e.child("identifier").map(|x| x.text(src)).unwrap_or_default();
            let (local, tspan) = match e.child("transform") {
                Some(t) => (read_matrix(src, t), Some((t.start, t.end))),
                None => (IDENTITY, None),
            };
            let world = mat_mul(&local, pw);
            let idx = out.len();
            out.push(VNode { name, parent, local, world, transform_span: tspan, span: (e.start, e.end) });
            for c in e.children_named("node") {
                walk(src, c, Some(idx), &world, out);
            }
        }
        for n in root.children_named("node") {
            walk(text, n, None, &IDENTITY, &mut nodes);
        }
        let mut render_sets = Vec::new();
        for rs in root.children_named("renderSet") {
            let g = rs.child("geometry");
            let mat = g.and_then(|g| g.child("primitiveGroup")).and_then(|p| p.child("material"));
            render_sets.push(VRenderSet {
                nodes: rs.children_named("node").map(|n| n.text(text)).collect(),
                vertices: g.and_then(|g| g.child("vertices")).map(|x| x.text(text)).unwrap_or_default(),
                primitive: g.and_then(|g| g.child("primitive")).map(|x| x.text(text)).unwrap_or_default(),
                material_identifier: mat.and_then(|m| m.child("identifier")).map(|x| x.text(text)).unwrap_or_default(),
                mfm: mat.and_then(|m| m.child("mfm")).map(|x| x.text(text)).unwrap_or_default(),
                world_space: rs.child("treatAsWorldSpaceObject").map(|x| x.text(text) == "true").unwrap_or(false),
                span: (rs.start, rs.end),
            });
        }
        let (mut bbox, mut bbox_span) = (None, None);
        if let Some(bb) = root.child("boundingBox") {
            let mn = bb.child("min").map(|x| parse_floats(&x.text(text))).unwrap_or_default();
            let mx = bb.child("max").map(|x| parse_floats(&x.text(text))).unwrap_or_default();
            if mn.len() >= 3 && mx.len() >= 3 {
                bbox = Some(([mn[0], mn[1], mn[2]], [mx[0], mx[1], mx[2]]));
            }
            bbox_span = Some((bb.start, bb.end));
        }
        Ok(Visual { text: text.to_string(), root_tag: root.tag.clone(), nodes, render_sets, bbox, bbox_span })
    }

    pub fn node(&self, name: &str) -> Option<&VNode> {
        self.nodes.iter().find(|n| n.name == name)
    }

    /// 文本中 `[a,b)` 前的缩进(该行开头的制表符/空格)
    fn indent_at(&self, pos: usize) -> String {
        let ls = self.text[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        self.text[ls..pos].chars().take_while(|c| *c == '\t' || *c == ' ').collect()
    }

    fn splice(&mut self, span: (usize, usize), new: &str) -> Result<()> {
        let mut t = String::with_capacity(self.text.len() + new.len());
        t.push_str(&self.text[..span.0]);
        t.push_str(new);
        t.push_str(&self.text[span.1..]);
        *self = Visual::parse(&t)?;
        Ok(())
    }

    /// 设定节点局部矩阵(行向量约定)
    pub fn set_node_local(&mut self, name: &str, m: &M4) -> Result<()> {
        let n = self.node(name).ok_or_else(|| Error::NotFound(format!("visual 中没有节点 {name}")))?.clone();
        let ind = self.indent_at(n.span.0);
        let block = format_transform(m, &format!("{ind}\t"));
        match n.transform_span {
            Some(sp) => self.splice(sp, block.trim_start()),
            None => Err(Error::Unsupported(format!("节点 {name} 没有 <transform>"))),
        }
    }

    pub fn set_bbox(&mut self, mn: [f64; 3], mx: [f64; 3]) -> Result<()> {
        let sp = self.bbox_span.ok_or_else(|| Error::NotFound("visual 没有 <boundingBox>".into()))?;
        let ind = self.indent_at(sp.0);
        let s = format!(
            "<boundingBox>\n{ind}\t<min>\t{:.4} {:.4} {:.4}\t</min>\n{ind}\t<max>\t{:.4} {:.4} {:.4}\t</max>\n{ind}</boundingBox>",
            mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
        );
        self.splice(sp, &s)
    }

    /// 删除第 k 个 renderSet(连同其所在行的缩进与换行)
    pub fn remove_render_set(&mut self, k: usize) -> Result<()> {
        let sp = self.render_sets.get(k).ok_or_else(|| Error::NotFound(format!("renderSet #{k}")))?.span;
        let ls = self.text[..sp.0].rfind('\n').map(|p| p + 1).unwrap_or(sp.0);
        let le = if self.text[sp.1..].starts_with("\r\n") {
            sp.1 + 2
        } else if self.text[sp.1..].starts_with('\n') {
            sp.1 + 1
        } else {
            sp.1
        };
        let start = if self.text[ls..sp.0].trim().is_empty() { ls } else { sp.0 };
        self.splice((start, le), "")
    }

    /// 在最后一个 renderSet 之后(或 boundingBox 之前)插入一个 renderSet
    pub fn add_render_set(&mut self, rs: &NewRenderSet) -> Result<()> {
        let (pos, ind) = if let Some(last) = self.render_sets.last() {
            (last.span.1, self.indent_at(last.span.0))
        } else if let Some(sp) = self.bbox_span {
            (sp.0, self.indent_at(sp.0))
        } else {
            return Err(Error::Unsupported("visual 既没有 renderSet 也没有 boundingBox, 无法定位插入点".into()));
        };
        let nl = if self.text.contains("\r\n") { "\r\n" } else { "\n" };
        let mut s = String::new();
        let lead_nl = self.render_sets.last().is_some();
        if lead_nl {
            s.push_str(nl);
            s.push_str(&ind);
        }
        s.push_str(&rs.to_xml(&ind, nl));
        if !lead_nl {
            s.push_str(nl);
            s.push_str(&ind);
        }
        self.splice((pos, pos), &s)
    }

    /// 让某 renderSet 改用另一组网格(vertices / primitive 名), 例如让 LOD1-3 直接用 LOD0 的新网格
    pub fn set_render_set_geometry(&mut self, k: usize, vertices: &str, primitive: &str) -> Result<()> {
        let rs = self.render_sets.get(k).ok_or_else(|| Error::NotFound(format!("renderSet #{k}")))?.clone();
        let mut block = self.text[rs.span.0..rs.span.1].to_string();
        block = replace_tag_text(&block, "vertices", &rs.vertices, vertices);
        block = replace_tag_text(&block, "primitive", &rs.primitive, primitive);
        self.splice(rs.span, &block)
    }

    /// 修改某 renderSet 的材质(identifier, mfm)
    pub fn set_render_set_material(&mut self, k: usize, identifier: &str, mfm: &str) -> Result<()> {
        let rs = self.render_sets.get(k).ok_or_else(|| Error::NotFound(format!("renderSet #{k}")))?.clone();
        let mut block = self.text[rs.span.0..rs.span.1].to_string();
        if !rs.material_identifier.is_empty() {
            block = replace_tag_text(&block, "identifier", &rs.material_identifier, identifier);
        }
        if !rs.mfm.is_empty() {
            block = replace_tag_text(&block, "mfm", &rs.mfm, mfm);
        }
        self.splice(rs.span, &block)
    }
}

fn replace_tag_text(block: &str, tag: &str, old: &str, new: &str) -> String {
    let open = format!("<{tag}>");
    if let Some(p) = block.find(&open) {
        if let Some(q) = block[p..].find(old) {
            let a = p + q;
            return format!("{}{}{}", &block[..a], new, &block[a + old.len()..]);
        }
    }
    block.to_string()
}

pub fn format_transform(m: &M4, ind: &str) -> String {
    let mut s = String::from("<transform>\n");
    for r in 0..4 {
        s.push_str(&format!(
            "{ind}\t<row{r}>\t{} {} {} {}\t</row{r}>\n",
            fnum(m[r][0]),
            fnum(m[r][1]),
            fnum(m[r][2]),
            fnum(m[r][3])
        ));
    }
    s.push_str(ind);
    s.push_str("</transform>");
    s
}

/// 4 位小数足够时保持原样式(与 SDK 一致), 否则写 6 位
fn fnum(v: f64) -> String {
    let a = format!("{v:.4}");
    if (a.parse::<f64>().unwrap_or(v) - v).abs() < 5e-7 {
        a
    } else {
        format!("{v:.6}")
    }
}

#[derive(Debug, Clone)]
pub struct NewRenderSet {
    pub nodes: Vec<String>,
    pub name: String,
    pub material_identifier: String,
    pub mfm: String,
    pub world_space: bool,
}

impl NewRenderSet {
    fn to_xml(&self, ind: &str, nl: &str) -> String {
        let mut s = format!("<renderSet>{nl}");
        if self.world_space {
            s.push_str(&format!("{ind}\t<treatAsWorldSpaceObject>\ttrue\t</treatAsWorldSpaceObject>{nl}"));
        }
        for n in &self.nodes {
            s.push_str(&format!("{ind}\t<node>\t{n}\t</node>{nl}"));
        }
        s.push_str(&format!("{ind}\t<geometry>{nl}"));
        s.push_str(&format!("{ind}\t\t<vertices>\t{}Shape.vertices\t</vertices>{nl}", self.name));
        s.push_str(&format!("{ind}\t\t<primitive>\t{}Shape.indices\t</primitive>{nl}", self.name));
        s.push_str(&format!("{ind}\t\t<primitiveGroup>\t0{nl}"));
        s.push_str(&format!("{ind}\t\t\t<material>{nl}"));
        s.push_str(&format!("{ind}\t\t\t\t<identifier>\t{}\t</identifier>{nl}", self.material_identifier));
        s.push_str(&format!("{ind}\t\t\t\t<mfm>\t{}\t</mfm>{nl}", self.mfm));
        s.push_str(&format!("{ind}\t\t\t</material>{nl}"));
        s.push_str(&format!("{ind}\t\t</primitiveGroup>{nl}"));
        s.push_str(&format!("{ind}\t</geometry>{nl}"));
        s.push_str(&format!("{ind}</renderSet>"));
        s
    }
}

/// .model: 读取 nodefullVisual / parent / extent
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    pub visual: String,
    pub parent: Option<String>,
    pub extent: Option<f64>,
}

pub fn parse_model(text: &str) -> Result<ModelInfo> {
    let root = parse_xml(text)?;
    let visual = root
        .child("nodefullVisual")
        .or_else(|| root.child("nodelessVisual"))
        .or_else(|| root.child("visual"))
        .map(|e| e.text(text))
        .ok_or_else(|| Error::format("model", "没有 nodefullVisual / visual"))?;
    Ok(ModelInfo {
        visual,
        parent: root.child("parent").map(|e| e.text(text)),
        extent: root.child("extent").and_then(|e| e.text(text).parse().ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const V: &str = "<a.visual>\n\t<node>\n\t\t<identifier>\tScene Root\t</identifier>\n\t\t<transform>\n\t\t\t<row0>\t1.0000 0.0000 0.0000 0.0000\t</row0>\n\t\t\t<row1>\t0.0000 1.0000 0.0000 0.0000\t</row1>\n\t\t\t<row2>\t0.0000 0.0000 1.0000 0.0000\t</row2>\n\t\t\t<row3>\t0.0000 0.0000 0.0000 1.0000\t</row3>\n\t\t</transform>\n\t\t<node>\n\t\t\t<identifier>\tHP_X\t</identifier>\n\t\t\t<transform>\n\t\t\t\t<row0>\t1.0000 0.0000 0.0000 0.0000\t</row0>\n\t\t\t\t<row1>\t0.0000 1.0000 0.0000 0.0000\t</row1>\n\t\t\t\t<row2>\t0.0000 0.0000 1.0000 0.0000\t</row2>\n\t\t\t\t<row3>\t1.0000 2.0000 3.0000 1.0000\t</row3>\n\t\t\t</transform>\n\t\t</node>\n\t</node>\n\t<renderSet>\n\t\t<node>\tScene Root\t</node>\n\t\t<geometry>\n\t\t\t<vertices>\tAShape.vertices\t</vertices>\n\t\t\t<primitive>\tAShape.indices\t</primitive>\n\t\t\t<primitiveGroup>\t0\n\t\t\t\t<material>\n\t\t\t\t\t<identifier>\tM\t</identifier>\n\t\t\t\t\t<mfm>\tx.mfm\t</mfm>\n\t\t\t\t</material>\n\t\t\t</primitiveGroup>\n\t\t</geometry>\n\t</renderSet>\n\t<boundingBox>\n\t\t<min>\t-1.0000 0.0000 -1.0000\t</min>\n\t\t<max>\t1.0000 1.0000 1.0000\t</max>\n\t</boundingBox>\n</a.visual>\n";

    #[test]
    fn parse_edit_roundtrip() {
        let mut v = Visual::parse(V).unwrap();
        assert_eq!(v.nodes.len(), 2);
        assert_eq!(v.node("HP_X").unwrap().world[3][..3], [1.0, 2.0, 3.0]);
        assert_eq!(v.render_sets[0].base_name(), "A");
        assert_eq!(v.render_sets[0].mfm, "x.mfm");
        // 不改内容时文本完全不变
        let m = v.node("HP_X").unwrap().local;
        v.set_node_local("HP_X", &m).unwrap();
        assert_eq!(v.text, V);
        let bb = v.bbox.unwrap();
        v.set_bbox(bb.0, bb.1).unwrap();
        assert_eq!(v.text, V);
        let mut m2 = m;
        m2[3][0] = 5.5;
        v.set_node_local("HP_X", &m2).unwrap();
        assert_eq!(v.node("HP_X").unwrap().world[3][0], 5.5);
        v.add_render_set(&NewRenderSet { nodes: vec!["Scene Root".into()], name: "B".into(), material_identifier: "M".into(), mfm: "x.mfm".into(), world_space: false }).unwrap();
        assert_eq!(v.render_sets.len(), 2);
        assert_eq!(v.render_sets[1].vertices, "BShape.vertices");
        v.remove_render_set(1).unwrap();
        v.set_node_local("HP_X", &m).unwrap();
        assert_eq!(v.text, V);
    }

    #[test]
    fn lod_family() {
        assert_eq!(strip_lod("Turret_Gun9_lod1"), "Turret_Gun9");
        assert_eq!(strip_lod("Turret"), "Turret");
        assert_eq!(strip_lod("Hull_lodx"), "Hull_lodx");
    }

    #[test]
    fn compose_inverse() {
        let m = compose([1.0, 2.0, 3.0], [10.0, 20.0, 30.0], [1.5, 1.5, 1.5]);
        let i = inverse(&m).unwrap();
        let e = mat_mul(&m, &i);
        for r in 0..4 {
            for c in 0..4 {
                assert!((e[r][c] - IDENTITY[r][c]).abs() < 1e-9);
            }
        }
        // 绕 Y 90 度: +z → +x (行向量约定, 左手系下的偏航)
        let r = compose([0.0; 3], [0.0, 90.0, 0.0], [1.0; 3]);
        let d = xform_dir(&r, [0.0, 0.0, 1.0]);
        assert!((d[0] - 1.0).abs() < 1e-9 && d[2].abs() < 1e-9);
    }
}
