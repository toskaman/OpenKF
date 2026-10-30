//! Decoding of replicated properties and RPCs inside actor-channel bunches.
//! Formats are hypotheses validated by "bunch consumed exactly" statistics on
//! real captures (see `examples/replay.rs`).

use crate::actor_open::read_compressed_vector;
use crate::schema::{ClassNet, GameSchema, NetType};
use crate::wire::{Bits, WireError, WireResult, MAX_CHANNELS};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    Bool(bool),
    Byte(u32),
    Name(u32),
    Object(u32),
    /// Reference to a dynamic actor by channel index.
    ActorChannel(u32),
    Str(String),
    Vector([i32; 3]),
    Rotator([i32; 3]),
    Struct(Vec<(String, Value)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Property { name: String, element: u32, value: Value },
    Rpc { name: String, params: Vec<(String, Value)> },
}

/// UE2 `FRotator` net form: for each of Pitch/Yaw/Roll, 1 bit + optional byte (<<8).
pub fn read_rotator(r: &mut Bits<'_>) -> WireResult<[i32; 3]> {
    let mut v = [0; 3];
    for c in &mut v {
        if r.bit()? {
            *c = (r.byte()? as i32) << 8;
        }
    }
    Ok(v)
}

/// `UPackageMapLevel::SerializeObject` (Engine.dll 0x1046AE50): always 1 bit;
/// 1 -> dynamic actor by channel (SerializeInt 1023), 0 -> package object index.
pub fn read_object(r: &mut Bits<'_>, s: &GameSchema, _actor: bool) -> WireResult<Value> {
    if r.bit()? {
        return Ok(Value::ActorChannel(r.int(MAX_CHANNELS)?));
    }
    Ok(Value::Object(r.int(s.map.max_object_index)?))
}

pub fn read_value(r: &mut Bits<'_>, t: &NetType, s: &GameSchema) -> WireResult<Value> {
    Ok(match t {
        NetType::Int => Value::Int(r.u32_le()? as i32),
        NetType::Float => Value::Float(r.f32_le()?),
        NetType::Bool => Value::Bool(r.bit()?),
        NetType::Byte { bits } => {
            let b = r.bits(*bits as usize)?;
            Value::Byte(b.first().copied().unwrap_or(0) as u32)
        }
        NetType::Name => Value::Name(r.int(s.names.len() as u32 + 1)?),
        NetType::Object { actor } => read_object(r, s, *actor)?,
        NetType::Str => Value::Str(r.fstring()?),
        NetType::Vector => Value::Vector(read_compressed_vector(r)?),
        NetType::Rotator => Value::Rotator(read_rotator(r)?),
        NetType::CompressedPosition => {
            let loc = read_compressed_vector(r)?;
            let pitch = (r.byte()? as i32) << 8;
            let yaw = (r.byte()? as i32) << 8;
            let vel = read_compressed_vector(r)?;
            Value::Struct(vec![
                ("Location".into(), Value::Vector(loc)),
                ("Rotation".into(), Value::Rotator([pitch, yaw, 0])),
                ("Velocity".into(), Value::Vector(vel)),
            ])
        }
        NetType::Plane => {
            let mut v = Vec::new();
            for n in ["X", "Y", "Z", "W"] {
                let b = r.bits(16)?;
                v.push((n.to_string(), Value::Int(i16::from_le_bytes([b[0], b[1]]) as i32)));
            }
            Value::Struct(v)
        }
        NetType::Struct { members, .. } => {
            let mut out = Vec::new();
            for (n, dim, mt) in members {
                for _ in 0..*dim {
                    out.push((n.clone(), read_value(r, mt, s)?));
                }
            }
            Value::Struct(out)
        }
        NetType::Unknown(_) => return Err(WireError::Overflow { at: r.pos(), len: 0 }),
    })
}

/// Reads `RepIndex` + payload until the bunch is exhausted.
pub fn read_updates(r: &mut Bits<'_>, c: &ClassNet, s: &GameSchema) -> Result<Vec<Update>, (Vec<Update>, String)> {
    let mut out = Vec::new();
    let max = c.fields.len() as u32;
    while !r.at_end() {
        let start = r.pos();
        let idx = match r.int(max) {
            Ok(i) => i,
            Err(e) => return Err((out, format!("repindex @{start}: {e}"))),
        };
        let Some(f) = c.fields.get(idx as usize) else {
            return Err((out, format!("repindex {idx} >= {max} @{start}")));
        };
        let res: WireResult<Update> = (|| {
            if let Some(t) = &f.ty {
                let element = if f.array_dim > 1 { r.byte()? as u32 } else { 0 };
                Ok(Update::Property { name: f.name.clone(), element, value: read_value(r, t, s)? })
            } else {
                let mut params = Vec::new();
                for (n, flags, t) in &f.params {
                    if matches!(t, NetType::Bool) {
                        if r.at_end() && (flags & crate::schema::CPF_OPTIONAL_PARM != 0) {
                            break;
                        }
                        params.push((n.clone(), Value::Bool(r.bit()?)));
                    } else {
                        if r.at_end() && (flags & crate::schema::CPF_OPTIONAL_PARM != 0) {
                            break;
                        }
                        // Non-bool: presence bit + value; if bunch ends here, trailing optional params are omitted.
                        let present = match r.bit() {
                            Ok(b) => b,
                            Err(_) if r.at_end() && (flags & crate::schema::CPF_OPTIONAL_PARM != 0) => break,
                            Err(e) => return Err(e),
                        };
                        if present {
                            params.push((n.clone(), read_value(r, t, s)?));
                        }
                    }
                }
                Ok(Update::Rpc { name: f.name.clone(), params })
            }
        })();
        match res {
            Ok(u) => out.push(u),
            Err(e) => return Err((out, format!("{}.{} ({:?}) @{start}: {e}", f.owner, f.name, f.ty))),
        }
    }
    Ok(out)
}

/// Encodes one value (inverse of [`read_value`]) for the simple types used by RPCs.
pub fn write_value(w: &mut crate::wire::BitsOut, t: &NetType, v: &Value, s: &GameSchema) -> Result<(), String> {
    match (t, v) {
        (NetType::Int, Value::Int(x)) => w.bits(&x.to_le_bytes(), 32),
        (NetType::Float, Value::Float(x)) => w.bits(&x.to_le_bytes(), 32),
        (NetType::Bool, Value::Bool(b)) => w.bit(*b),
        (NetType::Byte { bits }, Value::Byte(x)) => w.bits(&[*x as u8], *bits as usize),
        (NetType::Str, Value::Str(x)) => w.fstring(x),
        (NetType::Name, Value::Name(i)) => w.int(*i, s.names.len() as u32 + 1),
        (NetType::Object { .. }, Value::Object(i)) => {
            w.bit(false);
            w.int(*i, s.map.max_object_index);
        }
        (NetType::Object { .. }, Value::ActorChannel(c)) => {
            w.bit(true);
            w.int(*c, MAX_CHANNELS);
        }
        (NetType::Rotator, Value::Rotator(r)) => {
            for c in r {
                let b = ((*c >> 8) & 0xff) as u8;
                w.bit(b != 0);
                if b != 0 {
                    w.byte(b);
                }
            }
        }
        (NetType::Vector, Value::Vector(v)) => {
            let m = v.iter().map(|c| c.unsigned_abs()).max().unwrap_or(0) + 1;
            let bits = (32 - (m - 1).leading_zeros()).clamp(1, 20) - 1;
            let bits = if m <= 1 { 0 } else { bits };
            w.int(bits, 20);
            let bias = 1i64 << (bits + 1);
            for c in v {
                w.int((*c as i64 + bias) as u32, 1 << (bits + 2));
            }
        }
        _ => return Err(format!("cannot encode {v:?} as {t:?}")),
    }
    Ok(())
}

/// Encodes an RPC call: RepIndex + parameters (bool → bit, others → presence bit + value).
/// Missing parameters are sent as "not present" (default value).
pub fn write_rpc(w: &mut crate::wire::BitsOut, c: &ClassNet, s: &GameSchema, name: &str, args: &[(&str, Value)]) -> Result<(), String> {
    let idx = c.fields.iter().position(|f| f.ty.is_none() && f.name.eq_ignore_ascii_case(name)).ok_or(format!("{} has no RPC {name}", c.name))?;
    w.int(idx as u32, c.fields.len() as u32);
    for (pn, _flags, t) in &c.fields[idx].params {
        let arg = args.iter().find(|a| a.0.eq_ignore_ascii_case(pn)).map(|a| &a.1);
        if matches!(t, NetType::Bool) {
            w.bit(matches!(arg, Some(Value::Bool(true))));
        } else if let Some(v) = arg {
            w.bit(true);
            write_value(w, t, v, s)?;
        } else {
            w.bit(false);
        }
    }
    Ok(())
}
