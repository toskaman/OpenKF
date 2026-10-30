//! Client-side control-channel messages, reverse-engineered from Engine.dll
//! (KF v1065) and verified against a real capture.

/// `UGameEngine::ChallengeResponse` (Engine.dll 0x104225F0).
pub fn challenge_response(c: i32) -> i32 {
    let c = c as u32;
    (((c as i32) >> 16) as u32 ^ c.wrapping_mul(0xED) ^ (c << 16) ^ 0x93FE_92CE) as i32
}

/// Packages hashed by the client for `PETE`/`REPEAT` (order observed on v1065).
pub const PETE_PACKAGES: &[&str] = &[
    "Engine", "Core", "XGame", "IpDrv", "Editor", "ROEffects", "ROEngine", "XInterface", "GUI2K4", "UWeb", "ROInterface", "xVoting",
];

/// Builds the `PETE` + `REPEAT` messages (Engine.dll ~0x104289A0).
/// `entries`: per package `GUID(32 hex, upper) + MD5(file, 32 hex, lower)`.
/// `gz`: value of `GZ=` from the server's CHALLENGE.
/// `chunk`: lengths of the REPEAT pieces (the game uses 50 - rand).
pub fn pete_messages(entries: &[String], gz: u32, mut chunk: impl FnMut() -> usize) -> Vec<String> {
    let cols = entries.iter().map(|e| e.chars().count()).max().unwrap_or(0).min(64);
    let mut key = gz;
    let mut pieces = Vec::new();
    let mut cur = String::new();
    let mut want = chunk().max(1);
    for c in 0..cols {
        for e in entries {
            let ch = e.chars().nth(c).unwrap_or('\0') as u32;
            let v = (ch + (key & 0x7f)) & 0xffff;
            key = key.wrapping_add(1);
            cur.push(char::from_u32(v).unwrap_or('?'));
            if cur.chars().count() >= want {
                pieces.push(std::mem::take(&mut cur));
                want = chunk().max(1);
            }
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    let mut out = vec![format!("PETE PKT={} PKG={}", pieces.len(), entries.len())];
    out.extend(pieces.into_iter().map(|p| format!("REPEAT {p}")));
    out
}

/// Inverse of [`pete_messages`] (for checking captures).
pub fn decode_repeat(pieces: &[&str], packages: usize, gz: u32) -> Vec<String> {
    let s: Vec<char> = pieces.concat().chars().collect();
    let mut key = gz;
    let plain: Vec<char> = s
        .iter()
        .map(|&c| {
            let v = (c as u32).wrapping_sub(key & 0x7f) & 0xffff;
            key = key.wrapping_add(1);
            char::from_u32(v).unwrap_or('?')
        })
        .collect();
    let cols = plain.len() / packages.max(1);
    (0..packages).map(|j| (0..cols).map(|c| plain[c * packages + j]).collect()).collect()
}

/// `GM=` blob (Engine.dll 0x10486BC5): 64 random bytes in which the 16 bytes
/// of `gmd5` are nibble-mixed from offset `k2`, xored with `k1 + i`; then k1, k2.
pub fn make_gm(gmd5: &[u8; 16], mut rand: impl FnMut() -> u8) -> String {
    let mut b = [0u8; 66];
    for x in b.iter_mut().take(64) {
        *x = rand();
    }
    let k1 = rand();
    let k2 = rand() % 32;
    let mut c = k2 as usize;
    for (i, &m) in gmd5.iter().enumerate() {
        let a = m ^ k1.wrapping_add(i as u8);
        b[c] = (a & 0xf0) | (b[c] & 0x0f);
        b[c + 1] = (b[c + 1] & 0xf0) | (a & 0x0f);
        c += 2;
    }
    b[64] = k1;
    b[65] = k2;
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn decode_gm(hex: &str) -> Option<[u8; 16]> {
    let b: Vec<u8> = (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)).collect::<Result<_, _>>().ok()?;
    if b.len() != 66 {
        return None;
    }
    let (k1, mut c) = (b[64], b[65] as usize);
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        let a = (b.get(c)? & 0xf0) | (b.get(c + 1)? & 0x0f);
        *o = a ^ k1.wrapping_add(i as u8);
        c += 2;
    }
    Some(out)
}

/// Value of `KEY=` in a control message.
pub fn param<'a>(msg: &'a str, key: &str) -> Option<&'a str> {
    msg.split(' ').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_matches_capture() {
        assert_eq!(challenge_response(-705_965_475), -1_257_404_612);
    }
    #[test]
    fn gm_roundtrip() {
        let md: [u8; 16] = core::array::from_fn(|i| (i * 17) as u8);
        let mut s = 12345u32;
        let gm = make_gm(&md, || {
            s = s.wrapping_mul(1103515245).wrapping_add(12345);
            (s >> 16) as u8
        });
        assert_eq!(decode_gm(&gm), Some(md));
    }
    #[test]
    fn pete_roundtrip() {
        let e = vec!["A".repeat(64), "b".repeat(64), "0123456789abcdef".repeat(4)];
        let m = pete_messages(&e, 39, || 50);
        let pieces: Vec<&str> = m[1..].iter().map(|s| s.strip_prefix("REPEAT ").unwrap()).collect();
        assert_eq!(decode_repeat(&pieces, 3, 39), e);
        assert!(m[0].starts_with("PETE PKT="));
    }
}
