use openkf_package::*;
fn main(){ for f in std::env::args().skip(1){ let m=MmapPackage::open(&f).unwrap(); let p=m.as_package();
 for i in 0..p.exports.len(){ if export_class_name(&p,i)=="SkeletalMesh"{ println!("{} {}", f.rsplit('/').next().unwrap(), p.resolve_name(p.exports.get(i).unwrap().object_name).unwrap()); } } } }
