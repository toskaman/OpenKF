//! OpenKF Physics and Collision Engine.
//!
//! Subsystem Pillar 04:
//! - Exact coordinate and unit conversions between Unreal Units (Z-Up, Left-Handed) and SI Metric (Right-Handed).
//! - Analytical collision primitives: Bounding Cylinders (radius, height), Boxes, Spheres, Capsules, Planes.
//! - Discrete Oriented Polytopes: kDOP-18 and kDOP-26 hulls and hierarchical trees for static meshes.
//! - Binary Space Partitioning (BSP) tree ray clipping.
//! - Unreal Engine 2.5 trace pipelines (`SingleLineCheck`, `MultiLineCheck`, `PointCheck`, `FCheckResult`, `TraceFlags`).
//! - Karma rigid body and ragdoll runtime bindings (`KRigidBodyState`, 42 AActor native operations).

pub mod coords;
pub mod primitives;
pub mod kdop;
pub mod bsp;
pub mod raycast;
pub mod karma;
pub mod world;

// Re-exports of primary types
pub use coords::{
    ue_to_metric_position, metric_to_ue_position,
    ue_to_metric_velocity, metric_to_ue_velocity,
    ue_to_metric_direction, metric_to_ue_direction,
    ue_to_metric_quat, metric_to_ue_quat,
    ue_to_jolt_zup_quat, jolt_zup_to_ue_quat,
    ue_to_si_scale, si_to_ue_scale,
    UnrealRotator, UU_TO_METERS, METERS_TO_UU,
};

pub use primitives::{
    Ray, RayHit, Aabb, Sphere, BoundingCylinder, Capsule, Plane,
};

pub use kdop::{
    KDop18, KDop26, KDopCollisionTriangle, KDopNode, KDopTree,
    KDOP18_DIRECTIONS, KDOP26_DIRECTIONS,
};

pub use bsp::{BspNode, BspTree, surf_flags};

pub use raycast::{FCheckResult, TraceFlags};

pub use karma::{
    KRigidBodyState, KarmaPhysicsBody, KarmaPrimitiveShape, KarmaCollisionElement, BoneLifter,
};

pub use world::{CollisionWorld, CollisionActor, CollisionShape};
