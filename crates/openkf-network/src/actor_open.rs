//! Payload of the first bunch of an actor channel (bOpen, ChType = Actor).
//!
//! Verified on the reference capture:
//! ```text
//! 1 bit      unknown, always 0 so far
//! Object     SerializeInt(PackageMap.max_object_index)
//!            - level object (static actor: LevelInfo, Movers, Volumes...) -> nothing else
//!            - class (dynamic actor)  -> Location (compressed vector), then
//!              rotation / initial properties (not decoded yet)
//! ```

use crate::wire::{Bits, WireResult};

/// UE2 `FVector::SerializeCompressed`: `Bits = SerializeInt(20)`, then
/// each component `SerializeInt(1 << (Bits+2)) - (1 << (Bits+1))`.
pub fn read_compressed_vector(r: &mut Bits<'_>) -> WireResult<[i32; 3]> {
    let nb = r.int(20)?;
    let bias = 1i64 << (nb + 1);
    let max = 1u64 << (nb + 2);
    let mut v = [0i32; 3];
    for c in &mut v {
        *c = (r.int(max.min(u32::MAX as u64) as u32)? as i64 - bias) as i32;
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorOpen {
    pub flag: bool,
    pub object_index: u32,
    /// Bit position right after the object index (start of class-specific data).
    pub after_object_bit: usize,
}

pub fn read_actor_open(r: &mut Bits<'_>, max_object_index: u32) -> WireResult<ActorOpen> {
    let flag = r.bit()?;
    let object_index = r.int(max_object_index)?;
    Ok(ActorOpen { flag, object_index, after_object_bit: r.pos() })
}
