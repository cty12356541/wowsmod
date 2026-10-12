// 把 docs/skill/*.md 与 styles/**/(README|STYLE).md 编进二进制(技能树节点)
use std::path::Path;
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    let skill = root.join("docs/skill");
    println!("cargo:rerun-if-changed={}", skill.display());
    for e in std::fs::read_dir(&skill).expect("docs/skill") {
        let p = e.unwrap().path();
        if p.extension().map(|x| x == "md").unwrap_or(false) {
            files.push(p);
        }
    }
    let styles = root.join("styles");
    println!("cargo:rerun-if-changed={}", styles.display());
    files.push(styles.join("README.md"));
    for e in std::fs::read_dir(&styles).expect("styles") {
        let p = e.unwrap().path().join("STYLE.md");
        if p.is_file() {
            println!("cargo:rerun-if-changed={}", p.display());
            files.push(p);
        }
    }
    files.sort();
    let mut s = String::from("pub const NODE_FILES: &[(&str, &str)] = &[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let c = std::fs::canonicalize(f).unwrap();
        let rel = c.strip_prefix(std::fs::canonicalize(&root).unwrap()).unwrap().to_string_lossy().replace('\\', "/");
        s += &format!("    ({rel:?}, include_str!({:?})),\n", c.to_string_lossy());
    }
    s += "];\n";
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("skill_nodes.rs"), s).unwrap();
}
