//! `USound` exports: KF stores a complete RIFF/WAVE file after a small header
//! (FileType name, likelihood float, lazy-array skip offset, compact size).

/// The embedded `.wav` bytes of a Sound export's data, if any.
pub fn embedded_wav(data: &[u8]) -> Option<&[u8]> {
    let start = data.get(..64.min(data.len()))?.windows(4).position(|w| w == b"RIFF")?;
    let len = u32::from_le_bytes(data.get(start + 4..start + 8)?.try_into().ok()?) as usize + 8;
    data.get(start..(start + len).min(data.len()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_riff() {
        let mut d = vec![0u8, 1, 0, 0, 0x80, 0x3f, 1, 2, 3, 4, 5, 6, 7];
        d.extend_from_slice(b"RIFF");
        d.extend_from_slice(&8u32.to_le_bytes());
        d.extend_from_slice(b"WAVEdata");
        assert_eq!(super::embedded_wav(&d).unwrap().len(), 16);
    }
}
