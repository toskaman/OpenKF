use openkf_package::MmapPackage;
fn main(){
  let kf=std::path::Path::new("/sessions");
  let _=kf;
  for f in std::env::args().skip(1){
    let p=MmapPackage::open(&f).unwrap(); let p=p.as_package();
    println!("{f} exports={} gens={:?}", p.exports.entries.len(), p.summary.generations.iter().map(|g|g.export_count).collect::<Vec<_>>());
  }
}
