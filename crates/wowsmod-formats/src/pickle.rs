//! 最小的 Python pickle(协议 2)读取器, 只为解析 content/GameParams_py3.data。
//!
//! 只实现 GameParams 实际用到的操作码(26.10 实测: 见 [`gameparams`](crate::gameparams) 模块文档)。
//! 遇到未知操作码返回 Unsupported 错误, 不会静默跳过。
//! 不执行任何代码: GLOBAL/NEWOBJ/REDUCE 只记录类名, 生成 [`Value::Object`]。
use crate::error::{Error, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    /// 超出 i64 的 LONG1, 以小端字节保存
    BigInt(Rc<Vec<u8>>),
    Float(f64),
    Str(Rc<str>),
    Bytes(Rc<Vec<u8>>),
    List(Rc<RefCell<Vec<Value>>>),
    Tuple(Rc<Vec<Value>>),
    Dict(Rc<RefCell<Vec<(Value, Value)>>>),
    /// GLOBAL 得到的类引用 "module name"
    Class(Rc<str>),
    /// NEWOBJ/REDUCE 生成的对象; state 由 BUILD 设置
    Object(Rc<Obj>),
    Mark,
}

#[derive(Debug)]
pub struct Obj {
    pub class: Rc<str>,
    pub args: Vec<Value>,
    pub state: RefCell<Option<Value>>,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }
    /// 对象取其 state, 其余原样
    pub fn deref_obj(&self) -> Value {
        match self {
            Value::Object(o) => o.state.borrow().clone().unwrap_or(Value::None),
            v => v.clone(),
        }
    }
    /// 字典(或对象 state 字典)按字符串键取值
    pub fn get(&self, key: &str) -> Option<Value> {
        match self.deref_obj() {
            Value::Dict(d) => d.borrow().iter().find(|(k, _)| k.as_str() == Some(key)).map(|(_, v)| v.clone()),
            _ => None,
        }
    }
    /// 字典(或对象 state 字典)的全部 (字符串键, 值)
    pub fn items(&self) -> Vec<(String, Value)> {
        match self.deref_obj() {
            Value::Dict(d) => d.borrow().iter().filter_map(|(k, v)| k.as_str().map(|k| (k.to_string(), v.clone()))).collect(),
            _ => Vec::new(),
        }
    }
}

fn need(d: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    d.get(p..p + n).ok_or_else(|| Error::format("pickle", format!("数据在偏移 {p} 处被截断")))
}

/// 解析 pickle 字节流, 返回顶层对象。
pub fn load(d: &[u8]) -> Result<Value> {
    let mut stack: Vec<Value> = Vec::with_capacity(1024);
    let mut memo: HashMap<u32, Value> = HashMap::new();
    let mut p = 0usize;
    let pop_mark = |stack: &mut Vec<Value>| -> Result<Vec<Value>> {
        let m = stack.iter().rposition(|v| matches!(v, Value::Mark)).ok_or_else(|| Error::format("pickle", "缺少 MARK"))?;
        let items = stack.split_off(m + 1);
        stack.pop();
        Ok(items)
    };
    macro_rules! pop {
        () => {
            stack.pop().ok_or_else(|| Error::format("pickle", format!("栈为空 (偏移 {p})")))?
        };
    }
    loop {
        let op = *d.get(p).ok_or_else(|| Error::format("pickle", "缺少 STOP"))?;
        p += 1;
        match op {
            0x80 => p += 1, // PROTO
            b'.' => return stack.pop().ok_or_else(|| Error::format("pickle", "STOP 时栈为空")),
            b'(' => stack.push(Value::Mark),
            b'}' => stack.push(Value::Dict(Rc::new(RefCell::new(Vec::new())))),
            b']' => stack.push(Value::List(Rc::new(RefCell::new(Vec::new())))),
            b')' => stack.push(Value::Tuple(Rc::new(Vec::new()))),
            b'N' => stack.push(Value::None),
            0x88 => stack.push(Value::Bool(true)),
            0x89 => stack.push(Value::Bool(false)),
            b'J' => {
                let b = need(d, p, 4)?;
                stack.push(Value::Int(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64));
                p += 4;
            }
            b'K' => {
                stack.push(Value::Int(need(d, p, 1)?[0] as i64));
                p += 1;
            }
            b'M' => {
                let b = need(d, p, 2)?;
                stack.push(Value::Int(u16::from_le_bytes([b[0], b[1]]) as i64));
                p += 2;
            }
            0x8a => {
                // LONG1
                let n = need(d, p, 1)?[0] as usize;
                let b = need(d, p + 1, n)?;
                p += 1 + n;
                if n <= 8 {
                    let mut v: i64 = 0;
                    for (i, &x) in b.iter().enumerate() {
                        v |= (x as i64) << (8 * i);
                    }
                    if n > 0 && n < 8 && b[n - 1] & 0x80 != 0 {
                        v -= 1i64 << (8 * n);
                    }
                    stack.push(Value::Int(v));
                } else {
                    stack.push(Value::BigInt(Rc::new(b.to_vec())));
                }
            }
            b'G' => {
                let b = need(d, p, 8)?;
                stack.push(Value::Float(f64::from_be_bytes(b.try_into().unwrap())));
                p += 8;
            }
            b'X' => {
                let b = need(d, p, 4)?;
                let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
                let s = need(d, p + 4, n)?;
                stack.push(Value::Str(Rc::from(String::from_utf8_lossy(s).as_ref())));
                p += 4 + n;
            }
            0x8c => {
                // SHORT_BINUNICODE (协议 4, 以防万一)
                let n = need(d, p, 1)?[0] as usize;
                let s = need(d, p + 1, n)?;
                stack.push(Value::Str(Rc::from(String::from_utf8_lossy(s).as_ref())));
                p += 1 + n;
            }
            b'U' => {
                let n = need(d, p, 1)?[0] as usize;
                stack.push(Value::Bytes(Rc::new(need(d, p + 1, n)?.to_vec())));
                p += 1 + n;
            }
            b'T' => {
                let b = need(d, p, 4)?;
                let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
                stack.push(Value::Bytes(Rc::new(need(d, p + 4, n)?.to_vec())));
                p += 4 + n;
            }
            b'q' => {
                let k = need(d, p, 1)?[0] as u32;
                p += 1;
                memo.insert(k, stack.last().cloned().ok_or_else(|| Error::format("pickle", "BINPUT 时栈为空"))?);
            }
            b'r' => {
                let b = need(d, p, 4)?;
                let k = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                p += 4;
                memo.insert(k, stack.last().cloned().ok_or_else(|| Error::format("pickle", "LONG_BINPUT 时栈为空"))?);
            }
            b'h' => {
                let k = need(d, p, 1)?[0] as u32;
                p += 1;
                stack.push(memo.get(&k).cloned().ok_or_else(|| Error::format("pickle", format!("memo {k} 不存在")))?);
            }
            b'j' => {
                let b = need(d, p, 4)?;
                let k = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                p += 4;
                stack.push(memo.get(&k).cloned().ok_or_else(|| Error::format("pickle", format!("memo {k} 不存在")))?);
            }
            b's' => {
                let v = pop!();
                let k = pop!();
                match stack.last() {
                    Some(Value::Dict(dd)) => dd.borrow_mut().push((k, v)),
                    _ => return Err(Error::format("pickle", "SETITEM 目标不是字典")),
                }
            }
            b'u' => {
                let items = pop_mark(&mut stack)?;
                match stack.last() {
                    Some(Value::Dict(dd)) => {
                        let mut dd = dd.borrow_mut();
                        for kv in items.chunks(2) {
                            if kv.len() == 2 {
                                dd.push((kv[0].clone(), kv[1].clone()));
                            }
                        }
                    }
                    _ => return Err(Error::format("pickle", "SETITEMS 目标不是字典")),
                }
            }
            b'a' => {
                let v = pop!();
                match stack.last() {
                    Some(Value::List(l)) => l.borrow_mut().push(v),
                    _ => return Err(Error::format("pickle", "APPEND 目标不是列表")),
                }
            }
            b'e' => {
                let items = pop_mark(&mut stack)?;
                match stack.last() {
                    Some(Value::List(l)) => l.borrow_mut().extend(items),
                    _ => return Err(Error::format("pickle", "APPENDS 目标不是列表")),
                }
            }
            b't' => {
                let items = pop_mark(&mut stack)?;
                stack.push(Value::Tuple(Rc::new(items)));
            }
            0x85 | 0x86 | 0x87 => {
                let n = (op - 0x84) as usize;
                if stack.len() < n {
                    return Err(Error::format("pickle", "TUPLEn 栈不足"));
                }
                let items = stack.split_off(stack.len() - n);
                stack.push(Value::Tuple(Rc::new(items)));
            }
            b'c' => {
                // GLOBAL "module\nname\n"
                let e1 = d[p..].iter().position(|&c| c == b'\n').ok_or_else(|| Error::format("pickle", "GLOBAL 格式错误"))?;
                let e2 = d[p + e1 + 1..].iter().position(|&c| c == b'\n').ok_or_else(|| Error::format("pickle", "GLOBAL 格式错误"))?;
                let s = format!("{} {}", String::from_utf8_lossy(&d[p..p + e1]), String::from_utf8_lossy(&d[p + e1 + 1..p + e1 + 1 + e2]));
                p += e1 + e2 + 2;
                stack.push(Value::Class(Rc::from(s.as_str())));
            }
            0x81 | b'R' => {
                // NEWOBJ(cls, args) / REDUCE(callable, args)
                let args = pop!();
                let cls = pop!();
                let class: Rc<str> = match cls {
                    Value::Class(c) => c,
                    _ => Rc::from("?"),
                };
                let args = match args {
                    Value::Tuple(t) => t.as_ref().clone(),
                    _ => Vec::new(),
                };
                // set(list) → 当作列表
                if &*class == "__builtin__ set" || &*class == "builtins set" {
                    let items = args.first().cloned().unwrap_or(Value::None);
                    let l = match items {
                        Value::List(l) => l,
                        _ => Rc::new(RefCell::new(Vec::new())),
                    };
                    stack.push(Value::List(l));
                } else {
                    stack.push(Value::Object(Rc::new(Obj { class, args, state: RefCell::new(None) })));
                }
            }
            b'b' => {
                let st = pop!();
                match stack.last() {
                    Some(Value::Object(o)) => *o.state.borrow_mut() = Some(st),
                    _ => return Err(Error::format("pickle", "BUILD 目标不是对象")),
                }
            }
            0x94 => {
                // MEMOIZE (协议 4)
                let k = memo.len() as u32;
                memo.insert(k, stack.last().cloned().ok_or_else(|| Error::format("pickle", "MEMOIZE 时栈为空"))?);
            }
            other => {
                return Err(Error::Unsupported(format!("pickle 操作码 {other:#04x} (偏移 {}) 未实现", p - 1)));
            }
        }
    }
}
