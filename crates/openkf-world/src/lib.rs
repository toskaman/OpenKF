//! # OpenKF World & Simulation Crate
//!
//! Archetype-based Entity Component System (ECS) world architecture replacing legacy
//! Unreal Engine 2.5 `ULevel` flat `AActor*` array, specified in `scratch/recon_03_actor_simulation.md`.
//!
//! Features:
//! - Archetype-backed cache-friendly SoA component storage via `hecs`.
//! - Retail Killing Floor v1065 Left-Handed Z-Up coordinate system (+X Forward, +Y Right, +Z Up).
//! - 16-bit integer UE2 Rotator conversions with quaternion math (`glam`).
//! - ULevel::Tick simulation pipeline matching binary disassembly (DeltaTime clamping [0.0005s, 0.20s],
//!   LevelInfo dilation, actor ticking order, static actor skipping, post-tick cleanup).
//! - Actor lifecycle management (`SpawnActor`, `PreBeginPlay`, `BeginPlay`, `PostBeginPlay`, `DestroyActor`).
//! - Spatial hashing broadphase collision index replacing legacy `FCollisionHash`.
//! - High-performance multithreaded simulation execution using Rayon.
//! - High-level level loader for Unreal Engine 2.5 `.rom` packages and ECS actor spawning.

pub mod components;
pub mod level_loader;
pub mod lifecycle;
pub mod pipeline;
pub mod spatial;
pub mod types;
pub mod world;

pub use components::{
    ActorState, LevelInfo, LifecyclePhase, SpatialIndex, TickGroup, Transform, Velocity, ZedAI,
};
pub use level_loader::{
    BspCollisionModel, LevelLoadError, LevelLoader, LevelSummary, PathNodeComponent,
    PlayerSpawnPoint, SpecimenSpawnPoint, TraderZone,
};
pub use lifecycle::{ActorLifecycleHook, SpawnActorBuilder};
pub use pipeline::{SimulationTickReport, TickPipeline, TickType};
pub use spatial::SpatialHashGrid;
pub use types::Rotator;
pub use world::World;

// Re-export core ECS primitives from hecs
pub use hecs::{Entity, Query};
