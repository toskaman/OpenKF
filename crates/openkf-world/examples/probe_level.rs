fn main() {
    for p in std::env::args().skip(1) {
        let mut world = openkf_world::World::new();
        let t = std::time::Instant::now();
        match openkf_world::level_loader::LevelLoader::load_level_from_rom(&p, &mut world) {
            Ok(s) => println!("{p}: {:.0}ms {:#?}", t.elapsed().as_secs_f64()*1000.0, s),
            Err(e) => println!("{p}: ERROR {e}"),
        }
    }
}
