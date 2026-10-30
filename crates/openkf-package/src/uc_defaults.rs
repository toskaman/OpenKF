//! `defaultproperties` of UnrealScript classes, read from the `.uc` sources
//! shipped with the game (`<KF>/<Package>/Classes/*.uc`).
//!
//! Used where compiled class defaults would otherwise be needed (mesh of a
//! class, `bNetInitialRotation`, ...). Values are kept as raw text.

use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct UcClass {
    pub package: String,
    pub parent: Option<String>,
    /// Lower-cased key (`Mesh`, `MovementAnims(0)`) -> raw value text.
    pub defaults: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct ScriptDefaults {
    /// Lower-cased class name -> class.
    pub classes: HashMap<String, UcClass>,
}

fn parse_uc(text: &str, package: &str) -> Option<(String, UcClass)> {
    // Header: "class Foo extends Bar ..." (may span lines, may lack extends).
    let lower = text.to_ascii_lowercase();
    // First line that starts with "class " (comments may mention the word earlier).
    let mut off = 0usize;
    let mut start = None;
    for line in lower.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("class ") {
            start = Some(off + (line.len() - t.len()));
            break;
        }
        off += line.len();
    }
    let start = start?;
    let header: String = text[start..].split(';').next()?.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut it = header.split(' ');
    it.next();
    let name = it.next()?.to_string();
    let parent = {
        let words: Vec<&str> = header.split(' ').collect();
        words.iter().position(|w| w.eq_ignore_ascii_case("extends")).and_then(|i| words.get(i + 1)).map(|p| p.rsplit('.').next().unwrap_or(p).to_string())
    };
    let mut defaults = HashMap::new();
    if let Some(dp) = lower.rfind("defaultproperties") {
        let body = &text[dp..];
        if let (Some(a), Some(b)) = (body.find('{'), body.rfind('}')) {
            for line in body[a + 1..b].lines() {
                let l = line.trim();
                if l.starts_with("//") || l.starts_with("Begin ") || l.starts_with("End ") {
                    continue;
                }
                if let Some((k, v)) = l.split_once('=') {
                    let k = k.trim().replace('[', "(").replace(']', ")").to_ascii_lowercase();
                    let v = v.split("//").next().unwrap_or("").trim().trim_end_matches(';').trim().to_string();
                    if !k.is_empty() && !k.contains(' ') {
                        defaults.insert(k, v);
                    }
                }
            }
        }
    }
    Some((name, UcClass { package: package.to_string(), parent, defaults }))
}

impl ScriptDefaults {
    /// Scans `<kf_dir>/*/Classes/*.uc`.
    pub fn load(kf_dir: &Path) -> Self {
        let mut s = ScriptDefaults::default();
        let Ok(dirs) = std::fs::read_dir(kf_dir) else { return s };
        for d in dirs.flatten() {
            let classes = d.path().join("Classes");
            let Ok(files) = std::fs::read_dir(&classes) else { continue };
            let package = d.file_name().to_string_lossy().to_string();
            for f in files.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("uc")) != Some(true) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let text = String::from_utf8_lossy(&bytes);
                if let Some((name, c)) = parse_uc(&text, &package) {
                    s.classes.entry(name.to_ascii_lowercase()).or_insert(c);
                }
            }
        }
        s
    }

    /// Default value of `key` for `class`, walking parents.
    pub fn get(&self, class: &str, key: &str) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        let mut cur = class.to_ascii_lowercase();
        for _ in 0..32 {
            let c = self.classes.get(&cur)?;
            if let Some(v) = c.defaults.get(&key) {
                return Some(v.as_str());
            }
            cur = c.parent.as_ref()?.to_ascii_lowercase();
        }
        None
    }

    pub fn get_bool(&self, class: &str, key: &str) -> Option<bool> {
        self.get(class, key).map(|v| v.eq_ignore_ascii_case("true"))
    }

    /// `Mesh=SkeletalMesh'Pkg.Name'` -> ("SkeletalMesh", "Pkg", "Name").
    pub fn object_ref(&self, class: &str, key: &str) -> Option<(String, String, String)> {
        let v = self.get(class, key)?;
        let (kind, rest) = v.split_once('\'')?;
        let path = rest.trim_end_matches('\'');
        let (pkg, name) = path.split_once('.')?;
        Some((kind.to_string(), pkg.to_string(), name.rsplit('.').next().unwrap_or(name).to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_basic() {
        let t = "//x\nclass SingleAttachment extends KFWeaponAttachment;\nvar int A;\ndefaultproperties\n{\n    Mesh=SkeletalMesh'KF_Weapons3rd_Trip.Single9mm_3rd'\n    MovementAnims(0)=JogF_Single9mm\n    bNetInitialRotation=true\n}\n";
        let (n, c) = parse_uc(t, "KFMod").unwrap();
        assert_eq!(n, "SingleAttachment");
        assert_eq!(c.parent.as_deref(), Some("KFWeaponAttachment"));
        let mut s = ScriptDefaults::default();
        s.classes.insert(n.to_ascii_lowercase(), c);
        assert_eq!(s.object_ref("SingleAttachment", "Mesh"), Some(("SkeletalMesh".into(), "KF_Weapons3rd_Trip".into(), "Single9mm_3rd".into())));
        assert_eq!(s.get_bool("singleattachment", "bNetInitialRotation"), Some(true));
        assert_eq!(s.get("SingleAttachment", "movementanims(0)"), Some("JogF_Single9mm"));
    }
}
