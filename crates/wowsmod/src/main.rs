//! wowsmod —— 莱服《战舰世界》(Мир кораблей) AI 改模工具箱命令行。
//!
//! 子命令: doctor / find / extract / export / check / geom / tex / mask / glow / mod / install / uninstall / assemble / model
//! 所有子命令支持 `--json`(机器可读输出, 供 AI 代理调用)。
mod ai;
mod design;
mod design_preview;
mod fxcmd;
mod shipcolor;
mod game;
mod guide;
mod license;
mod m2cmd;
mod m3cmd;
mod m4cmd;
mod meshsrc;
mod mcp;
mod preview;
mod modproj;
mod oodle_rt;
mod registry;
mod render;
mod texcmd;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use serde_json::{json, Value};

use wowsmod_formats::{
    assets_bin::AssetsBin,
    compression,
    export as gexport,
    geometry::Geometry,
    glb, health,
    index::{Entry, GameIndex},
    pairing::{self, PairSource, RenderSetRef},
    pkg::PkgReader,
};

#[derive(Parser)]
#[command(name = "wowsmod", version, about = "莱服《战舰世界》AI 改模工具箱 / Lesta WoWS modding toolkit", long_about = None, disable_help_subcommand = true)]
struct Cli {
    /// 游戏根目录(含 res_packages 与 bin), 不指定则自动查找 / game root (auto-detected if omitted)
    #[arg(long, global = true, value_name = "DIR")]
    game: Option<PathBuf>,
    /// 输出 JSON / machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,
    /// 把 JSON 结果按 UTF-8 写进文件(隐含 --json; 适合 PowerShell 等会改编码的管道) / write JSON result to file (UTF-8)
    #[arg(long, global = true, value_name = "FILE")]
    json_out: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 体检: 定位游戏、版本、idx/pkg、res_mods 状态、Blender / environment report
    Doctor,
    /// 检索资源: glob(含 * ? [)或关键字; --ship 按船名/代号 / search assets
    Find {
        /// glob(如 "*FSB510*.geometry")或关键字(如 Bourgogne); --ship 时为船名/代号
        pattern: String,
        /// 按船名(含本地化名)或代号检索 / search by ship name or code
        #[arg(long)]
        ship: bool,
        /// 人类可读输出最多显示多少条(JSON 不受限)
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// 解包: 纯 Rust 读取 pkg(含莱服 Oodle) / extract files
    Extract {
        /// 一个或多个 glob / 关键字
        #[arg(required_unless_present_any = ["ship", "list"])]
        patterns: Vec<String>,
        /// 从文本文件读取精确的游戏内路径(每行一个; 空行与 # 开头的行忽略)
        #[arg(long)]
        list: Option<PathBuf>,
        /// 解包整条船(船体 + 炮塔等全部模型目录 + 对应贴图); 值为 find --ship 给出的键, 如 PFSB510
        #[arg(long)]
        ship: Option<String>,
        /// 输出目录(保持游戏内相对路径)
        #[arg(short, long)]
        out: PathBuf,
        /// 不校验 idx 中的 crc32
        #[arg(long)]
        no_verify: bool,
        /// 只列出将要解包的文件
        #[arg(long)]
        dry_run: bool,
        /// 线程数(默认 = CPU 数)
        #[arg(long)]
        threads: Option<usize>,
        /// 保持 pkg 中的原始字节(不把 BC7Prep 贴图还原成标准 BC7) / keep raw bytes, skip BC7Prep decode
        #[arg(long)]
        raw: bool,
    },
    /// 导出 geometry 为 GLB(按 mapping id 正确配对 + 健康检查) / export geometry to GLB
    Export {
        /// 本地 .geometry 文件, 或游戏内路径 / glob(匹配多个时 -o 视为目录)
        input: String,
        /// 输出 .glb(或目录)
        #[arg(short, long)]
        out: PathBuf,
        /// 配对来源
        #[arg(long, value_enum, default_value_t = PairingMode::Auto)]
        pairing: PairingMode,
        /// 指定 .visual XML(SDK 文本格式)
        #[arg(long)]
        visual: Option<PathBuf>,
        /// 指定本地 assets.bin(默认从游戏 pkg 读取 content/assets.bin)
        #[arg(long)]
        assets_bin: Option<PathBuf>,
        /// LOD: 0(默认)=主模型, 1/2/3/4…, 或 all
        #[arg(long, default_value = "0")]
        lod: String,
        /// 健康检查不通过时以退出码 2 结束
        #[arg(long)]
        strict: bool,
        /// 不做逐 primitive 绕序规范化(保持统一的 Z 取反 + (a,c,b) 变换)
        #[arg(long)]
        keep_winding: bool,
    },
    /// 健康检查 GLB: 索引越界 + 尖刺三角形 / check GLB health
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// geometry 信息与往返校验(本地文件或游戏内 glob) / geometry info & round-trip
    Geom {
        input: String,
        /// 逐字节往返校验: 重排版 + meshopt 解码再编码
        #[arg(long)]
        roundtrip: bool,
        /// 人类可读输出最多显示多少个文件的详情
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// 贴图: info / export(DDS→PNG) / import(PNG→DDS) / rewrite / recolor / texture tools
    #[command(subcommand)]
    Tex(m2cmd::TexCmd),
    /// 生成遮罩 PNG(按 GLB 部件/三维盒、UV 矩形、遮罩图), 可叠加到贴图上预览 / build a UV mask
    Mask {
        #[command(flatten)]
        mask: texcmd::MaskArgs,
        /// 遮罩尺寸取自该贴图(并可 --overlay 预览)
        #[arg(long)]
        like: Option<String>,
        /// 遮罩尺寸 WxH(如 4096x4096)
        #[arg(long)]
        size: Option<String>,
        #[arg(short, long)]
        out: PathBuf,
        /// 把遮罩以品红叠到 --like 贴图上, 写预览 PNG
        #[arg(long)]
        overlay: Option<PathBuf>,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// 结构线稿遮罩: 按几何折边/部件轮廓/世界等值线/圆环/法线板缝在 UV 里生成贴合船体的线(复刻/设计用) / structure-fitted line mask
    Lineart(design::LineartArgs),
    /// 把同机位的设计图/参考图投影进材质 UV, 可按颜色抠遮罩 / project a reference/design image into UV
    BakeRef(design::BakeRefArgs),
    /// 设计模板: 参考分析 / 拆解清单 / 分层验收清单(markdown, 复制到项目里填写) / design worksheets
    DesignTemplate(design::TemplateArgs),
    /// 设计书/拆解书: init / check / revise / finalize(定稿后再执行; 未定稿时执行类命令会提示) / design book
    #[command(subcommand)]
    DesignBook(design::BookCmd),
    /// 方向粗样一键出图: 按 spec(TOML) 为每个方向复制基础项目、跑一串 wowsmod 命令、同机位出图并拼总表 / one-shot direction previews
    DesignPreview(design_preview::PreviewArgs),
    /// 整船同色: 船体/上层建筑/炮塔/指挥仪/雷达按组一次换色(内部逐张 tex recolor + 清 _mg B) / recolor whole ship by part groups
    ShipColor(shipcolor::ShipColorArgs),
    /// 特效层: 炮口焰/开火烟/烟囱烟/航迹 — 按类查贴图、看船的特效绑定、按项目 fx.json 主题换色(全局生效) / FX layer
    #[command(subcommand)]
    Fx(fxcmd::FxCmd),
    /// 发光: 写 _mg 的 B 通道(发光遮罩/强度)、把发光区漫反射染色、切换 .mfm 到 emissive 着色器 / emissive edit
    Glow(m2cmd::GlowArgs),
    /// 颜色换算: 想在港口里"看到"某颜色时贴图该用什么颜色(按勃艮第港口实测标定, 近似) / port colour compensation
    PortAlbedo {
        /// 希望在港口截图里看到的颜色 #RRGGBB
        screen: String,
        /// recolor 的 --to-color 是选区平均色; 船体侧板比选区平均亮约这么多(sRGB 0-255)
        #[arg(long, default_value_t = 10.7)]
        mean_offset: f32,
    },
    /// mod 项目: init / import-sdk / status
    #[command(subcommand)]
    Mod(m2cmd::ModCmd),
    /// 安装 mod 项目或 .wsmod 包到 res_mods(登记每个文件的哈希/备份, 检查与已装 mod 的冲突, 失败自动回滚)
    Install(m4cmd::InstallArgs),
    /// 整船装配: SDK visual + geometry + GameParams 挂点 → 带层级的 GLB 与装配表 JSON / assemble a whole ship
    Assemble(m3cmd::AssembleArgs),
    /// 模型写回: list / scale / move / rotate / replace / remove / add / rebuild / export-blender / import-blender / undo / replay
    #[command(subcommand)]
    Model(m3cmd::ModelCmd),
    /// 卸载: 只删哈希与安装时一致的文件, 还原被覆盖的原文件, 与其他 mod 共用的保留; 最后一个卸载后 res_mods 回到原样
    Uninstall(m4cmd::UninstallArgs),
    /// 列出已装的 mod(--verify 逐文件核对哈希与备份)
    Installed(m4cmd::InstalledArgs),
    /// 不安装, 只检查几个项目/包之间以及与已装 mod 的冲突
    Conflicts(m4cmd::ConflictsArgs),
    /// 游戏更新(bin 下出现新版本号目录)后, 把 mod 从旧 res_mods 迁到新的(按新版本重新补齐原版资源)
    Migrate(m4cmd::MigrateArgs),
    /// 打成可分发的 .wsmod 包(不含游戏原版资源与任何专有组件)
    Pack(m4cmd::PackArgs),
    /// 出图看效果: 默认 Edge/Chrome 无头 WebGL(API.md §11); --engine soft 为软件光栅
    Render(render::RenderArgs),
    /// 启动本机预览服务(嵌入前端 + /api/v1), 默认打开浏览器 / local preview server
    Preview(preview::PreviewArgs),
    /// 总览: 游戏、已装 mod、本机 SDK 素材、项目状态(AI 的第一步) / overview
    Status(ai::StatusArgs),
    /// 技能树(渐进阅读): wowsmod help [节点id] [--children] [--tree] [--search 词] [--check] / skill tree help
    Help {
        /// 节点 id(点分路径, 如 workflow.layers.paint; 省略则显示树的前两层); 也可写旧主题名或命令名
        topic: Option<String>,
        /// 在全部节点里搜索
        #[arg(long)]
        search: Option<String>,
        /// 显示(子)树
        #[arg(long)]
        tree: bool,
        /// 只显示上级与下级索引
        #[arg(long)]
        children: bool,
        /// 校验技能树(格式/孤儿/死链/长度)并列出各节点字数
        #[arg(long)]
        check: bool,
    },
    /// MCP 服务器(stdio): 给 Cursor / Claude Code 等 AI 工具调用 / MCP server
    Mcp,
    /// AI 工具接入: ai init <目录> 写 AGENTS.md 与各工具的规则副本、MCP 配置
    #[command(subcommand)]
    Ai(ai::AiCmd),
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum, Debug)]
enum PairingMode {
    /// 自动: --visual / 同目录 .visual > assets.bin > 启发式
    Auto,
    /// 只用 assets.bin 渲染集
    Assets,
    /// 只用 .visual XML(murmur3 名字)
    Visual,
    /// 启发式(texel density + 顶点数), 不推荐
    Heuristic,
    /// 按数组下标两两配对(M0 的错误做法, 仅用于对照演示)
    Naive,
}

fn main() {
    use clap::{CommandFactory, FromArgMatches};
    let m = guide::decorate(Cli::command()).get_matches();
    let mut cli = match Cli::from_arg_matches(&m) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    oodle_rt::install(cli.game.as_deref());
    let json_out = cli.json_out.clone();
    if json_out.is_some() {
        cli.json = true;
    }
    let json_mode = cli.json;
    let key = guide::cmd_key();
    let write_out = |v: &Value| {
        if let Some(f) = &json_out {
            if let Some(d) = f.parent().filter(|d| !d.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(d);
            }
            if let Err(e) = std::fs::write(f, serde_json::to_string_pretty(v).unwrap_or_default()) {
                eprintln!("写 --json-out {} 失败: {e}", f.display());
            }
        }
    };
    match run(cli) {
        Ok(code) => {
            if json_out.is_some() {
                let v = guide::peek().map(|v| guide::with_next(&v)).unwrap_or_else(|| json!({"ok": code == 0}));
                write_out(&v);
            }
            if !json_mode {
                guide::print_next_human(code);
            }
            std::process::exit(code)
        }
        Err(e) => {
            let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
            let hint = guide::error_hint(&key, &chain.join(" | "));
            write_out(&json!({"ok": false, "error": chain, "next": hint}));
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&json!({"ok": false, "error": chain, "next": hint})).unwrap());
            } else {
                eprintln!("错误 / Error: {e}");
                for c in e.chain().skip(1) {
                    eprintln!("  原因 / caused by: {c}");
                }
                if !hint.is_empty() {
                    eprintln!("建议下一步:");
                    for h in hint {
                        eprintln!("  - {h}");
                    }
                }
            }
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    // 执行类命令: 项目里的设计书未定稿时提示
    if matches!(cli.cmd, Cmd::Tex(_) | Cmd::Mask { .. } | Cmd::Glow(_) | Cmd::Lineart(_) | Cmd::BakeRef(_) | Cmd::Model(_) | Cmd::Install(_)) {
        modproj::EXEC_CMD.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    match &cli.cmd {
        Cmd::Doctor => cmd_doctor(&cli),
        Cmd::Find { pattern, ship, limit } => cmd_find(&cli, pattern, *ship, *limit),
        Cmd::Extract { patterns, list, ship, out, no_verify, dry_run, threads, raw } => {
            cmd_extract(&cli, patterns, list.as_deref(), ship.as_deref(), out, *no_verify, *dry_run, *threads, *raw)
        }
        Cmd::Export { input, out, pairing, visual, assets_bin, lod, strict, keep_winding } => {
            cmd_export(&cli, input, out, *pairing, visual.as_deref(), assets_bin.as_deref(), lod, *strict, *keep_winding)
        }
        Cmd::Check { files } => cmd_check(&cli, files),
        Cmd::Geom { input, roundtrip, limit } => cmd_geom(&cli, input, *roundtrip, *limit),
        Cmd::Tex(c) => m2cmd::cmd_tex(&cli, c),
        Cmd::Mask { mask, like, size, out, overlay, project } => m2cmd::cmd_mask(&cli, mask, like.as_deref(), size.as_deref(), out, overlay.as_deref(), project),
        Cmd::Glow(a) => m2cmd::cmd_glow(&cli, a),
        Cmd::PortAlbedo { screen, mean_offset } => m2cmd::cmd_port_albedo(&cli, screen, *mean_offset),
        Cmd::Mod(c) => m2cmd::cmd_mod(&cli, c),
        Cmd::Install(a) => m4cmd::cmd_install(&cli, a),
        Cmd::Uninstall(a) => m4cmd::cmd_uninstall(&cli, a),
        Cmd::Installed(a) => m4cmd::cmd_installed(&cli, a),
        Cmd::Conflicts(a) => m4cmd::cmd_conflicts(&cli, a),
        Cmd::Migrate(a) => m4cmd::cmd_migrate(&cli, a),
        Cmd::Pack(a) => m4cmd::cmd_pack(&cli, a),
        Cmd::Render(a) => render::cmd_render(&cli, a),
        Cmd::Lineart(a) => design::cmd_lineart(&cli, a),
        Cmd::BakeRef(a) => design::cmd_bake_ref(&cli, a),
        Cmd::DesignTemplate(a) => design::cmd_template(&cli, a),
        Cmd::DesignBook(c) => design::cmd_book(&cli, c),
        Cmd::DesignPreview(a) => design_preview::cmd(&cli, a),
        Cmd::ShipColor(a) => shipcolor::cmd(&cli, a),
        Cmd::Fx(c) => fxcmd::cmd(&cli, c),
        Cmd::Preview(a) => preview::cmd_preview(&cli, a),
        Cmd::Status(a) => ai::cmd_status(&cli, a),
        Cmd::Help { topic, search, tree, children, check } => ai::cmd_help(&cli, topic.as_deref(), search.as_deref(), *tree, *children, *check),
        Cmd::Mcp => mcp::serve(&cli),
        Cmd::Ai(c) => ai::cmd_ai(&cli, c),
        Cmd::Assemble(a) => m3cmd::cmd_assemble(&cli, a),
        Cmd::Model(c) => m3cmd::cmd_model(&cli, c),
    }
}

fn emit(cli: &Cli, v: &Value) {
    guide::stash(v);
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&guide::with_next(v)).unwrap());
    }
}

fn human_size(b: u64) -> String {
    let f = b as f64;
    if f >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GiB", f / 1024.0 / 1024.0 / 1024.0)
    } else if f >= 1024.0 * 1024.0 {
        format!("{:.1} MiB", f / 1024.0 / 1024.0)
    } else if f >= 1024.0 {
        format!("{:.1} KiB", f / 1024.0)
    } else {
        format!("{b} B")
    }
}

fn require_game(cli: &Cli) -> Result<game::GameInstall> {
    let (gi, cands) = game::locate(cli.game.as_deref());
    match gi {
        Some(g) => Ok(g),
        None => {
            let tried: Vec<String> = cands.iter().map(|c| format!("{} [{}]: {}", c.path, c.source, c.note)).collect();
            if let Some(p) = &cli.game {
                bail!(
                    "--game {} 不是有效的莱服客户端目录: {}\n  需要: res_packages\\ 与 bin\\<版本号>\\idx\\*.idx",
                    p.display(),
                    game::validate(p).err().unwrap_or_default()
                );
            }
            bail!(
                "没有找到游戏客户端。请用 --game <游戏根目录> 指定(例如 --game D:\\Korabli), 或设置环境变量 WOWSMOD_GAME。\n  已尝试: {}",
                if tried.is_empty() { "(无候选)".to_string() } else { tried.join("\n          ") }
            )
        }
    }
}

fn load_index(g: &game::GameInstall) -> Result<GameIndex> {
    GameIndex::load_dir(&g.idx_dir).with_context(|| format!("读取 idx 目录失败: {}", g.idx_dir.display()))
}

// ---------------------------------------------------------------- doctor
fn cmd_doctor(cli: &Cli) -> Result<i32> {
    let (gi, cands) = game::locate(cli.game.as_deref());
    let blender = game::find_blender();
    let mut report = json!({
        "wowsmod_version": env!("CARGO_PKG_VERSION"),
        "oodle_backend": compression::oodle_backend().map(|b| b.name()),
        "oodle": oodle_rt::status(),
        "candidates": cands,
        "blender": blender,
    });
    let mut ok = true;
    if let Some(g) = &gi {
        let t = Instant::now();
        let idx = load_index(g);
        let (idx_info, pkg_summary) = match &idx {
            Ok(ix) => {
                let mut pkgs: HashMap<&str, usize> = HashMap::new();
                for e in &ix.entries {
                    *pkgs.entry(e.pkg.as_str()).or_default() += 1;
                }
                let mut missing = Vec::new();
                for p in pkgs.keys() {
                    if !g.res_packages.join(p).is_file() {
                        missing.push(p.to_string());
                    }
                }
                missing.sort();
                if !missing.is_empty() {
                    ok = false;
                }
                let oodle = ix.entries.iter().filter(|e| compression::method_for(e.info.compression_info, e.info.size, e.info.unpacked_size) == compression::Method::LestaOodle).count();
                (
                    json!({"idx_files": ix.idx_files, "entries": ix.entries.len(), "duplicates": ix.duplicates, "oodle_entries": oodle, "load_ms": t.elapsed().as_millis()}),
                    json!({"referenced_pkgs": pkgs.len(), "missing_pkgs": missing}),
                )
            }
            Err(e) => {
                ok = false;
                (json!({"error": format!("{e:#}")}), Value::Null)
            }
        };
        let (pkg_count, pkg_bytes) = std::fs::read_dir(&g.res_packages)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("pkg")).unwrap_or(false))
                    .fold((0usize, 0u64), |(n, b), e| (n + 1, b + e.metadata().map(|m| m.len()).unwrap_or(0)))
            })
            .unwrap_or((0, 0));
        let rm = game::res_mods_state(&g.res_mods);
        let mods_dir = g.idx_dir.parent().map(|p| p.join("mods")).unwrap_or_default();
        let mods = game::res_mods_state(&mods_dir);
        let version_matches_build = g.client_version.as_deref().map(|v| v.ends_with(&g.build));
        report["game"] = json!({
            "root": g.root, "found_by": g.found_by, "game_id": g.game_id, "is_lesta": g.is_lesta,
            "client_version": g.client_version, "newest_build": g.build, "version_matches_build": version_matches_build,
            "bin_dirs": g.bin_dirs, "idx_dir": g.idx_dir, "res_packages": g.res_packages,
            "pkg_files": pkg_count, "pkg_bytes": pkg_bytes, "index": idx_info, "pkgs": pkg_summary,
            "res_mods": rm, "mods_dir": mods,
        });
        if !g.is_lesta {
            ok = false;
        }
    } else {
        ok = false;
    }
    report["ok"] = json!(ok);
    crate::guide::stash(&report);
    if cli.json {
        emit(cli, &report);
        return Ok(if ok { 0 } else { 1 });
    }
    println!("wowsmod {}  (Oodle 后端: {})", env!("CARGO_PKG_VERSION"), compression::oodle_backend().map(|b| b.name()).unwrap_or("未编译"));
    if compression::oodle_backend().is_none() {
        if cfg!(windows) {
            println!("提示: 未找到 Oodle 运行库(oo2core*.dll)。可设 WOWSMOD_OODLE=<dll 或目录>, 或放到 wowsmod.exe 同目录; 见 {}", oodle_rt::README_HINT);
        } else {
            println!("提示: 非 Windows 平台无法加载游戏自带的 Oodle DLL; 可用 WOWSMOD_OODLE 指向本机 liboo2core*.so。见 {}", oodle_rt::README_HINT);
        }
    }
    println!("\n[游戏客户端]");
    match &gi {
        Some(g) => {
            let r = &report["game"];
            println!("  ✔ 找到: {}", g.root.display());
            println!("    来源: {}", g.found_by);
            println!("    游戏 ID: {}  {}", g.game_id.as_deref().unwrap_or("不确定(无 game_info.xml)"), if g.is_lesta { "(莱服 Lesta)" } else { "(⚠ 不是莱服客户端, 本工具只针对莱服)" });
            println!("    客户端版本: {}", g.client_version.as_deref().unwrap_or("不确定"));
            println!("    最新 bin 目录: bin\\{}  (全部: {})", g.build, g.bin_dirs.join(", "));
            if r["version_matches_build"] == json!(false) {
                println!("    ⚠ game_info.xml 版本号与最新 bin 目录不一致, 可能正在更新");
            }
            println!("    idx: {}", g.idx_dir.display());
            if let Some(n) = r["index"]["entries"].as_u64() {
                println!("         {} 个 idx, {} 个文件条目 (Oodle 压缩 {}), 读取耗时 {} ms", r["index"]["idx_files"], n, r["index"]["oodle_entries"], r["index"]["load_ms"]);
            } else {
                println!("         ✘ 读取失败: {}", r["index"]["error"]);
            }
            println!("    pkg: {}  ({} 个, {})", g.res_packages.display(), r["pkg_files"], human_size(r["pkg_bytes"].as_u64().unwrap_or(0)));
            if let Some(m) = r["pkgs"]["missing_pkgs"].as_array() {
                if !m.is_empty() {
                    println!("         ✘ idx 引用但缺失的 pkg: {m:?}  (请在启动器里\"检查完整性\")");
                }
            }
            let rm = &r["res_mods"];
            println!("    res_mods: {}", rm["path"].as_str().unwrap_or(""));
            if rm["pristine"] == json!(true) {
                println!("         干净(只有 readme.txt), 未安装任何模组");
            } else {
                println!("         {} 个文件, {}; 顶层: {}", rm["files"], human_size(rm["bytes"].as_u64().unwrap_or(0)), rm["top_level"]);
                if let Some(p) = rm["pnf_mods"].as_array() {
                    if !p.is_empty() {
                        println!("         PnFMods 模组: {}", p.iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join(", "));
                    }
                }
            }
            let md = &r["mods_dir"];
            if md["exists"] == json!(true) {
                println!("    mods: {}  ({} 个文件; 顶层: {})", md["path"].as_str().unwrap_or(""), md["files"], md["top_level"]);
            }
        }
        None => {
            println!("  ✘ 没找到莱服客户端。请用 --game <目录> 指定, 或设置环境变量 WOWSMOD_GAME。");
        }
    }
    println!("  候选目录:");
    for c in &cands {
        println!("    {} {}  ← {}{}", if c.valid { "✔" } else { "✘" }, c.path, c.source, if c.valid { String::new() } else { format!(" ({})", c.note) });
    }
    println!("\n[Blender]");
    if blender.is_empty() {
        println!("  未找到 Blender(可选; M3 起用于预览/编辑)。安装后重新运行 doctor, 或设 WOWSMOD_BLENDER=<blender.exe>; 见 README「需要自行下载的组件」。");
    }
    for b in &blender {
        println!("  ✔ {}  版本 {}  ← {}", b.path, if b.version.is_empty() { "不确定" } else { &b.version }, b.source);
    }
    println!("\n结论: {}", if ok { "环境正常 ✔" } else { "有问题, 见上 ✘" });
    Ok(if ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- find
fn ship_index(g: &game::GameInstall, ix: &GameIndex) -> (HashMap<String, String>, Option<String>) {
    // 代号 → 本地化名, 来自 bin/<build>/res/texts/*/LC_MESSAGES/global.mo
    let mut names = HashMap::new();
    let texts = g.idx_dir.parent().map(|p| p.join("res").join("texts")).unwrap_or_default();
    let mut mo_path = None;
    if let Ok(rd) = std::fs::read_dir(&texts) {
        for e in rd.flatten() {
            let p = e.path().join("LC_MESSAGES").join("global.mo");
            if let Ok(d) = std::fs::read(&p) {
                if let Ok(m) = wowsmod_formats::mo::parse_mo(&d) {
                    for (k, v) in m {
                        if let Some(code) = k.strip_prefix("IDS_P") {
                            if code.len() >= 6 && code.len() <= 8 && code.chars().all(|c| c.is_ascii_alphanumeric()) && code.chars().take(3).all(|c| c.is_ascii_uppercase()) {
                                names.insert(code.to_string(), v);
                            }
                        }
                    }
                    mo_path = Some(p.display().to_string());
                    break;
                }
            }
        }
    }
    let _ = ix;
    (names, mo_path)
}


/// 一条船只检索结果: GameParams 舰船(若有) + 其全部模型目录 + 文件集合
struct ShipHit {
    /// 用于 `extract --ship` 的唯一键: 参数代号(PFSB510)或模型目录代号(FSB026)
    key: String,
    param: Option<wowsmod_formats::gameparams::ShipParam>,
    localized: Option<String>,
    model_dirs: Vec<String>,
    files: Vec<String>,
    bytes: u64,
}

impl ShipHit {
    fn to_json(&self) -> Value {
        json!({
            "key": self.key, "param": self.param, "localized_name": self.localized,
            "model_dirs": self.model_dirs, "file_count": self.files.len(), "bytes": self.bytes,
            "geometry_files": self.files.iter().filter(|e| e.ends_with(".geometry")).count(),
            "extract": format!("wowsmod extract --ship {} -o <目录>", self.key),
        })
    }
}

/// 一组模型目录对应的文件: 目录下全部文件 + 同级 textures/ 下以该模型代号开头的贴图
/// (例如 FSB025_Bourgogne_1945 → battleship/textures/FSB025_*)。
fn ship_file_set(ix: &GameIndex, dirs: &[String]) -> (Vec<String>, u64) {
    let mut prefixes: Vec<String> = Vec::new();
    for d in dirs {
        prefixes.push(format!("{}/", d.to_lowercase()));
        if let Some((parent, stem)) = d.rsplit_once('/') {
            let code = stem.split('_').next().unwrap_or(stem);
            prefixes.push(format!("{}/textures/{}_", parent.to_lowercase(), code.to_lowercase()));
        }
    }
    let mut files = Vec::new();
    let mut bytes = 0u64;
    for e in &ix.entries {
        let l = e.path.to_lowercase();
        if prefixes.iter().any(|p| l.starts_with(p)) {
            files.push(e.path.clone());
            bytes += e.info.unpacked_size as u64;
        }
    }
    files.sort();
    (files, bytes)
}

/// 船只检索: GameParams(参数代号/参数名/本地化名/模型目录名) + 无参数的模型目录兜底。
fn search_ships(g: &game::GameInstall, ix: &GameIndex, reader: &PkgReader, query: &str) -> Result<(Vec<ShipHit>, Option<String>, Option<String>)> {
    let (names, mo) = ship_index(g, ix);
    let q = query.trim().to_lowercase();
    let mut hits: Vec<ShipHit> = Vec::new();
    let mut covered_dirs: std::collections::HashSet<String> = Default::default();
    let mut gp_err = None;
    let ships = match ix.get("content/GameParams_py3.data") {
        Some(e) => reader
            .read(e)
            .map_err(|x| anyhow!("{x}"))
            .and_then(|raw| wowsmod_formats::gameparams::ships_from_raw(&raw).map_err(|x| anyhow!("{x}"))),
        None => Err(anyhow!("索引中没有 content/GameParams_py3.data")),
    };
    let ships = match ships {
        Ok(s) => s,
        Err(e) => {
            gp_err = Some(e.to_string());
            Vec::new()
        }
    };
    let exact = ships.iter().any(|s| s.index.to_lowercase() == q || s.name.to_lowercase() == q);
    for s in &ships {
        for d in s.model_dirs() {
            if s.hull_models.iter().any(|m| m.starts_with(&format!("{d}/"))) {
                covered_dirs.insert(d.to_lowercase());
            }
        }
        let local = names.get(s.index.trim_start_matches('P')).cloned();
        let hull_dirs: Vec<String> = s.hull_models.iter().filter_map(|m| m.rsplit_once('/').map(|x| x.0.to_string())).collect();
        let matched = if exact {
            s.index.to_lowercase() == q || s.name.to_lowercase() == q
        } else {
            s.index.to_lowercase().contains(&q)
                || s.name.to_lowercase().contains(&q)
                || local.as_deref().map(|n| n.to_lowercase().contains(&q)).unwrap_or(false)
                || hull_dirs.iter().any(|d| d.rsplit('/').next().unwrap_or("").to_lowercase().contains(&q))
        };
        if matched {
            let dirs = s.model_dirs();
            let (files, bytes) = ship_file_set(ix, &dirs);
            hits.push(ShipHit { key: s.index.clone(), param: Some(s.clone()), localized: local, model_dirs: dirs, files, bytes });
        }
    }
    if !exact {
        // 兜底: 没有对应 GameParams 船体的船只模型目录 content/gameplay/<nation>/ship/<class>/<CODE>_<Name>
        let mut dirs: std::collections::BTreeMap<String, String> = Default::default();
        for e in &ix.entries {
            let parts: Vec<&str> = e.path.split('/').collect();
            if parts.len() > 6 && parts[0] == "content" && parts[1] == "gameplay" && parts[3] == "ship" && parts[5] != "textures" {
                let d = parts[..6].join("/");
                if !covered_dirs.contains(&d.to_lowercase()) {
                    let code = parts[5].split('_').next().unwrap_or(parts[5]).to_string();
                    dirs.entry(d).or_insert(code);
                }
            }
        }
        for (d, code) in dirs {
            let stem = d.rsplit('/').next().unwrap_or("").to_lowercase();
            if stem.contains(&q) || code.to_lowercase() == q {
                let (files, bytes) = ship_file_set(ix, std::slice::from_ref(&d));
                hits.push(ShipHit { key: code, param: None, localized: None, model_dirs: vec![d], files, bytes });
            }
        }
    }
    Ok((hits, mo, gp_err))
}

fn cmd_find(cli: &Cli, pattern: &str, ship: bool, limit: usize) -> Result<i32> {
    let g = require_game(cli)?;
    let ix = load_index(&g)?;
    if ship {
        let reader = PkgReader::new(&g.res_packages);
        let (hits, mo, gp_err) = search_ships(&g, &ix, &reader, pattern)?;
        let list: Vec<Value> = hits.iter().map(|h| h.to_json()).collect();
        let report = json!({"ok": true, "query": pattern, "mo": mo, "gameparams_error": gp_err, "ships": list});
        crate::guide::stash(&report);
        if cli.json {
            emit(cli, &report);
        } else {
            if let Some(e) = &gp_err {
                eprintln!("⚠ GameParams 解析失败, 只按模型目录名检索: {e}");
            }
            if hits.is_empty() {
                println!("没有匹配 \"{pattern}\" 的船只。可以试试参数代号(PFSB510)、模型代号(FSB025)、英文名(Bourgogne)或俄文名(Бургундия)。");
            }
            for h in &hits {
                println!(
                    "{}  {}  {}",
                    h.param.as_ref().map(|p| format!("{} ({}, {} 级 {} {})", p.name, p.index, p.level, p.nation, p.species)).unwrap_or_else(|| format!("模型目录 {}", h.key)),
                    h.localized.as_deref().unwrap_or("(无本地化名)"),
                    format!("[{} 个文件, geometry {}, 共 {}]", h.files.len(), h.files.iter().filter(|e| e.ends_with(".geometry")).count(), human_size(h.bytes)),
                );
                for d in &h.model_dirs {
                    println!("    {d}");
                }
                println!("    解包整船: wowsmod extract --ship {} -o <目录>", h.key);
            }
        }
        return Ok(if hits.is_empty() { 3 } else { 0 });
    }
    let hits = ix.search(pattern).map_err(|e| anyhow!("{e}"))?;
    let total: u64 = hits.iter().map(|e| e.info.unpacked_size as u64).sum();
    if cli.json {
        let list: Vec<Value> = hits.iter().map(|e| json!({"path": e.path, "size": e.info.unpacked_size, "packed": e.info.size, "pkg": e.pkg, "crc32": format!("{:08x}", e.info.crc32), "compression": format!("{:?}", compression::method_for(e.info.compression_info, e.info.size, e.info.unpacked_size))})).collect();
        emit(cli, &json!({"ok": true, "pattern": pattern, "count": hits.len(), "bytes": total, "files": list}));
    } else {
        for e in hits.iter().take(limit) {
            println!("{:>10}  {}  [{}]", e.info.unpacked_size, e.path, e.pkg);
        }
        if hits.len() > limit {
            println!("... 还有 {} 条未显示(--limit 调整, 或 --json 输出全部)", hits.len() - limit);
        }
        println!("共 {} 个文件, {}", hits.len(), human_size(total));
        if hits.is_empty() {
            println!("提示: 关键字按完整路径子串匹配(不区分大小写); glob 例: \"*FSB510*.geometry\"、\"content/gameplay/france/**/*.dds\"");
        }
    }
    Ok(if hits.is_empty() { 3 } else { 0 })
}

// ---------------------------------------------------------------- extract
fn safe_join(base: &Path, vpath: &str) -> Result<PathBuf> {
    let mut p = base.to_path_buf();
    for c in vpath.split('/') {
        if c.is_empty() || c == "." {
            continue;
        }
        if c == ".." || c.contains(':') || c.contains('\\') {
            bail!("拒绝不安全的路径: {vpath}");
        }
        p.push(c);
    }
    Ok(p)
}

#[allow(clippy::too_many_arguments)]
fn cmd_extract(cli: &Cli, patterns: &[String], list: Option<&Path>, ship: Option<&str>, out: &Path, no_verify: bool, dry_run: bool, threads: Option<usize>, raw: bool) -> Result<i32> {
    let g = require_game(cli)?;
    let ix = load_index(&g)?;
    let mut sel: Vec<&Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(lp) = list {
        let text = std::fs::read_to_string(lp).with_context(|| format!("读取列表 {}", lp.display()))?;
        let mut missing = Vec::new();
        for line in text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let vp = line.replace('\\', "/");
            match ix.get(&vp) {
                Some(e) => {
                    if seen.insert(e.path.as_str()) {
                        sel.push(e);
                    }
                }
                None => missing.push(vp),
            }
        }
        if !missing.is_empty() {
            bail!("列表中有 {} 个路径不在游戏索引中, 例如: {}", missing.len(), missing.iter().take(5).cloned().collect::<Vec<_>>().join(", "));
        }
    }
    if let Some(q) = ship {
        let reader = PkgReader::new(&g.res_packages);
        let (hits, _, gp_err) = search_ships(&g, &ix, &reader, q)?;
        let exact: Vec<&ShipHit> = hits.iter().filter(|h| h.key.eq_ignore_ascii_case(q)).collect();
        let chosen = if exact.len() == 1 {
            exact[0]
        } else if hits.len() == 1 {
            &hits[0]
        } else if hits.is_empty() {
            bail!("--ship {q}: 没有匹配的船只{}", gp_err.map(|e| format!(" (GameParams: {e})")).unwrap_or_default());
        } else {
            bail!(
                "--ship {q} 匹配到 {} 条船, 请用唯一的键: {}",
                hits.len(),
                hits.iter().map(|h| format!("{} ({})", h.key, h.param.as_ref().map(|p| p.name.as_str()).unwrap_or("模型目录"))).collect::<Vec<_>>().join(", ")
            );
        };
        if !cli.json {
            eprintln!("整船: {} → {} 个模型目录, {} 个文件", chosen.key, chosen.model_dirs.len(), chosen.files.len());
        }
        for f in &chosen.files {
            if let Some(e) = ix.get(f) {
                if seen.insert(e.path.as_str()) {
                    sel.push(e);
                }
            }
        }
    }
    for p in patterns {
        let hits = ix.search(p).map_err(|e| anyhow!("{e}"))?;
        if hits.is_empty() && !cli.json {
            eprintln!("⚠ \"{p}\" 没有匹配任何文件");
        }
        for e in hits {
            if seen.insert(e.path.as_str()) {
                sel.push(e);
            }
        }
    }
    if sel.is_empty() {
        bail!("没有文件匹配 {patterns:?}。先用 wowsmod find 确认路径/关键字。");
    }
    let total: u64 = sel.iter().map(|e| e.info.unpacked_size as u64).sum();
    if dry_run {
        if cli.json {
            emit(cli, &json!({"ok": true, "dry_run": true, "count": sel.len(), "bytes": total, "files": sel.iter().map(|e| &e.path).collect::<Vec<_>>()}));
        } else {
            for e in &sel {
                println!("{}", e.path);
            }
            println!("将解包 {} 个文件, {}", sel.len(), human_size(total));
        }
        return Ok(0);
    }
    std::fs::create_dir_all(out).with_context(|| format!("无法创建输出目录 {}", out.display()))?;
    let mut reader = PkgReader::new(&g.res_packages);
    reader.verify_crc = !no_verify;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads.unwrap_or(0)).build()?;
    let t = Instant::now();
    let done = AtomicUsize::new(0);
    let bytes = AtomicU64::new(0);
    let errors: std::sync::Mutex<Vec<(String, String)>> = Default::default();
    let bc7prep: std::sync::Mutex<Vec<String>> = Default::default();
    pool.install(|| {
        sel.par_iter().for_each(|e| {
            let r = (|| -> Result<u64> {
                let mut data = reader.read(e).map_err(|x| anyhow!("{x}"))?;
                if !raw && wowsmod_formats::bc7prep::is_bc7prep_dds(&data) {
                    data = wowsmod_formats::bc7prep::decode_dds(&data).map_err(|x| anyhow!("BC7Prep 还原失败(可加 --raw 保留原始字节) / BC7Prep decode failed: {x}"))?;
                    bc7prep.lock().unwrap().push(e.path.clone());
                }
                let dst = safe_join(out, &e.path)?;
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent).with_context(|| format!("创建目录 {}", parent.display()))?;
                }
                std::fs::write(&dst, &data).with_context(|| format!("写入 {}", dst.display()))?;
                Ok(data.len() as u64)
            })();
            match r {
                Ok(n) => {
                    bytes.fetch_add(n, Ordering::Relaxed);
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if !cli.json && (d % 500 == 0) {
                        eprint!("\r  {d}/{} ...", sel.len());
                        let _ = std::io::stderr().flush();
                    }
                }
                Err(err) => errors.lock().unwrap().push((e.path.clone(), format!("{err:#}"))),
            }
        })
    });
    let errors = errors.into_inner().unwrap();
    let mut bc7prep = bc7prep.into_inner().unwrap();
    bc7prep.sort();
    let el = t.elapsed();
    let report = json!({
        "ok": errors.is_empty(), "out": out, "requested": sel.len(), "extracted": done.load(Ordering::Relaxed),
        "bytes": bytes.load(Ordering::Relaxed), "crc_verified": !no_verify, "elapsed_ms": el.as_millis(),
        "raw": raw, "bc7prep_decoded": bc7prep.len(), "bc7prep_files": bc7prep,
        "errors": errors.iter().map(|(p, e)| json!({"path": p, "error": e})).collect::<Vec<_>>(),
    });
    crate::guide::stash(&report);
    if cli.json {
        emit(cli, &report);
    } else {
        eprintln!();
        println!(
            "解包完成: {}/{} 个文件, {}, 用时 {:.1}s{} → {}",
            done.load(Ordering::Relaxed),
            sel.len(),
            human_size(bytes.load(Ordering::Relaxed)),
            el.as_secs_f64(),
            if no_verify { "" } else { ", crc32 全部校验通过" },
            out.display()
        );
        if !bc7prep.is_empty() {
            println!("  其中 {} 个 BC7Prep 贴图已还原为标准 BC7(与 pfsunpack 一致; 用 --raw 可保留 pkg 原始字节)", bc7prep.len());
        }
        for (p, e) in errors.iter().take(20) {
            println!("  ✘ {p}: {e}");
        }
        if errors.len() > 20 {
            println!("  ... 共 {} 个失败", errors.len());
        }
    }
    Ok(if errors.is_empty() { 0 } else { 1 })
}

// ---------------------------------------------------------------- export
enum Source<'a> {
    Local(PathBuf),
    Game(&'a Entry),
}

#[allow(dead_code)]
pub(crate) struct GameCtx {
    install: game::GameInstall,
    index: GameIndex,
    reader: PkgReader,
}

pub(crate) fn game_ctx(cli: &Cli, need: bool) -> Result<Option<GameCtx>> {
    match require_game(cli) {
        Ok(g) => {
            let index = load_index(&g)?;
            let reader = PkgReader::new(&g.res_packages);
            Ok(Some(GameCtx { install: g, index, reader }))
        }
        Err(e) if need => Err(e),
        Err(_) => Ok(None),
    }
}

fn resolve_inputs<'a>(input: &str, ctx: Option<&'a GameCtx>, ext: &str) -> Result<Vec<Source<'a>>> {
    let p = Path::new(input);
    if p.is_file() {
        return Ok(vec![Source::Local(p.to_path_buf())]);
    }
    if p.is_dir() {
        let mut v = Vec::new();
        let mut stack = vec![p.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let q = e.path();
                if q.is_dir() {
                    stack.push(q);
                } else if q.extension().map(|x| x.eq_ignore_ascii_case(ext)).unwrap_or(false) {
                    v.push(Source::Local(q));
                }
            }
        }
        v.sort_by_key(|s| match s {
            Source::Local(p) => p.clone(),
            _ => PathBuf::new(),
        });
        return Ok(v);
    }
    let ctx = ctx.ok_or_else(|| anyhow!("{input} 不是本地文件, 而且没有找到游戏客户端来按游戏内路径查找(用 --game 指定)"))?;
    if let Some(e) = ctx.index.get(input) {
        return Ok(vec![Source::Game(e)]);
    }
    let hits = ctx.index.search(input).map_err(|e| anyhow!("{e}"))?;
    let hits: Vec<&Entry> = hits.into_iter().filter(|e| e.path.to_lowercase().ends_with(&format!(".{ext}"))).collect();
    if hits.is_empty() {
        bail!("游戏内没有匹配 {input:?} 的 .{ext} 文件(可先用 wowsmod find 查看)");
    }
    Ok(hits.into_iter().map(Source::Game).collect())
}

fn load_assets(ctx: Option<&GameCtx>, local: Option<&Path>) -> Result<Option<AssetsBin>> {
    if let Some(p) = local {
        let d = std::fs::read(p).with_context(|| format!("读取 {}", p.display()))?;
        return Ok(Some(AssetsBin::parse(d).map_err(|e| anyhow!("解析 {}: {e}", p.display()))?));
    }
    let Some(ctx) = ctx else { return Ok(None) };
    let Some(e) = ctx.index.get("content/assets.bin") else { return Ok(None) };
    let d = ctx.reader.read(e).map_err(|e| anyhow!("从 pkg 读取 content/assets.bin: {e}"))?;
    Ok(Some(AssetsBin::parse(d).map_err(|e| anyhow!("解析 assets.bin: {e}"))?))
}

struct Loaded {
    label: String,
    vpath: Option<String>,
    local: Option<PathBuf>,
    data: Vec<u8>,
}

fn load_source(src: &Source, ctx: Option<&GameCtx>) -> Result<Loaded> {
    Ok(match src {
        Source::Local(p) => {
            let data = std::fs::read(p).with_context(|| format!("读取 {}", p.display()))?;
            // 本地文件: 按文件名在游戏索引中找同名文件, 用于 assets.bin 配对
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            let vpath = ctx.and_then(|c| {
                let v: Vec<&Entry> = c.index.entries.iter().filter(|e| e.path.rsplit('/').next().map(|n| n.to_lowercase() == name).unwrap_or(false)).collect();
                if v.len() == 1 {
                    Some(v[0].path.clone())
                } else {
                    None
                }
            });
            Loaded { label: p.display().to_string(), vpath, local: Some(p.clone()), data }
        }
        Source::Game(e) => {
            let ctx = ctx.unwrap();
            let data = ctx.reader.read(e).map_err(|x| anyhow!("读取 {}: {x}", e.path))?;
            Loaded { label: e.path.clone(), vpath: Some(e.path.clone()), local: None, data }
        }
    })
}

fn stem_of(p: &str) -> String {
    let n = p.replace('\\', "/");
    let n = n.rsplit('/').next().unwrap_or(&n).to_string();
    n.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or(n)
}

#[allow(clippy::too_many_arguments)]
fn cmd_export(cli: &Cli, input: &str, out: &Path, mode: PairingMode, visual: Option<&Path>, assets_path: Option<&Path>, lod: &str, strict: bool, keep_winding: bool) -> Result<i32> {
    let local_input = Path::new(input).exists();
    let need_game = !local_input;
    let ctx = game_ctx(cli, need_game)?;
    let sources = resolve_inputs(input, ctx.as_ref(), "geometry")?;
    let multi = sources.len() > 1;
    if multi {
        std::fs::create_dir_all(out).with_context(|| format!("创建输出目录 {}", out.display()))?;
    }
    let want_assets = matches!(mode, PairingMode::Auto | PairingMode::Assets);
    let assets = if want_assets {
        let t = Instant::now();
        let a = load_assets(ctx.as_ref(), assets_path)?;
        if a.is_some() && !cli.json {
            eprintln!("已载入 assets.bin ({:.1}s)", t.elapsed().as_secs_f64());
        }
        a
    } else {
        None
    };
    if mode == PairingMode::Assets && assets.is_none() {
        bail!("--pairing assets 需要 assets.bin: 找不到游戏客户端, 也没有 --assets-bin");
    }
    let mut results = Vec::new();
    let mut all_ok = true;
    for src in &sources {
        let ld = load_source(src, ctx.as_ref())?;
        let g = Geometry::parse(&ld.data).map_err(|e| anyhow!("解析 {}: {e}", ld.label))?;
        let mut warnings: Vec<String> = Vec::new();
        let mut pairs = Vec::new();
        let mut source_used = String::new();
        // 1) .visual XML
        let vis_path = match (mode, visual) {
            (PairingMode::Auto | PairingMode::Visual, Some(v)) => Some(v.to_path_buf()),
            (PairingMode::Auto | PairingMode::Visual, None) => ld.local.as_ref().map(|p| p.with_extension("visual")).filter(|p| p.is_file()),
            _ => None,
        };
        if let Some(vp) = &vis_path {
            let xml = std::fs::read(vp).with_context(|| format!("读取 {}", vp.display()))?;
            let xml = String::from_utf8_lossy(&xml);
            let sets = pairing::render_sets_from_visual_xml(&xml);
            if sets.is_empty() {
                warnings.push(format!("{} 中没有 <renderSet>(可能是 SDK 占位文件)", vp.display()));
            } else {
                let r = pairing::pair_by_ids(&g, &sets, PairSource::VisualXml);
                warnings.extend(r.warnings);
                pairs = r.pairs;
                source_used = format!("visual XML {}", vp.display());
            }
        }
        if mode == PairingMode::Visual && pairs.is_empty() {
            bail!("{}: --pairing visual 但没有可用的 .visual(用 --visual 指定)", ld.label);
        }
        // 2) assets.bin
        if pairs.is_empty() && want_assets {
            // 本地文件且游戏索引里找不到唯一同名: 改在 assets.bin 自身的路径表里按文件名找
            let vpath_guess = ld.vpath.clone().or_else(|| {
                let (ab, lp) = (assets.as_ref()?, ld.local.as_ref()?);
                let v = ab.find_by_file_name(&lp.file_name()?.to_string_lossy());
                if v.len() == 1 {
                    Some(v[0].clone())
                } else {
                    None
                }
            });
            if let (Some(ab), Some(vp)) = (&assets, &vpath_guess) {
                match ab.visuals_for_geometry(vp) {
                    Ok(vis) if !vis.is_empty() => {
                        let stem = stem_of(vp);
                        let chosen: Vec<&wowsmod_formats::assets_bin::VisualInfo> = if lod == "all" {
                            vis.iter().collect()
                        } else {
                            let n: u32 = lod.parse().map_err(|_| anyhow!("--lod 只能是 0..9 的数字或 all"))?;
                            let want = if n == 0 { stem.clone() } else { format!("{stem}_lod{n}") };
                            let suffix = format!("_lod{n}");
                            let mut v: Vec<_> = vis.iter().filter(|v| stem_of(&v.path).eq_ignore_ascii_case(&want)).collect();
                            if v.is_empty() && n > 0 {
                                // 如 FSB003_Richelieu_1944.geometry 只被 lods/..._Full_lod4.visual 引用
                                v = vis.iter().filter(|v| stem_of(&v.path).to_ascii_lowercase().ends_with(&suffix)).collect();
                            }
                            let with_sets: Vec<_> = v.iter().copied().filter(|x| !x.render_sets.is_empty()).collect();
                            if with_sets.is_empty() {
                                // 主 visual 没有渲染集(例如整船根 visual 只是装配节点): 用引用本 geometry 的、带渲染集的 visual
                                let alt: Vec<_> = vis.iter().filter(|x| !x.render_sets.is_empty()).collect();
                                if alt.is_empty() {
                                    warnings.push(format!("引用本 geometry 的 {} 个 visual 都没有渲染集", vis.len()));
                                    v
                                } else {
                                    warnings.push(format!(
                                        "{want}.visual 不存在或没有渲染集, 改用带渲染集的: {}",
                                        alt.iter().map(|x| stem_of(&x.path)).collect::<Vec<_>>().join(", ")
                                    ));
                                    alt
                                }
                            } else {
                                with_sets
                            }
                        };
                        let mut sets = Vec::new();
                        for v in &chosen {
                            let vs = stem_of(&v.path);
                            for (k, rs) in v.render_sets.iter().enumerate() {
                                let name = if chosen.len() > 1 { format!("{vs}/rs{k}") } else { format!("rs{k}") };
                                sets.push(RenderSetRef::from_assets(rs, name));
                            }
                        }
                        let r = pairing::pair_by_ids(&g, &sets, PairSource::AssetsBin);
                        warnings.extend(r.warnings);
                        pairs = r.pairs;
                        source_used = format!("assets.bin ({})", chosen.iter().map(|v| stem_of(&v.path)).collect::<Vec<_>>().join(", "));
                    }
                    Ok(_) => warnings.push(format!("assets.bin 中没有引用 {vp} 的 visual")),
                    Err(e) => warnings.push(format!("assets.bin 查找失败: {e}")),
                }
            } else if assets.is_some() && vpath_guess.is_none() {
                warnings.push("本地文件在游戏索引/assets.bin 中找不到唯一同名 geometry, 无法用 assets.bin 配对".into());
            }
        }
        if mode == PairingMode::Assets && pairs.is_empty() {
            bail!("{}: assets.bin 配对失败: {}", ld.label, warnings.join("; "));
        }
        // 3) 启发式 / naive
        if pairs.is_empty() {
            match mode {
                PairingMode::Naive => {
                    pairs = gexport::naive_pairs(&g);
                    source_used = "naive(数组下标, 错误示范)".into();
                    warnings.push("naive 配对仅用于对照演示, 结果几乎必然破面".into());
                }
                PairingMode::Auto | PairingMode::Heuristic => {
                    let f = gexport::max_local_index_fn(&g).map_err(|e| anyhow!("{e}"))?;
                    let r = pairing::pair_heuristic(&g, &f);
                    warnings.extend(r.warnings);
                    pairs = r.pairs;
                    source_used = "启发式(不确定)".into();
                }
                _ => {}
            }
        }
        if pairs.is_empty() {
            bail!("{}: 没有得到任何可导出的配对", ld.label);
        }
        let (prims, w2, stats) = gexport::build_prims_opts(&g, &pairs, !keep_winding).map_err(|e| anyhow!("{}: {e}", ld.label))?;
        warnings.extend(w2);
        let items: Vec<health::PrimInput> = prims
            .iter()
            .map(|p| health::PrimInput { name: p.name.clone(), positions: &p.positions, indices: &p.indices, normals: p.normals.as_deref() })
            .collect();
        let h = health::check(&items);
        let dst = if multi { out.join(format!("{}.glb", stem_of(&ld.label))) } else { out.to_path_buf() };
        if let Some(parent) = dst.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let glb_bytes = glb::write_glb(
            &prims,
            json!({"source": ld.label, "pairing": source_used, "tool": format!("wowsmod {}", env!("CARGO_PKG_VERSION"))}),
        );
        std::fs::write(&dst, &glb_bytes).with_context(|| format!("写入 {}", dst.display()))?;
        // 再从磁盘读回 GLB 独立复查一遍
        let reread = health::check_glb(&std::fs::read(&dst)?).map_err(|e| anyhow!("复查 GLB 失败: {e}"))?;
        let ok = h.ok && reread.ok && stats.bbox_mismatch == 0 && pairs.iter().all(|p| !p.ambiguous || mode == PairingMode::Naive);
        all_ok &= ok;
        results.push(json!({
            "input": ld.label, "output": dst, "glb_bytes": glb_bytes.len(), "pairing": source_used,
            "primitives": prims.len(), "triangles": h.total_triangles, "vertices": prims.iter().map(|p| p.positions.len()).sum::<usize>(),
            "oob_triangles": h.total_oob_triangles, "spike_triangles": h.total_spike_triangles,
            "long_triangles_info": h.total_long_triangles,
            "bbox_checked": stats.bbox_checked, "bbox_mismatch": stats.bbox_mismatch,
            "winding_normal_agreement": stats.winding_agrees, "winding_normal_agreement_raw": stats.winding_agrees_raw, "winding_flipped_primitives": stats.flipped_prims, "mean_normal_length": stats.mean_normal_length,
            "reread_ok": reread.ok, "ok": ok, "warnings": warnings, "health": h,
            "pairs": pairs.iter().map(|p| json!({"name": p.name, "material": p.material, "vertices_mapping": format!("{:#010x}", p.vertices.id), "indices_mapping": format!("{:#010x}", p.indices.id), "vertex_buffer": p.vertices.buffer_index, "index_buffer": p.indices.buffer_index, "source": format!("{:?}", p.source), "ambiguous": p.ambiguous})).collect::<Vec<_>>(),
        }));
    }
    let report = json!({"ok": all_ok, "results": results});
    crate::guide::stash(&report);
    if cli.json {
        emit(cli, &report);
    } else {
        for r in &results {
            println!("{} {}", if r["ok"] == json!(true) { "✔" } else { "✘" }, r["input"].as_str().unwrap());
            println!("    → {}  ({})", r["output"].as_str().unwrap_or(""), human_size(r["glb_bytes"].as_u64().unwrap_or(0)));
            println!("    配对: {}", r["pairing"].as_str().unwrap_or(""));
            println!(
                "    {} 个 primitive, {} 三角形, {} 顶点 | 越界 {} | 尖刺 {} | 包围盒校验 {}/{} 一致 | 绕序-法线一致 {:.2}% | GLB 复查 {}",
                r["primitives"], r["triangles"], r["vertices"], r["oob_triangles"], r["spike_triangles"],
                r["bbox_checked"].as_u64().unwrap_or(0) - r["bbox_mismatch"].as_u64().unwrap_or(0), r["bbox_checked"],
                r["winding_normal_agreement"].as_f64().unwrap_or(0.0) * 100.0,
                if r["reread_ok"] == json!(true) { "通过" } else { "未通过" }
            );
            for w in r["warnings"].as_array().unwrap() {
                println!("    ⚠ {}", w.as_str().unwrap_or(""));
            }
        }
    }
    Ok(if !all_ok && strict { 2 } else { 0 })
}

// ---------------------------------------------------------------- check
fn cmd_check(cli: &Cli, files: &[PathBuf]) -> Result<i32> {
    let mut all_ok = true;
    let mut out = Vec::new();
    for f in files {
        let d = std::fs::read(f).with_context(|| format!("读取 {}", f.display()))?;
        let h = health::check_glb(&d).map_err(|e| anyhow!("{}: {e}", f.display()))?;
        all_ok &= h.ok;
        if !cli.json {
            println!("== {}", f.display());
            for p in &h.primitives {
                let flag = if p.ok { String::new() } else { format!("  ✘ 越界索引 {} / 尖刺 {}", p.oob_indices, p.spike_triangles) };
                println!("  {}: 三角形 {} 顶点 {} 最大索引 {:?} 长边(参考) {}{}", p.name, p.triangles, p.vertices, p.max_index, p.long_triangles, flag);
            }
            println!("  {}", if h.ok { "✔ 健康: 0 越界, 0 尖刺" } else { "✘ 有问题" });
        }
        out.push(json!({"file": f, "health": h}));
    }
    emit(cli, &json!({"ok": all_ok, "files": out}));
    Ok(if all_ok { 0 } else { 2 })
}

// ---------------------------------------------------------------- geom
fn cmd_geom(cli: &Cli, input: &str, roundtrip: bool, limit: usize) -> Result<i32> {
    let local_input = Path::new(input).exists();
    let ctx = game_ctx(cli, !local_input)?;
    let sources = resolve_inputs(input, ctx.as_ref(), "geometry")?;
    let t = Instant::now();
    let results: Vec<Value> = sources
        .par_iter()
        .map(|src| {
            let r = (|| -> Result<Value> {
                let ld = load_source(src, ctx.as_ref())?;
                let g = Geometry::parse(&ld.data).map_err(|e| anyhow!("{e}"))?;
                let mut v = json!({
                    "input": ld.label, "bytes": ld.data.len(),
                    "vertex_buffers": g.vertex_buffers.iter().map(|b| json!({"format": b.format(), "stride": b.stride, "vertices": b.vertex_count(), "encoded": b.is_encoded(), "blob": b.blob.len()})).collect::<Vec<_>>(),
                    "index_buffers": g.index_buffers.iter().map(|b| json!({"index_size": b.index_size, "indices": b.index_count(), "encoded": b.is_encoded(), "blob": b.blob.len()})).collect::<Vec<_>>(),
                    "vertex_mappings": g.vertex_mappings.len(), "index_mappings": g.index_mappings.len(),
                    "collision": g.collision.iter().map(|s| s.name()).collect::<Vec<_>>(), "armor": g.armor.iter().map(|s| s.name()).collect::<Vec<_>>(),
                });
                if roundtrip {
                    let a = g.serialize() == ld.data;
                    let mut g2 = g.clone();
                    let mut vsame = 0;
                    let mut isame = 0;
                    for vb in g2.vertex_buffers.iter_mut() {
                        let raw = vb.decode().map_err(|e| anyhow!("{e}"))?;
                        let old = vb.blob.clone();
                        if vb.is_encoded() {
                            vb.encode_from(&raw).map_err(|e| anyhow!("{e}"))?;
                        }
                        vsame += (old == vb.blob) as usize;
                    }
                    for ib in g2.index_buffers.iter_mut() {
                        let idx = ib.decode().map_err(|e| anyhow!("{e}"))?;
                        let old = ib.blob.clone();
                        if ib.is_encoded() {
                            ib.encode_from(&idx).map_err(|e| anyhow!("{e}"))?;
                        }
                        isame += (old == ib.blob) as usize;
                    }
                    let b = g2.serialize() == ld.data;
                    v["roundtrip"] = json!({"relayout_identical": a, "reencode_identical": b, "vertex_buffers_identical": vsame, "index_buffers_identical": isame});
                }
                Ok(v)
            })();
            r.unwrap_or_else(|e| json!({"input": match src { Source::Local(p) => p.display().to_string(), Source::Game(e) => e.path.clone() }, "error": format!("{e:#}")}))
        })
        .collect();
    let errors = results.iter().filter(|r| r.get("error").is_some()).count();
    let a_ok = results.iter().filter(|r| r["roundtrip"]["relayout_identical"] == json!(true)).count();
    let b_ok = results.iter().filter(|r| r["roundtrip"]["reencode_identical"] == json!(true)).count();
    let all_ok = errors == 0 && (!roundtrip || (a_ok == results.len() && b_ok == results.len()));
    let summary = json!({"files": results.len(), "errors": errors, "relayout_identical": a_ok, "reencode_identical": b_ok, "elapsed_ms": t.elapsed().as_millis()});
    if cli.json {
        emit(cli, &json!({"ok": all_ok, "summary": summary, "results": results}));
    } else {
        for r in results.iter().take(limit) {
            if let Some(e) = r.get("error") {
                println!("✘ {}: {}", r["input"].as_str().unwrap_or(""), e);
                continue;
            }
            println!("{} ({} B)", r["input"].as_str().unwrap_or(""), r["bytes"]);
            for b in r["vertex_buffers"].as_array().unwrap() {
                println!("    顶点 {} stride {} × {}", b["format"].as_str().unwrap_or(""), b["stride"], b["vertices"]);
            }
            for b in r["index_buffers"].as_array().unwrap() {
                println!("    索引 {} 位 × {}", b["index_size"].as_u64().unwrap_or(0) * 8, b["indices"]);
            }
            let names = |v: &Value| v.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            println!("    mapping: 顶点 {} / 索引 {}; 碰撞段 [{}]; 装甲段 [{}]", r["vertex_mappings"], r["index_mappings"], names(&r["collision"]), names(&r["armor"]));
            if roundtrip {
                println!("    往返: 重排版逐字节一致={} | 解码再编码逐字节一致={}", r["roundtrip"]["relayout_identical"], r["roundtrip"]["reencode_identical"]);
            }
        }
        if results.len() > limit {
            println!("... 还有 {} 个文件未显示", results.len() - limit);
        }
        if roundtrip || results.len() > 1 {
            println!(
                "共 {} 个文件, 失败 {} | 重排版一致 {} | 解码再编码一致 {} | 用时 {:.1}s",
                results.len(), errors, a_ok, b_ok, t.elapsed().as_secs_f64()
            );
        }
        for r in results.iter().filter(|r| r.get("error").is_some()).take(20) {
            println!("  ✘ {}: {}", r["input"].as_str().unwrap_or(""), r["error"]);
        }
        for r in results.iter().filter(|r| r.get("roundtrip").is_some() && (r["roundtrip"]["relayout_identical"] != json!(true) || r["roundtrip"]["reencode_identical"] != json!(true))).take(20) {
            println!("  ✘ 往返不一致: {} {}", r["input"].as_str().unwrap_or(""), r["roundtrip"]);
        }
    }
    Ok(if all_ok { 0 } else { 1 })
}
