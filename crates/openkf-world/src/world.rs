use crate::components::{
    ActorState, LevelInfo, LifecyclePhase, MonsterType, SpatialIndex, Transform, Velocity, ZedAI,
};
use crate::lifecycle::SpawnActorBuilder;
use crate::pipeline::{SimulationTickReport, TickPipeline, TickType};
use crate::spatial::SpatialHashGrid;
use crate::types::Rotator;
use glam::Vec3;
use hecs::{DynamicBundle, Entity, World as HecsWorld};
use rayon::prelude::*;
use std::time::Instant;

/// Archetype-based ECS simulation world replacing legacy Unreal Engine 2.5 `ULevel`.
///
/// In retail Killing Floor / `Engine.dll`:
/// - `ULevel::Actors`: Replaced by `hecs::World` (contiguous archetype columns, cache-friendly SoA).
/// - `ULevel::Hash`: Replaced by `SpatialHashGrid` (fast 3D cell hashing for broadphase collision).
/// - `ULevel::TimeSeconds`: Maintained in `LevelInfo` with clamping [0.0005, 0.20].
/// - `ULevel::Tick`: Replaced by parallel Rayon simulation stages.
pub struct World {
    ecs: HecsWorld,
    spatial_grid: SpatialHashGrid,
    level_info: LevelInfo,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Create a new simulation world with default level settings and spatial hash grid (cell size 512.0).
    pub fn new() -> Self {
        Self {
            ecs: HecsWorld::new(),
            spatial_grid: SpatialHashGrid::new(512.0),
            level_info: LevelInfo::default(),
        }
    }

    /// Create with custom level configuration and spatial grid cell size.
    pub fn with_config(level_info: LevelInfo, cell_size: f32) -> Self {
        Self {
            ecs: HecsWorld::new(),
            spatial_grid: SpatialHashGrid::new(cell_size),
            level_info,
        }
    }

    // --- Accessors ---

    #[inline]
    pub fn ecs(&self) -> &HecsWorld {
        &self.ecs
    }

    #[inline]
    pub fn ecs_mut(&mut self) -> &mut HecsWorld {
        &mut self.ecs
    }

    #[inline]
    pub fn spatial_grid(&self) -> &SpatialHashGrid {
        &self.spatial_grid
    }

    #[inline]
    pub fn spatial_grid_mut(&mut self) -> &mut SpatialHashGrid {
        &mut self.spatial_grid
    }

    #[inline]
    pub fn level_info(&self) -> &LevelInfo {
        &self.level_info
    }

    #[inline]
    pub fn level_info_mut(&mut self) -> &mut LevelInfo {
        &mut self.level_info
    }

    // --- Actor Counts & Archetypes ---

    #[inline]
    pub fn actor_count(&self) -> usize {
        self.ecs.len() as usize
    }

    /// Count of active dynamic ticking actors.
    pub fn dynamic_actor_count(&self) -> usize {
        let mut count = 0;
        for (_, state) in self.ecs.query::<&ActorState>().iter() {
            if state.can_tick() {
                count += 1;
            }
        }
        count
    }

    /// Count of static world actors.
    pub fn static_actor_count(&self) -> usize {
        let mut count = 0;
        for (_, state) in self.ecs.query::<&ActorState>().iter() {
            if state.b_static {
                count += 1;
            }
        }
        count
    }

    /// Count of unique archetypes stored in the ECS world.
    #[inline]
    pub fn archetype_count(&self) -> usize {
        self.ecs.archetypes().len()
    }

    /// Iterate over raw archetypes for low-level inspection.
    pub fn archetypes(&self) -> impl ExactSizeIterator<Item = &hecs::Archetype> {
        self.ecs.archetypes()
    }

    // --- Transient effect budget — hardens retail ROBulletHitEffect / SpriteEmitter FArray 0*2 GPF
    // Retail spawns unlimited ROBulletHitRockEffect/SpriteEmitter629012 on every pellet HitWall;
    // with nailgun mods this floods the transient pool and corrupts the allocator.
    const MAX_TRANSIENT_EFFECTS_PER_TICK: usize = 64;
    const MAX_TOTAL_TRANSIENTS: usize = 512;

    // --- Lifecycle Management (SpawnActor, DestroyActor) ---

    /// Spawn an actor using the builder pattern, running the Unreal lifecycle event chain.
    /// Returns `None`-entity on transient budget overflow — caller must handle gracefully
    /// instead of crashing (retail GPF was uncontrolled FArray::Realloc 0*2 in transient pool).
    pub fn spawn_actor(&mut self, builder: SpawnActorBuilder) -> Entity {
        builder.spawn(&mut self.ecs, &mut self.spatial_grid)
    }

    /// Spawn a transient hit-effect actor with per-tick budget enforcement.
    /// Used for `ROBulletHitEffect` / `ROBulletHitRockEffect` / `SpriteEmitter` chains
    /// triggered by `WSBNailGunProjectile::HitWall`. Returns `None` when budget exhausted
    /// — excess pellets are coalesced instead of crashing.
    pub fn try_spawn_transient_effect(&mut self, builder: SpawnActorBuilder, tick_budget_used: &mut usize) -> Option<Entity> {
        if *tick_budget_used >= Self::MAX_TRANSIENT_EFFECTS_PER_TICK {
            return None;
        }
        let live_transients = self.ecs.query::<&ActorState>().iter().filter(|(_, s)| !s.b_static && !s.b_delete_me).count();
        if live_transients >= Self::MAX_TOTAL_TRANSIENTS {
            return None;
        }
        *tick_budget_used += 1;
        Some(builder.spawn(&mut self.ecs, &mut self.spatial_grid))
    }

    /// Spawn an actor with additional custom archetype components.
    pub fn spawn_actor_with_bundle<B: DynamicBundle>(
        &mut self,
        builder: SpawnActorBuilder,
        bundle: B,
    ) -> Entity {
        builder.spawn_with_bundle(&mut self.ecs, &mut self.spatial_grid, bundle)
    }

    /// Helper to spawn a dynamic Killing Floor Zed specimen.
    pub fn spawn_zed(&mut self, monster_type: MonsterType, position: Vec3, rotator: Rotator) -> Entity {
        let zed_ai = ZedAI::new(monster_type);
        let builder = SpawnActorBuilder::new()
            .with_position(position)
            .with_rotator(rotator)
            .with_spatial(24.0, 48.0); // Standard specimen collision cylinder

        self.spawn_actor_with_bundle(builder, (zed_ai,))
    }

    /// Helper to spawn a static world geometry actor (e.g. static mesh, BSP collision volume).
    pub fn spawn_static_actor(&mut self, position: Vec3, rotator: Rotator, radius: f32, height: f32) -> Entity {
        let builder = SpawnActorBuilder::new()
            .with_position(position)
            .with_rotator(rotator)
            .as_static()
            .with_spatial(radius, height);

        self.spawn_actor(builder)
    }

    /// Mark an actor for destruction (`bDeleteMe = true`).
    /// The actor will be skipped during ticking and cleaned up at the end of the tick pipeline.
    pub fn destroy_actor(&mut self, entity: Entity) -> bool {
        if let Ok(mut state) = self.ecs.get::<&mut ActorState>(entity) {
            if !state.b_delete_me {
                state.b_delete_me = true;
                state.lifecycle_phase = LifecyclePhase::PendingDestroy;
                return true;
            }
        }
        false
    }

    /// Check if an entity is alive and not pending deletion.
    pub fn is_alive(&self, entity: Entity) -> bool {
        if let Ok(state) = self.ecs.get::<&ActorState>(entity) {
            !state.b_delete_me && state.lifecycle_phase != LifecyclePhase::Destroyed
        } else {
            false
        }
    }

    // --- Spatial Queries ---

    /// Query entities in broadphase radius.
    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<Entity> {
        self.spatial_grid.query_radius(center, radius)
    }

    /// Query entities in broadphase AABB.
    pub fn query_aabb(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        self.spatial_grid.query_aabb(min, max)
    }

    /// Raycast broadphase returning candidate entities intersecting ray bounds.
    pub fn raycast_candidates(&self, start: Vec3, dir: Vec3, max_dist: f32) -> Vec<Entity> {
        self.spatial_grid.raycast_candidates(start, dir, max_dist)
    }

    // --- Simulation Pipeline (ULevel::Tick) ---

    /// Execute the complete simulation tick pipeline matching `ULevel::Tick` (`Engine.dll:0x1044fea0`).
    ///
    /// Pipeline Stages:
    /// 1. Performance timer start.
    /// 2. Inbound network sync.
    /// 3. DeltaTime clamping: `clamp(raw_dt * TimeDilation, 0.0005, 0.20)`.
    /// 4. Clock accumulation: `time_seconds += effective_dt`.
    /// 5. Filter & execute actor ticking (skipping `b_static` and `b_delete_me`).
    /// 6. Parallel physics movement and AI simulation across CPU threads via Rayon.
    /// 7. Post-tick cleanup: despawn and unindex all `b_delete_me` actors.
    /// 8. Outbound network sync.
    pub fn tick(&mut self, raw_dt: f32, tick_type: TickType) -> SimulationTickReport {
        let tick_start = Instant::now();
        let mut report = SimulationTickReport {
            raw_delta_seconds: raw_dt,
            ..Default::default()
        };

        // If Synchronize mode and no tick needed
        if tick_type == TickType::Synchronize {
            report.tick_duration = tick_start.elapsed();
            return report;
        }

        // Stage 3 & 4: Clamping and Clock Update (decompiled from 0x1044fffe)
        let effective_dt = TickPipeline::compute_effective_dt(raw_dt, self.level_info.time_dilation);
        report.effective_delta_seconds = effective_dt;
        self.level_info.time_seconds += effective_dt as f64;
        self.level_info.real_time_seconds += raw_dt as f64;
        report.total_time_seconds = self.level_info.time_seconds;

        // Stage 5 & 6: Update timers & count filtered actors
        for (_, state) in self.ecs.query::<&mut ActorState>().iter() {
            if state.b_static {
                report.static_actors_skipped += 1;
            } else if state.b_delete_me {
                report.deleted_actors_skipped += 1;
            } else if state.lifecycle_phase == LifecyclePhase::Active {
                report.dynamic_actors_ticked += 1;
                state.update_timers(effective_dt);
            }
        }

        // Parallel simulation: physics integration & AI rage evaluation
        self.par_simulate_movement_and_ai(effective_dt);

        // Stage 7: Post-Tick Priority List Flush & Despawn Cleanup
        report.actors_despawned_cleanup = self.post_tick_cleanup();

        report.tick_duration = tick_start.elapsed();
        report
    }

    /// Multithreaded simulation: integrates positions and updates AI in parallel via Rayon.
    pub fn par_simulate_movement_and_ai(&mut self, dt: f32) {
        // Collect dynamic actor payloads for parallel processing
        // Struct contains only data needed for movement and AI update
        struct ActorSimPayload {
            entity: Entity,
            transform: Transform,
            velocity: Velocity,
            zed_ai: Option<ZedAI>,
            old_pos: Vec3,
        }

        let mut payloads: Vec<ActorSimPayload> = Vec::new();

        // 1. Gather dynamic actors
        for (entity, (transform, velocity, state)) in
            self.ecs.query::<(&Transform, &Velocity, &ActorState)>().iter()
        {
            if !state.can_tick() {
                continue;
            }

            let zed_ai = self.ecs.get::<&ZedAI>(entity).ok().map(|z| *z);

            payloads.push(ActorSimPayload {
                entity,
                transform: *transform,
                velocity: *velocity,
                zed_ai,
                old_pos: transform.position,
            });
        }

        if payloads.is_empty() {
            return;
        }

        // 2. Parallel simulation execution across all CPU cores using Rayon
        payloads.par_iter_mut().for_each(|payload| {
            // AI state logic (e.g. rage activation on Scrake/Fleshpound)
            if let Some(ref mut ai) = payload.zed_ai {
                ai.update_state(&mut payload.velocity, dt);
            }

            // Linear position integration
            payload.transform.position += payload.velocity.linear * dt;
        });

        // 3. Write back simulated state to ECS and update SpatialHashGrid
        for payload in payloads {
            if let Ok(mut t) = self.ecs.get::<&mut Transform>(payload.entity) {
                *t = payload.transform;
            }
            if let Ok(mut v) = self.ecs.get::<&mut Velocity>(payload.entity) {
                *v = payload.velocity;
            }
            if let Some(ai) = payload.zed_ai {
                if let Ok(mut z) = self.ecs.get::<&mut ZedAI>(payload.entity) {
                    *z = ai;
                }
            }

            // Update spatial grid if actor moved into different cell
            if payload.old_pos != payload.transform.position {
                if let Ok(mut spatial) = self.ecs.get::<&mut SpatialIndex>(payload.entity) {
                    let old_min = spatial.aabb_min;
                    let old_max = spatial.aabb_max;

                    spatial.update_from_position(payload.transform.position);
                    let new_cell = self.spatial_grid.point_to_cell(payload.transform.position);

                    if spatial.cell_coord != new_cell {
                        spatial.cell_coord = new_cell;
                        self.spatial_grid.update(
                            payload.entity,
                            old_min,
                            old_max,
                            spatial.aabb_min,
                            spatial.aabb_max,
                        );
                    }
                }
            }
        }
    }

    /// Generic parallel iteration over all dynamic active actors.
    /// Allows systems to run arbitrary thread-safe closures in parallel across all dynamic actors.
    pub fn par_for_each_dynamic<F>(&mut self, f: F)
    where
        F: Fn(Entity, &mut Transform, &mut Velocity, Option<&mut ZedAI>) + Send + Sync,
    {
        struct WorkItem {
            entity: Entity,
            transform: Transform,
            velocity: Velocity,
            zed_ai: Option<ZedAI>,
        }

        let mut items = Vec::new();
        for (entity, (transform, velocity, state)) in
            self.ecs.query::<(&Transform, &Velocity, &ActorState)>().iter()
        {
            if state.can_tick() {
                let zed_ai = self.ecs.get::<&ZedAI>(entity).ok().map(|z| *z);
                items.push(WorkItem {
                    entity,
                    transform: *transform,
                    velocity: *velocity,
                    zed_ai,
                });
            }
        }

        items.par_iter_mut().for_each(|item| {
            f(
                item.entity,
                &mut item.transform,
                &mut item.velocity,
                item.zed_ai.as_mut(),
            );
        });

        // Write back
        for item in items {
            if let Ok(mut t) = self.ecs.get::<&mut Transform>(item.entity) {
                *t = item.transform;
            }
            if let Ok(mut v) = self.ecs.get::<&mut Velocity>(item.entity) {
                *v = item.velocity;
            }
            if let Some(ai) = item.zed_ai {
                if let Ok(mut z) = self.ecs.get::<&mut ZedAI>(item.entity) {
                    *z = ai;
                }
            }
        }
    }

    /// Post-tick garbage collection: despawns all actors flagged with `b_delete_me`.
    fn post_tick_cleanup(&mut self) -> usize {
        let mut to_despawn = Vec::new();

        for (entity, state) in self.ecs.query::<&ActorState>().iter() {
            if state.b_delete_me || state.lifecycle_phase == LifecyclePhase::PendingDestroy {
                to_despawn.push(entity);
            }
        }

        let count = to_despawn.len();
        for entity in to_despawn {
            // Remove from spatial grid if indexed
            if let Ok(spatial) = self.ecs.get::<&SpatialIndex>(entity) {
                if spatial.is_indexed {
                    self.spatial_grid.remove(entity, spatial.aabb_min, spatial.aabb_max);
                }
            }

            // Despawn from ECS archetype table
            let _ = self.ecs.despawn(entity);
        }

        count
    }
}
