use crate::error::PackageError;

/// Deserializes a variable-length `FCompactIndex` from raw byte data.
///
/// Matches the exact bitwise algorithm from Killing Floor Core.dll (0x10147af0):
/// - First byte (`b0`):
///   - Bit 7 (0x80): Sign bit (1 = negative).
///   - Bit 6 (0x40): Continuation bit (1 = more bytes follow).
///   - Bits 0..5 (0x3F): Lowest 6 bits of data.
/// - Subsequent bytes (`b1..b4`):
///   - Bit 7 (0x80): Continuation bit.
///   - Bits 0..6 (0x7F): 7 bits of data shifted left by 6, 13, 20, 27.
#[inline]
pub fn read_compact_index(data: &[u8], offset: &mut usize) -> Result<i32, PackageError> {
    let start_offset = *offset;
    if *offset >= data.len() {
        return Err(PackageError::UnexpectedEof {
            offset: *offset,
            requested: 1,
            total: data.len(),
        });
    }

    let b0 = data[*offset];
    *offset += 1;

    let sign = (b0 & 0x80) != 0;
    let mut more = (b0 & 0x40) != 0;
    let mut val = (b0 & 0x3F) as i32;

    if more {
        let mut shift = 6;
        while more {
            if *offset >= data.len() {
                return Err(PackageError::UnexpectedEof {
                    offset: *offset,
                    requested: 1,
                    total: data.len(),
                });
            }

            let b = data[*offset];
            *offset += 1;
            more = (b & 0x80) != 0;
            if shift > 27 {
                // 5 bytes max (6 + 7*4 = 34 bits); beyond that the stream is corrupt.
                // Return error instead of panicking on shift overflow (>=32).
                return Err(PackageError::InvalidCompactIndex(start_offset));
            }
            let add = ((b & 0x7F) as u32).checked_shl(shift as u32)
                .ok_or(PackageError::InvalidCompactIndex(start_offset))?;
            val |= add as i32;
            shift += 7;
        }
    }

    Ok(if sign { -val } else { val })
}

/// Serializes an `i32` into `FCompactIndex` byte representation.
///
/// Writes 1 to 5 bytes according to the magnitude and sign of `val`.
#[inline]
pub fn write_compact_index(val: i32, out: &mut Vec<u8>) {
    let sign = val < 0;
    let mut uval = if sign {
        (-val) as u32
    } else {
        val as u32
    };

    let mut b0 = (uval & 0x3F) as u8;
    if sign {
        b0 |= 0x80;
    }

    uval >>= 6;
    if uval > 0 {
        b0 |= 0x40; // continuation bit for byte 0
        out.push(b0);
        while uval > 0 {
            let mut b = (uval & 0x7F) as u8;
            uval >>= 7;
            if uval > 0 {
                b |= 0x80; // continuation bit for subsequent bytes
            }
            out.push(b);
        }
    } else {
        out.push(b0);
    }
}

/// Returns the byte length needed to encode `val` as an `FCompactIndex`.
#[inline]
pub fn encoded_compact_index_len(val: i32) -> usize {
    let uval = if val < 0 { (-val) as u32 } else { val as u32 };
    if uval < (1 << 6) {
        1
    } else if uval < (1 << (6 + 7)) {
        2
    } else if uval < (1 << (6 + 14)) {
        3
    } else if uval < (1 << (6 + 21)) {
        4
    } else {
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_index_roundtrip() {
        let test_cases = vec![
            0,
            1,
            -1,
            42,
            -42,
            63,
            -63,
            64,
            -64,
            127,
            -127,
            128,
            -128,
            255,
            -255,
            8191,
            -8191,
            8192,
            -8192,
            16383,
            -16383,
            65535,
            -65535,
            1_000_000,
            -1_000_000,
            i32::MAX,
            -i32::MAX,
        ];

        for val in test_cases {
            let mut encoded = Vec::new();
            write_compact_index(val, &mut encoded);
            assert_eq!(encoded.len(), encoded_compact_index_len(val), "Length mismatch for {}", val);

            let mut offset = 0;
            let decoded = read_compact_index(&encoded, &mut offset).expect("Failed to decode");
            assert_eq!(offset, encoded.len());
            assert_eq!(decoded, val, "Decoded value mismatch for {}", val);
        }
    }

    #[test]
    fn test_compact_index_zero() {
        let mut buf = Vec::new();
        write_compact_index(0, &mut buf);
        assert_eq!(buf, vec![0x00]);

        let mut offset = 0;
        let val = read_compact_index(&buf, &mut offset).unwrap();
        assert_eq!(val, 0);
        assert_eq!(offset, 1);
    }

    #[test]
    fn test_compact_index_small_positive() {
        let mut buf = Vec::new();
        write_compact_index(1, &mut buf);
        assert_eq!(buf, vec![0x01]);

        let mut offset = 0;
        assert_eq!(read_compact_index(&buf, &mut offset).unwrap(), 1);
    }

    #[test]
    fn test_compact_index_small_negative() {
        let mut buf = Vec::new();
        write_compact_index(-1, &mut buf);
        // val = 1 (0x01) | sign (0x80) = 0x81
        assert_eq!(buf, vec![0x81]);

        let mut offset = 0;
        assert_eq!(read_compact_index(&buf, &mut offset).unwrap(), -1);
    }

    #[test]
    fn test_compact_index_boundary_64() {
        let mut buf = Vec::new();
        write_compact_index(64, &mut buf);
        // 64: b0 = (64 & 0x3F) | 0x40 = 0x40. uval >>= 6 -> 1. b1 = 1.
        assert_eq!(buf, vec![0x40, 0x01]);

        let mut offset = 0;
        assert_eq!(read_compact_index(&buf, &mut offset).unwrap(), 64);
    }
}
