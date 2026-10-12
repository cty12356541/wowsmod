fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let ships = wowsmod_formats::gameparams::ships_from_raw(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("{} ships in {:?}", ships.len(), t.elapsed());
    for s in ships.iter().filter(|s| a.len() < 3 || s.name.contains(&a[2])) {
        println!("{} {} L{} {} {} {} hull={:?} other={}", s.index, s.name, s.level, s.nation, s.species, s.group, s.hull_models, s.other_models.len());
    }
}
