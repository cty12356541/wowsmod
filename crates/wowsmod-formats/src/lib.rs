//! wowsmod-formats: 莱服《战舰世界》(Мир кораблей, Lesta) 客户端资源格式库。
//!
//! 模块:
//! - [`idx`]       : `bin/<build>/idx/*.idx` 索引 (V40, 48 字节 FileInfo)
//! - [`index`]     : 把整个 idx 目录合并成可检索的虚拟文件系统
//! - [`pkg`]       : `res_packages/*.pkg` 条目读取 + 解压
//! - [`compression`]: 莱服 Oodle 容器 / deflate / 存储; Oodle 后端可替换
//! - [`geometry`]  : `.geometry` 读/写(逐字节往返) + meshopt 编解码
//! - [`vertex`]    : 顶点格式串解析与属性解包
//! - [`assets_bin`]: `content/assets.bin` 中 VisualPrototype 渲染集(权威的顶点/索引配对)
//! - [`pairing`]   : 顶点池↔索引按 mapping id 配对(assets.bin / .visual 名字 murmur3 / 启发式兜底)
//! - [`glb`]       : 最小 glTF 2.0 二进制(GLB)读写
//! - [`health`]    : 网格健康检查(索引越界 / 尖刺三角形)
//! - [`murmur3`]   : MurmurHash3 x86_32
//! - [`mo`]        : gettext .mo 读取(船名本地化检索)
//! - [`bc7prep`]   : 贴图 BC7Prep(Oodle Texture 重排)逆变换 → 标准 BC7
//! - [`texture`]   : BC1–5/BC7 解码/编码、mip 生成、PNG 读写
//! - [`dds`]       : DDS 头解析/写出 + 莱服 .dd0/.dd1/.dd2 分卷拆分与合并
//! - [`color`]     : sRGB/线性/OKLab/HSL 颜色空间
//! - [`recolor`]   : 保留细节的换色、遮罩、_mg 发光通道写入
//! - [`uvmask`]    : 由网格三角形(材质/网格名/三维盒)光栅化 UV 遮罩
//! - [`mfm`]       : 明文 .mfm 编辑(fx/属性)与 SDK compile.info 编解码
//!
//! 部分实现参考 landaire/wows-toolkit (MIT, Copyright 2026 Lander Brandt),
//! 具体文件头部有注明。

pub mod anim;
pub mod assemble;
pub mod assets_bin;
pub mod bc7prep;
pub mod color;
pub mod compression;
pub mod dds;
pub mod error;
pub mod export;
pub mod gameparams;
pub mod geometry;
pub mod geomedit;
pub mod glb;
pub mod health;
pub mod idx;
pub mod index;
pub mod mfm;
pub mod mo;
pub mod murmur3;
pub mod pairing;
pub mod pickle;
pub mod pkg;
pub mod recolor;
pub mod scene;
pub mod texture;
pub mod util;
pub mod uvmask;
pub mod vertex;
pub mod vertex_codec;
pub mod visual;
pub mod zipfile;

pub use error::{Error, Result};
