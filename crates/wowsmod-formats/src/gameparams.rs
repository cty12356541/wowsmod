//! content/GameParams_py3.data: 字节整体反转 → zlib → pickle(协议 2)。
//!
//! 26.10 实测结构: 顶层是 2 元组 `(dict 参数ID → GPData, set)`; 舰船条目 typeinfo.type == "Ship",
//! 有 index("PFSB510")、name("PFSB510_Bourgogne")、level、typeinfo.nation/species、group,
//! 船体模块 `A_Hull`/`B_Hull`/... 的 `model` 字段是船体 .model 路径, 各武器挂点(HP_*)的 `model` 是炮塔等模型。
//! 26.10 用到的 pickle 操作码见 tests; 未知操作码会报错而不是猜。
use crate::error::{Error, Result};
use crate::pickle::{self, Value};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ShipParam {
    /// 例如 PFSB510
    pub index: String,
    /// 例如 PFSB510_Bourgogne
    pub name: String,
    pub level: i64,
    pub nation: String,
    pub species: String,
    pub group: String,
    /// 船体模型 (.model), 来自 *_Hull 模块
    pub hull_models: Vec<String>,
    /// 其它模型(炮塔、防空炮、雷达等), 去重
    pub other_models: Vec<String>,
}

impl ShipParam {
    /// 模型所在目录(去掉文件名), 船体优先, 去重
    pub fn model_dirs(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for m in self.hull_models.iter().chain(self.other_models.iter()) {
            if let Some((d, _)) = m.rsplit_once('/') {
                if !v.iter().any(|x| x == d) {
                    v.push(d.to_string());
                }
            }
        }
        v
    }
}

pub fn decompress(raw: &[u8]) -> Result<Vec<u8>> {
    let rev: Vec<u8> = raw.iter().rev().copied().collect();
    miniz_oxide::inflate::decompress_to_vec_zlib(&rev)
        .map_err(|e| Error::Decompress { what: "GameParams".into(), msg: format!("zlib: {e:?}(字节反转后)") })
}

fn collect_models(v: &Value, under_hull: bool, hull: &mut Vec<String>, other: &mut Vec<String>, depth: usize) {
    if depth > 12 {
        return;
    }
    match v.deref_obj() {
        Value::Dict(d) => {
            for (k, x) in d.borrow().iter() {
                let ks = k.as_str().unwrap_or("");
                if ks == "model" {
                    if let Some(s) = x.as_str() {
                        if s.ends_with(".model") {
                            let t = if under_hull { &mut *hull } else { &mut *other };
                            if !t.iter().any(|y| y == s) {
                                t.push(s.to_string());
                            }
                        }
                    }
                    continue;
                }
                let h = under_hull || (depth == 0 && ks.ends_with("_Hull"));
                collect_models(x, h, hull, other, depth + 1);
            }
        }
        Value::List(l) => {
            for x in l.borrow().iter() {
                collect_models(x, under_hull, hull, other, depth + 1);
            }
        }
        Value::Tuple(t) => {
            for x in t.iter() {
                collect_models(x, under_hull, hull, other, depth + 1);
            }
        }
        _ => {}
    }
}

/// 从 GameParams_py3.data 原始字节解析全部舰船。
pub fn ships_from_raw(raw: &[u8]) -> Result<Vec<ShipParam>> {
    let u = decompress(raw)?;
    let top = pickle::load(&u)?;
    let dict = match &top {
        Value::Tuple(t) => t.first().cloned().unwrap_or(Value::None),
        v => v.clone(),
    };
    let mut ships = Vec::new();
    for (_, v) in dict.items() {
        let ti = v.get("typeinfo").unwrap_or(Value::None);
        if ti.get("type").as_ref().and_then(|x| x.as_str()) != Some("Ship") {
            continue;
        }
        let s = |k: &str| v.get(k).and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default();
        let (mut hull, mut other) = (Vec::new(), Vec::new());
        collect_models(&v, false, &mut hull, &mut other, 0);
        ships.push(ShipParam {
            index: s("index"),
            name: s("name"),
            level: v.get("level").and_then(|x| x.as_i64()).unwrap_or(0),
            nation: ti.get("nation").and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default(),
            species: ti.get("species").and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default(),
            group: s("group"),
            hull_models: hull,
            other_models: other,
        });
    }
    ships.sort_by(|a, b| a.index.cmp(&b.index));
    Ok(ships)
}

/// pickle 值 → JSON(对象取 state; 深度上限防止环)
pub fn to_json(v: &Value, depth: usize) -> serde_json::Value {
    use serde_json::Value as J;
    if depth > 24 {
        return J::Null;
    }
    match v.deref_obj() {
        Value::None | Value::Mark => J::Null,
        Value::Bool(b) => J::Bool(b),
        Value::Int(i) => J::from(i),
        Value::BigInt(_) => J::Null,
        Value::Float(f) => serde_json::Number::from_f64(f).map(J::Number).unwrap_or(J::Null),
        Value::Str(s) => J::String(s.to_string()),
        Value::Bytes(b) => J::String(String::from_utf8_lossy(&b).into_owned()),
        Value::List(l) => J::Array(l.borrow().iter().map(|x| to_json(x, depth + 1)).collect()),
        Value::Tuple(t) => J::Array(t.iter().map(|x| to_json(x, depth + 1)).collect()),
        Value::Dict(d) => {
            let mut m = serde_json::Map::new();
            for (k, x) in d.borrow().iter() {
                let ks = match k {
                    Value::Str(s) => s.to_string(),
                    Value::Int(i) => i.to_string(),
                    _ => continue,
                };
                m.insert(ks, to_json(x, depth + 1));
            }
            J::Object(m)
        }
        Value::Class(c) => J::String(c.to_string()),
        Value::Object(_) => J::Null,
    }
}

/// 取一条舰船条目的完整 JSON。`key` 可以是 name(PFSB510_Bourgogne)或 index(PFSB510)。
pub fn ship_json(raw: &[u8], key: &str) -> Result<(String, serde_json::Value)> {
    let u = decompress(raw)?;
    let top = pickle::load(&u)?;
    let dict = match &top {
        Value::Tuple(t) => t.first().cloned().unwrap_or(Value::None),
        v => v.clone(),
    };
    for (_, v) in dict.items() {
        let name = v.get("name").and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default();
        let index = v.get("index").and_then(|x| x.as_str().map(str::to_string)).unwrap_or_default();
        if name == key || index == key {
            return Ok((name, to_json(&v, 0)));
        }
    }
    Err(Error::NotFound(format!("GameParams 中没有 {key}")))
}
