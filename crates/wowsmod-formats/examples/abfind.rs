// assets.bin 路径表按文件名/子串查询, 并列出引用某 geometry 的 visual
use wowsmod_formats::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ab = assets_bin::AssetsBin::parse(std::fs::read(&a[1]).unwrap()).unwrap();
    for q in &a[2..] {
        let v = ab.find_by_file_name(q);
        println!("{q}: {} 个同名路径", v.len());
        for p in &v { println!("  {p}"); match ab.visuals_for_geometry(p) { Ok(vs) => for x in vs { println!("    visual {} ({} 渲染集)", x.path, x.render_sets.len()); }, Err(e) => println!("    err {e}") } }
    }
}
