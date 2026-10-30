use openkf_package::*;
fn main(){ let a:Vec<String>=std::env::args().skip(1).collect(); let d=std::fs::read(&a[0]).unwrap(); let p=UnrealPackage::from_bytes(&d).unwrap(); for x in &a[1..]{ let i=i32::from_str_radix(x,16).unwrap(); println!("{x} = {:?}", p.resolve_name(i)); } }
