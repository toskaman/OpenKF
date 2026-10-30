//! Client-side actor-channel state machine: feeds server→client bunches,
//! tracks open actor channels and their replicated state.
//! Used by the capture replayer today, and by the live client later.

use crate::actor_open::{read_actor_open, read_compressed_vector};
use crate::props::{read_rotator, read_updates, Update, Value};
use crate::schema::{ClassNet, GameSchema};
use crate::wire::{Bunch, CHTYPE_ACTOR};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ActorState {
    pub channel: u32,
    pub class: Arc<ClassNet>,
    /// Package object index of the class (dynamic) or of the level actor (static).
    pub object_index: u32,
    pub is_static: bool,
    pub location: [f32; 3],
    pub rotation: [i32; 3],
    pub velocity: [f32; 3],
    pub props: HashMap<String, Value>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SessionStats {
    pub bunches_ok: u64,
    pub bunches_failed: u64,
    pub bunches_unknown_channel: u64,
}

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Opened(u32),
    Updated(u32, Vec<Update>),
    Closed(u32),
    Failed(u32, String),
}

pub struct ActorSession {
    pub schema: GameSchema,
    pub actors: HashMap<u32, ActorState>,
    pub stats: SessionStats,
    /// bNetInitialRotation per class: resolved from .uc defaults when available, else learned from first open.
    rot_choice: HashMap<String, bool>,
    /// Class defaults from the game's .uc sources (bNetInitialRotation, meshes...).
    pub defaults: Option<std::sync::Arc<openkf_package::uc_defaults::ScriptDefaults>>,
}

fn vecf(v: [i32; 3]) -> [f32; 3] {
    [v[0] as f32, v[1] as f32, v[2] as f32]
}

impl ActorSession {
    pub fn new(schema: GameSchema) -> Self {
        Self { schema, actors: HashMap::new(), stats: SessionStats::default(), rot_choice: HashMap::new(), defaults: None }
    }

    fn apply(state: &mut ActorState, updates: &[Update]) {
        for u in updates {
            if let Update::Property { name, element, value } = u {
                match (name.as_str(), value) {
                    ("Location", Value::Vector(v)) => state.location = vecf(*v),
                    ("Rotation", Value::Rotator(r)) => state.rotation = *r,
                    ("Velocity", Value::Vector(v)) => state.velocity = vecf(*v),
                    ("PawnPosition", Value::Struct(m)) => {
                        for (k, v) in m {
                            match (k.as_str(), v) {
                                ("Location", Value::Vector(v)) => state.location = vecf(*v),
                                ("Rotation", Value::Rotator(r)) => state.rotation = *r,
                                ("Velocity", Value::Vector(v)) => state.velocity = vecf(*v),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                let key = if *element > 0 { format!("{name}[{element}]") } else { name.clone() };
                state.props.insert(key, value.clone());
            }
        }
    }

    /// Handles one server→client bunch on a non-control channel.
    pub fn handle(&mut self, b: &Bunch) -> Option<SessionEvent> {
        if b.ch_index == 0 {
            return None;
        }
        if b.ch_type.is_some() && b.ch_type != Some(CHTYPE_ACTOR) {
            return None;
        }
        let mut r = b.reader();
        let mut opened = false;
        if b.open {
            let Ok(o) = read_actor_open(&mut r, self.schema.map.max_object_index) else {
                self.stats.bunches_failed += 1;
                return Some(SessionEvent::Failed(b.ch_index, "bad open".into()));
            };
            let Some(((cs, ce), is_class)) = self.schema.class_of_object(o.object_index) else {
                self.stats.bunches_unknown_channel += 1;
                return None;
            };
            let Some(c) = self.schema.class_net(cs, ce) else {
                self.stats.bunches_unknown_channel += 1;
                return None;
            };
            let mut location = [0.0; 3];
            let mut rotation = [0; 3];
            if is_class {
                if let Ok(v) = read_compressed_vector(&mut r) {
                    location = vecf(v);
                }
                let mut with_rot = r.clone();
                let rot = read_rotator(&mut with_rot).unwrap_or_default();
                let schema = &self.schema;
                let known = self.defaults.as_ref().map(|d| d.get_bool(&c.name, "bNetInitialRotation").unwrap_or(false));
                let use_rot = *self.rot_choice.entry(c.name.clone()).or_insert_with(|| {
                    let a = read_updates(&mut r.clone(), &c, schema).is_ok();
                    let b2 = read_updates(&mut with_rot.clone(), &c, schema).is_ok();
                    let guess = b2 && !a;
                    if let Some(k) = known {
                        if k != guess && std::env::var("OPENKF_DEBUG_ROT").is_ok() {
                            eprintln!("bNetInitialRotation {}: defaults {k}, bits say no-rot ok={a} rot ok={b2}", c.name);
                        }
                        return k;
                    }
                    guess
                });
                if use_rot {
                    r = with_rot;
                    rotation = rot;
                }
            }
            self.actors.insert(
                b.ch_index,
                ActorState {
                    channel: b.ch_index,
                    class: c,
                    object_index: o.object_index,
                    is_static: !is_class,
                    location,
                    rotation,
                    velocity: [0.0; 3],
                    props: HashMap::new(),
                },
            );
            opened = true;
        }
        let ev = if let Some(st) = self.actors.get_mut(&b.ch_index) {
            let class = st.class.clone();
            match read_updates(&mut r, &class, &self.schema) {
                Ok(u) => {
                    self.stats.bunches_ok += 1;
                    Self::apply(st, &u);
                    if opened { SessionEvent::Opened(b.ch_index) } else { SessionEvent::Updated(b.ch_index, u) }
                }
                Err((u, e)) => {
                    self.stats.bunches_failed += 1;
                    Self::apply(st, &u);
                    SessionEvent::Failed(b.ch_index, e)
                }
            }
        } else {
            self.stats.bunches_unknown_channel += 1;
            return None;
        };
        if b.close {
            self.actors.remove(&b.ch_index);
            return Some(SessionEvent::Closed(b.ch_index));
        }
        Some(ev)
    }
}
