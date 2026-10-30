//! Raycast and sweep query types: FCheckResult, TraceFlags, and query definitions.

use bitflags::bitflags;
use glam::Vec3;

bitflags! {
    /// Trace flags bitmask controlling collision filtering in Unreal Engine 2.5.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TraceFlags: u32 {
        /// Check pawns (monsters, players, bots).
        const PAWNS             = 0x00000001;
        /// Check moving brush geometry (doors, lifts).
        const MOVERS            = 0x00000002;
        /// Check BSP world geometry.
        const LEVEL             = 0x00000004;
        /// Trace across zone boundaries.
        const ZONE_CHANGES       = 0x00000008;
        /// Check generic dynamic actors.
        const OTHERS            = 0x00000010;
        /// Only actors blocking projectiles.
        const ONLY_PROJ_ACTORS  = 0x00000020;
        /// Only check solid blocking geometry.
        const BLOCKING          = 0x00000040;
        /// Extended level geometry checks.
        const LEVEL_GEOMETRY     = 0x00000080;
        /// Shadow ray visibility test.
        const SHADOW_CAST        = 0x00000100;
        /// Fast early-out on first intersection encountered.
        const STOP_AT_FIRST_HIT  = 0x00000200;
        /// Return single nearest result.
        const SINGLE_RESULT      = 0x00000400;
        /// Query material at impact.
        const MATERIAL          = 0x00000800;
        /// Visibility ray (ignores clear glass).
        const VISIBLE           = 0x00001000;
        /// Decal projector reception trace.
        const ACCEPT_PROJECTORS  = 0x00002000;
        /// Check heightmap terrain.
        const TERRAIN           = 0x00004000;
        /// Check static mesh kDOP hulls.
        const STATIC_MESHES      = 0x00008000;

        // Composite bitmasks
        const ACTORS            = Self::PAWNS.bits() | Self::MOVERS.bits() | Self::OTHERS.bits();
        const WORLD             = Self::LEVEL.bits() | Self::TERRAIN.bits() | Self::STATIC_MESHES.bits();
        const ALL_COLLIDING     = Self::ACTORS.bits() | Self::WORLD.bits();
    }
}

/// Represents the collision result structure `FCheckResult` from Unreal Engine 2.5 `Engine.dll`.
#[derive(Debug, Clone, PartialEq)]
pub struct FCheckResult {
    /// Impact point in world coordinates (X, Y, Z).
    pub location: Vec3,
    /// Surface normal at impact point (unit vector).
    pub normal: Vec3,
    /// Normalized time along trace [0.0 .. 1.0].
    pub time: f32,
    /// Hit Actor ID / index if an actor was struck.
    pub actor_id: Option<u32>,
    /// Hit primitive ID (StaticMesh/Model/Pawn cylinder/box/sphere).
    pub primitive_id: Option<u32>,
    /// Sub-item index (e.g., triangle, bone, face index).
    pub item: i32,
    /// Material ID at hit location (for decal/blood impact FX).
    pub material_id: Option<u32>,
}

impl FCheckResult {
    pub fn new(
        location: Vec3,
        normal: Vec3,
        time: f32,
        actor_id: Option<u32>,
        primitive_id: Option<u32>,
        item: i32,
    ) -> Self {
        Self {
            location,
            normal,
            time,
            actor_id,
            primitive_id,
            item,
            material_id: None,
        }
    }
}
