//! Lists exports (class, name, size, offset) of a package. Optional class filter.
use openkf_package::*;
fn main() {
    let mut a = std::env::args().skip(1);
    let path = a.next().unwrap();
    let filter = a.next();
    let data = std::fs::read(&path).unwrap();
    let pkg = UnrealPackage::from_bytes(&data).unwrap();
    for (i, e) in pkg.exports.entries.iter().enumerate() {
        let cls = pkg.format_full_name(e.class_index);
        if filter.as_deref().map_or(true, |f| cls.ends_with(f)) {
            println!("{i}\t{cls}\t{}\t{}\t{}\t{:08x}", pkg.format_full_name(i as i32 + 1), e.serial_size, e.serial_offset, e.object_flags);
        }
    }
}
