//! Verified Unreal Engine 2 (build 3339 / KF v1065) datagram framing.
//!
//! Every layout here was checked against a real client/server capture
//! (`captures/capture_1792954639.txt`, 7330 datagrams, 0 decode errors).
//!
//! Datagram (bits are LSB-first inside each byte):
//! ```text
//! PacketId            SerializeInt(MAX_PACKET_ID)
//! repeat until end:
//!   bIsAck            1 bit
//!   if ack:           AckPacketId SerializeInt(MAX_PACKET_ID)
//!   else bunch:
//!     bControl        1 bit
//!     if bControl:    bOpen 1 bit, bClose 1 bit
//!     bReliable       1 bit
//!     ChIndex         SerializeInt(MAX_CHANNELS)
//!     if bReliable:   ChSequence SerializeInt(MAX_CH_SEQUENCE)
//!     if bReliable || bOpen: ChType SerializeInt(CHTYPE_MAX)
//!     DataBits        SerializeInt(MAX_PACKET_BITS)
//!     Data            DataBits bits
//! terminator          a single 1 bit; the highest set bit of the last byte
//! ```

use std::fmt;

pub const MAX_PACKET_ID: u32 = 16384;
pub const MAX_CHANNELS: u32 = 1023;
pub const MAX_CH_SEQUENCE: u32 = 1024;
pub const CHTYPE_MAX: u32 = 8;
pub const MAX_PACKET_BITS: u32 = 4096;

pub const CHTYPE_CONTROL: u8 = 1;
pub const CHTYPE_ACTOR: u8 = 2;
pub const CHTYPE_FILE: u8 = 3;
pub const CHTYPE_VOICE: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Empty,
    NoTerminator,
    Overflow { at: usize, len: usize },
    BunchTooLarge(usize),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WireError {}

pub type WireResult<T> = Result<T, WireError>;

/// LSB-first bit reader with UE2 `SerializeInt` semantics.
#[derive(Debug, Clone)]
pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    len: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8], len_bits: usize) -> Self {
        Self { data, pos: 0, len: len_bits.min(data.len() * 8) }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }
    pub fn remaining(&self) -> usize {
        self.len - self.pos
    }
    pub fn at_end(&self) -> bool {
        self.pos >= self.len
    }
    pub fn bit(&mut self) -> WireResult<bool> {
        if self.pos >= self.len {
            return Err(WireError::Overflow { at: self.pos, len: self.len });
        }
        let v = (self.data[self.pos >> 3] >> (self.pos & 7)) & 1;
        self.pos += 1;
        Ok(v != 0)
    }
    /// `FBitReader::SerializeInt(Value, ValueMax)`.
    pub fn int(&mut self, max: u32) -> WireResult<u32> {
        let (mut v, mut m) = (0u32, 1u32);
        while (v as u64 + m as u64) < max as u64 && m != 0 {
            if self.bit()? {
                v |= m;
            }
            m = m.wrapping_shl(1);
        }
        Ok(v)
    }
    pub fn bits(&mut self, n: usize) -> WireResult<Vec<u8>> {
        if self.pos + n > self.len {
            return Err(WireError::Overflow { at: self.pos + n, len: self.len });
        }
        let mut out = vec![0u8; n.div_ceil(8)];
        for i in 0..n {
            if self.bit()? {
                out[i >> 3] |= 1 << (i & 7);
            }
        }
        Ok(out)
    }
    pub fn byte(&mut self) -> WireResult<u8> {
        Ok(self.bits(8)?[0])
    }
    pub fn u32_le(&mut self) -> WireResult<u32> {
        let b = self.bits(32)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn f32_le(&mut self) -> WireResult<f32> {
        Ok(f32::from_bits(self.u32_le()?))
    }
    /// UE2 `FCompactIndex` (sign + 6 bits, then 7-bit groups).
    pub fn compact_index(&mut self) -> WireResult<i32> {
        let b = self.byte()?;
        let neg = b & 0x80 != 0;
        let mut v = (b & 0x3f) as i64;
        let mut more = b & 0x40 != 0;
        let mut shift = 6;
        while more && shift < 34 {
            let b = self.byte()?;
            v |= ((b & 0x7f) as i64) << shift;
            shift += 7;
            more = b & 0x80 != 0;
        }
        Ok(if neg { -v as i32 } else { v as i32 })
    }
    /// UE2 `FString`: compact-index length (incl. NUL); negative = UTF-16.
    pub fn fstring(&mut self) -> WireResult<String> {
        let n = self.compact_index()?;
        if n == 0 {
            return Ok(String::new());
        }
        // Cap FString length to prevent OOM on malicious server
        if n > 4096 || n < -4096 {
            return Err(WireError::Overflow { at: self.pos, len: self.len });
        }
        if n > 0 {
            let mut s = String::with_capacity(n as usize);
            for i in 0..n {
                let c = self.byte()?;
                if i + 1 < n {
                    s.push(c as char);
                }
            }
            Ok(s)
        } else {
            let n = (-n) as usize;
            let mut u = Vec::with_capacity(n);
            for _ in 0..n {
                let b = self.bits(16)?;
                u.push(u16::from_le_bytes([b[0], b[1]]));
            }
            u.pop();
            Ok(String::from_utf16_lossy(&u))
        }
    }
}

/// LSB-first bit writer, mirror of [`Bits`].
#[derive(Debug, Default, Clone)]
pub struct BitsOut {
    data: Vec<u8>,
    len: usize,
}

impl BitsOut {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len_bits(&self) -> usize {
        self.len
    }
    pub fn bit(&mut self, v: bool) {
        if self.len & 7 == 0 {
            self.data.push(0);
        }
        if v {
            *self.data.last_mut().unwrap() |= 1 << (self.len & 7);
        }
        self.len += 1;
    }
    pub fn int(&mut self, value: u32, max: u32) {
        let (mut v, mut m) = (0u32, 1u32);
        while (v as u64 + m as u64) < max as u64 && m != 0 {
            let b = value & m != 0;
            if b {
                v |= m;
            }
            self.bit(b);
            m = m.wrapping_shl(1);
        }
    }
    pub fn bits(&mut self, data: &[u8], n: usize) {
        for i in 0..n {
            self.bit((data[i >> 3] >> (i & 7)) & 1 != 0);
        }
    }
    pub fn byte(&mut self, b: u8) {
        self.bits(&[b], 8);
    }
    pub fn compact_index(&mut self, v: i32) {
        let neg = v < 0;
        let mut a = v.unsigned_abs();
        let mut b0 = (a & 0x3f) as u8;
        if neg {
            b0 |= 0x80;
        }
        a >>= 6;
        if a != 0 {
            b0 |= 0x40;
        }
        self.byte(b0);
        while a != 0 {
            let mut b = (a & 0x7f) as u8;
            a >>= 7;
            if a != 0 {
                b |= 0x80;
            }
            self.byte(b);
        }
    }
    /// FString: ANSI (latin-1, 1 byte/char) when every char < 256, else UTF-16
    /// with a negative length — same rule as UE2's `operator<<(FArchive&, FString&)`.
    pub fn fstring(&mut self, s: &str) {
        let chars: Vec<u32> = s.chars().map(|c| c as u32).collect();
        if chars.iter().all(|&c| c < 256) {
            self.compact_index(chars.len() as i32 + 1);
            for c in chars {
                self.byte(c as u8);
            }
            self.byte(0);
        } else {
            let u: Vec<u16> = s.encode_utf16().collect();
            self.compact_index(-(u.len() as i32 + 1));
            for c in u.into_iter().chain([0]) {
                self.bits(&c.to_le_bytes(), 16);
            }
        }
    }
    pub fn into_parts(self) -> (Vec<u8>, usize) {
        (self.data, self.len)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bunch {
    pub control: bool,
    pub open: bool,
    pub close: bool,
    pub reliable: bool,
    pub ch_index: u32,
    pub ch_sequence: Option<u32>,
    pub ch_type: Option<u8>,
    pub data_bits: usize,
    pub data: Vec<u8>,
}

impl Bunch {
    pub fn reader(&self) -> Bits<'_> {
        Bits::new(&self.data, self.data_bits)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Ack(u32),
    Bunch(Bunch),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub id: u32,
    pub items: Vec<Item>,
}

impl Packet {
    pub fn decode(buf: &[u8]) -> WireResult<Self> {
        let last = *buf.last().ok_or(WireError::Empty)?;
        if last == 0 {
            return Err(WireError::NoTerminator);
        }
        let len = (buf.len() - 1) * 8 + (7 - last.leading_zeros() as usize);
        let mut r = Bits::new(buf, len);
        let id = r.int(MAX_PACKET_ID)?;
        let mut items = Vec::new();
        while !r.at_end() {
            if r.bit()? {
                items.push(Item::Ack(r.int(MAX_PACKET_ID)?));
                continue;
            }
            let control = r.bit()?;
            let (open, close) = if control { (r.bit()?, r.bit()?) } else { (false, false) };
            let reliable = r.bit()?;
            let ch_index = r.int(MAX_CHANNELS)?;
            let ch_sequence = if reliable { Some(r.int(MAX_CH_SEQUENCE)?) } else { None };
            let ch_type = if reliable || open { Some(r.int(CHTYPE_MAX)? as u8) } else { None };
            let data_bits = r.int(MAX_PACKET_BITS)? as usize;
            let data = r.bits(data_bits)?;
            items.push(Item::Bunch(Bunch {
                control,
                open,
                close,
                reliable,
                ch_index,
                ch_sequence,
                ch_type,
                data_bits,
                data,
            }));
        }
        Ok(Packet { id, items })
    }

    pub fn encode(&self) -> WireResult<Vec<u8>> {
        let mut w = BitsOut::new();
        w.int(self.id, MAX_PACKET_ID);
        for it in &self.items {
            match it {
                Item::Ack(a) => {
                    w.bit(true);
                    w.int(*a, MAX_PACKET_ID);
                }
                Item::Bunch(b) => {
                    if b.data_bits >= MAX_PACKET_BITS as usize {
                        return Err(WireError::BunchTooLarge(b.data_bits));
                    }
                    w.bit(false);
                    w.bit(b.control);
                    if b.control {
                        w.bit(b.open);
                        w.bit(b.close);
                    }
                    w.bit(b.reliable);
                    w.int(b.ch_index, MAX_CHANNELS);
                    if b.reliable {
                        w.int(b.ch_sequence.unwrap_or(0), MAX_CH_SEQUENCE);
                    }
                    if b.reliable || b.open {
                        w.int(b.ch_type.unwrap_or(0) as u32, CHTYPE_MAX);
                    }
                    w.int(b.data_bits as u32, MAX_PACKET_BITS);
                    w.bits(&b.data, b.data_bits);
                }
            }
        }
        w.bit(true); // terminator
        Ok(w.into_parts().0)
    }
}

/// Splits a control-channel bunch into its FString messages.
pub fn control_messages(b: &Bunch) -> WireResult<Vec<String>> {
    let mut r = b.reader();
    let mut out = Vec::new();
    while r.remaining() >= 8 {
        out.push(r.fstring()?);
    }
    Ok(out)
}

/// One line of an `openkf-netcap` capture file.
#[derive(Debug, Clone)]
pub struct CaptureRecord {
    pub ms: u64,
    pub from_client: bool,
    pub bytes: Vec<u8>,
}

pub fn parse_capture(text: &str) -> Vec<CaptureRecord> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let ms = it.next()?.parse().ok()?;
            let from_client = it.next()? == "C>S";
            let _n = it.next()?;
            let h = it.next()?;
            let bytes = (0..h.len() / 2)
                .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16))
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some(CaptureRecord { ms, from_client, bytes })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn empty_and_ack_packets() {
        assert_eq!(Packet::decode(&hex("0140")).unwrap(), Packet { id: 1, items: vec![] });
        let p = Packet::decode(&hex("01400020")).unwrap();
        assert_eq!(p.items, vec![Item::Ack(0)]);
        assert_eq!(p.encode().unwrap(), hex("01400020"));
    }

    #[test]
    fn real_hello() {
        let raw = hex("008005208040085000521113d31388549155d254d293530f0c48539293559154cf4c0c0e0c88559154cfcccc4c0ec814555150531251cf8d4d8d4d4c4c0e0e4c4c4c8ccd8c0c4e0d40");
        let p = Packet::decode(&raw).unwrap();
        let Item::Bunch(b) = &p.items[0] else { panic!() };
        assert!(b.open && b.reliable);
        assert_eq!((b.ch_index, b.ch_type, b.ch_sequence), (0, Some(CHTYPE_CONTROL), Some(1)));
        let m = control_messages(b).unwrap();
        assert!(m[0].starts_with("HELLO REVISION=0 MINVER=3180 VER=3339 STEAMID="), "{m:?}");
        assert_eq!(p.encode().unwrap(), raw);
    }

    #[test]
    fn fstring_latin1_roundtrip() {
        for txt in ["REPEAT \u{b1}\u{b3}\u{e9}xyz", "plain", "\u{263a} unicode"] {
            let mut w = BitsOut::new();
            w.fstring(txt);
            let (d, n) = w.into_parts();
            if txt.chars().all(|c| (c as u32) < 256) {
                assert_eq!(n, (txt.chars().count() + 2) * 8);
            }
            assert_eq!(Bits::new(&d, n).fstring().unwrap(), txt);
        }
    }

    #[test]
    fn compact_index_roundtrip() {
        for v in [0, 1, 63, 64, 300, -5, 100_000, -100_000] {
            let mut w = BitsOut::new();
            w.compact_index(v);
            let (d, n) = w.into_parts();
            assert_eq!(Bits::new(&d, n).compact_index().unwrap(), v);
        }
    }
}
