use std::process::Command;
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=KFIcon.ico");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let res_file = out_dir.join("app_icon.o");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let rc_file = manifest_dir.join("app.rc");

    // GCC windres supports pe-x86-64 target
    // Prefer an explicit WINDRES, then scoop's GCC, then whatever is on PATH.
    let scoop = env::var("USERPROFILE").map(|h| format!(r"{h}\scoop\apps\gcc\current\bin\windres.exe")).unwrap_or_default();
    let explicit = env::var("WINDRES").unwrap_or_default();
    let windres_candidates: Vec<String> = [explicit, scoop, "windres".to_string()].into_iter().filter(|s| !s.is_empty()).collect();

    let mut compiled = false;
    for windres in &windres_candidates {
        let status = Command::new(windres)
            .current_dir(&manifest_dir)
            .arg(&rc_file)
            .args(["-O", "coff", "--target=pe-x86-64", "-o"])
            .arg(&res_file)
            .status();

        if let Ok(s) = status {
            if s.success() {
                println!("cargo:rustc-link-arg={}", res_file.display());
                compiled = true;
                break;
            }
        }
    }

    if !compiled {
        println!("cargo:warning=Failed to compile icon resource with 64-bit windres.");
    }
}
