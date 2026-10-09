fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ships = wowsmod_formats::gameparams::ships_from_raw(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("{}", serde_json::to_string(&ships).unwrap());
}
