use crate::types::Rotator;
use glam::{Mat4, Quat, Vec3};
use hecs::Entity;

/// 3D Transform in Left-Handed Z-Up coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    #[inline]
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    #[inline]
    pub const fn from_position(position: Vec3) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    #[inline]
    pub const fn from_pos_rot(position: Vec3, rotation: Quat) -> Self {
        Self {
            position,
            rotation,
            scale: Vec3::ONE,
        }
    }

    #[inline]
    pub fn from_pos_rotator(position: Vec3, rotator: Rotator) -> Self {
        Self {
            position,
            rotation: rotator.to_quat(),
            scale: Vec3::ONE,
        }
    }

    /// Forward direction (+X rotated by rotation).
    #[inline]
    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    /// Right direction (+Y rotated by rotation).
    #[inline]
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    /// Up direction (+Z rotated by rotation).
    #[inline]
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }

    /// Convert rotation quaternion to UE2 Rotator.
    #[inline]
    pub fn rotator(&self) -> Rotator {
        Rotator::from_quat(self.rotation)
    }

    /// Build standard 4x4 transformation matrix.
    #[inline]
    pub fn to_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

/// Linear and angular velocity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Velocity {
    pub linear: Vec3,
    pub angular: Vec3,
}

impl Velocity {
    pub const ZERO: Self = Self {
        linear: Vec3::ZERO,
        angular: Vec3::ZERO,
    };

    #[inline]
    pub const fn new(linear: Vec3, angular: Vec3) -> Self {
        Self { linear, angular }
    }

    #[inline]
    pub const fn from_linear(linear: Vec3) -> Self {
        Self {
            linear,
            angular: Vec3::ZERO,
        }
    }

    #[inline]
    pub fn speed(&self) -> f32 {
        self.linear.length()
    }

    #[inline]
    pub fn speed_2d(&self) -> f32 {
        Vec3::new(self.linear.x, self.linear.y, 0.0).length()
    }

    /// Euler integration of linear position.
    #[inline]
    pub fn integrate(&self, transform: &mut Transform, dt: f32) {
        transform.position += self.linear * dt;
    }
}

/// Actor lifecycle states in Unreal Engine 2.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifecyclePhase {
    #[default]
    Uninitialized,
    PreBeginPlayDone,
    BeginPlayDone,
    PostBeginPlayDone,
    Active,
    PendingDestroy,
    Destroyed,
}

/// Execution tick group ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub enum TickGroup {
    PrePhysics,
    #[default]
    DuringPhysics,
    PostPhysics,
}

/// Timer scheduled on an Actor (similar to UnrealScript SetTimer).
#[derive(Debug, Clone, PartialEq)]
pub struct ActorTimer {
    pub name: String,
    pub rate: f32,
    pub count_down: f32,
    pub is_looping: bool,
    pub triggered: bool,
}

/// Unreal Actor state flags and lifecycle tracking.
/// Mirrors AActor core fields from Engine.dll:
/// - `bStatic` (disassembled: if set, actor is skipped by ULevel::Tick)
/// - `bDeleteMe` (disassembled: if set, actor is skipped and collected in post-tick)
#[derive(Debug, Clone, PartialEq)]
pub struct ActorState {
    pub b_static: bool,
    pub b_delete_me: bool,
    pub b_hidden: bool,
    pub b_collide_actors: bool,
    pub b_block_actors: bool,
    pub b_block_players: bool,
    pub b_always_tick: bool,
    pub lifecycle_phase: LifecyclePhase,
    pub tick_group: TickGroup,
    pub state_name: String,
    pub timers: Vec<ActorTimer>,
}

impl Default for ActorState {
    fn default() -> Self {
        Self {
            b_static: false,
            b_delete_me: false,
            b_hidden: false,
            b_collide_actors: true,
            b_block_actors: false,
            b_block_players: false,
            b_always_tick: false,
            lifecycle_phase: LifecyclePhase::Uninitialized,
            tick_group: TickGroup::DuringPhysics,
            state_name: "Auto".to_string(),
            timers: Vec::new(),
        }
    }
}

impl ActorState {
    /// Create state for dynamic ticking actor.
    pub fn dynamic() -> Self {
        Self::default()
    }

    /// Create state for static world geometry (never ticked).
    pub fn static_actor() -> Self {
        Self {
            b_static: true,
            b_collide_actors: true,
            b_block_actors: true,
            ..Default::default()
        }
    }

    /// Check if actor should receive ticks.
    #[inline]
    pub fn can_tick(&self) -> bool {
        !self.b_static && !self.b_delete_me && self.lifecycle_phase == LifecyclePhase::Active
    }

    /// Add a timer to this actor.
    pub fn set_timer(&mut self, name: impl Into<String>, rate: f32, looping: bool) {
        let name = name.into();
        if let Some(timer) = self.timers.iter_mut().find(|t| t.name == name) {
            timer.rate = rate;
            timer.count_down = rate;
            timer.is_looping = looping;
            timer.triggered = false;
        } else {
            self.timers.push(ActorTimer {
                name,
                rate,
                count_down: rate,
                is_looping: looping,
                triggered: false,
            });
        }
    }

    /// Update timers for an effective delta time.
    pub fn update_timers(&mut self, dt: f32) {
        for timer in &mut self.timers {
            timer.count_down -= dt;
            if timer.count_down <= 0.0 {
                timer.triggered = true;
                if timer.is_looping {
                    timer.count_down += timer.rate;
                }
            }
        }
    }
}

/// Spatial bounding and broadphase registration for collision and raycasting.
/// Replaces UE2 `FCollisionHash` leaf references.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialIndex {
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub radius: f32,
    pub height: f32,
    pub cell_coord: glam::IVec3,
    pub is_indexed: bool,
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self {
            aabb_min: Vec3::ZERO,
            aabb_max: Vec3::ZERO,
            radius: 17.0, // Default KF human pawn radius
            height: 48.0, // Default KF human pawn half-height
            cell_coord: glam::IVec3::ZERO,
            is_indexed: false,
        }
    }
}

impl SpatialIndex {
    pub fn cylinder(radius: f32, height: f32) -> Self {
        Self {
            radius,
            height,
            ..Default::default()
        }
    }

    /// Recompute axis-aligned bounding box from a position.
    pub fn update_from_position(&mut self, position: Vec3) {
        let extents = Vec3::new(self.radius, self.radius, self.height);
        self.aabb_min = position - extents;
        self.aabb_max = position + extents;
    }
}

/// Global level simulation clock and properties.
/// Mirrors `ULevelSummary` / `ALevelInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelInfo {
    pub time_dilation: f32,
    pub time_seconds: f64,
    pub real_time_seconds: f64,
    pub level_name: String,
    pub default_gravity: f32,
}

impl Default for LevelInfo {
    fn default() -> Self {
        Self {
            time_dilation: 1.0,
            time_seconds: 0.0,
            real_time_seconds: 0.0,
            level_name: "KF-Level".to_string(),
            default_gravity: -950.0, // Standard UE2 gravity (-950 uu/s^2)
        }
    }
}

/// Retail Killing Floor Zed monster types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MonsterType {
    Clot,
    Bloat,
    Crawler,
    Gorefast,
    Stalker,
    Scrake,
    Fleshpound,
    Siren,
    Patriarch,
}

/// Zed AI Component for Killing Floor specimen simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZedAI {
    pub monster_type: MonsterType,
    pub health: f32,
    pub max_health: f32,
    pub headshot_multiplier: f32,
    pub rage_threshold: f32,
    pub is_enraged: bool,
    pub target_player: Option<Entity>,
}

impl ZedAI {
    pub fn new(monster_type: MonsterType) -> Self {
        let (max_hp, headshot, rage_thresh) = match monster_type {
            MonsterType::Clot => (130.0, 1.25, 0.0),
            MonsterType::Bloat => (525.0, 1.5, 0.0),
            MonsterType::Crawler => (70.0, 1.25, 0.0),
            MonsterType::Gorefast => (250.0, 1.25, 0.0),
            MonsterType::Stalker => (100.0, 1.25, 0.0),
            MonsterType::Siren => (350.0, 1.5, 0.0),
            MonsterType::Scrake => (1500.0, 1.5, 750.0),
            MonsterType::Fleshpound => (2000.0, 1.1, 1500.0),
            MonsterType::Patriarch => (4000.0, 1.0, 2000.0),
        };

        Self {
            monster_type,
            health: max_hp,
            max_health: max_hp,
            headshot_multiplier: headshot,
            rage_threshold: rage_thresh,
            is_enraged: false,
            target_player: None,
        }
    }

    /// Update AI rage state based on health loss.
    pub fn update_state(&mut self, velocity: &mut Velocity, _dt: f32) {
        if self.rage_threshold > 0.0 && self.health < self.rage_threshold && !self.is_enraged {
            self.is_enraged = true;
            // Rage speed boost (+50% to +100%)
            velocity.linear *= 1.8;
        }
    }
}
