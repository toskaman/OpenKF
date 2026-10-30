use crate::components::{ActorState, LifecyclePhase, SpatialIndex, Transform, Velocity};
use crate::spatial::SpatialHashGrid;
use crate::types::Rotator;
use glam::{Quat, Vec3};
use hecs::{DynamicBundle, Entity, World};

/// Trait implemented by actors that receive lifecycle event callbacks,
/// matching Unreal Engine 2.5 `AActor` event notifications.
pub trait ActorLifecycleHook: Send + Sync {
    /// PreBeginPlay: Called before gameplay starts.
    /// Used for initial stat calculations, perk adjustments, and base attributes.
    fn pre_begin_play(&mut self, _entity: Entity, _world: &mut World) {}

    /// BeginPlay: Called when gameplay begins or immediately upon dynamic spawn.
    /// Used for level triggers, initial objective attachment, and event broadcasts.
    fn begin_play(&mut self, _entity: Entity, _world: &mut World) {}

    /// PostBeginPlay: Called after all actors have had BeginPlay called.
    /// Used for weapon inventories, AI controller attachment, and state machines.
    fn post_begin_play(&mut self, _entity: Entity, _world: &mut World) {}

    /// OnDestroy: Called when an actor is requested for destruction before entity removal.
    fn on_destroy(&mut self, _entity: Entity, _world: &mut World) {}
}

/// A default no-op lifecycle hook.
pub struct DefaultLifecycleHook;
impl ActorLifecycleHook for DefaultLifecycleHook {}

/// Builder pattern for spawning actors following `ULevel::SpawnActor` pipeline (`0x13df70`).
pub struct SpawnActorBuilder {
    pub transform: Transform,
    pub velocity: Velocity,
    pub state: ActorState,
    pub spatial: Option<SpatialIndex>,
    pub hook: Option<Box<dyn ActorLifecycleHook>>,
}

impl Default for SpawnActorBuilder {
    fn default() -> Self {
        Self {
            transform: Transform::IDENTITY,
            velocity: Velocity::ZERO,
            state: ActorState::dynamic(),
            spatial: Some(SpatialIndex::default()),
            hook: None,
        }
    }
}

impl SpawnActorBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_position(mut self, position: Vec3) -> Self {
        self.transform.position = position;
        self
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.transform.rotation = rotation;
        self
    }

    pub fn with_rotator(mut self, rotator: Rotator) -> Self {
        self.transform.rotation = rotator.to_quat();
        self
    }

    pub fn with_velocity(mut self, velocity: Velocity) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn with_linear_velocity(mut self, linear: Vec3) -> Self {
        self.velocity.linear = linear;
        self
    }

    pub fn with_state(mut self, state: ActorState) -> Self {
        self.state = state;
        self
    }

    pub fn as_static(mut self) -> Self {
        self.state = ActorState::static_actor();
        self
    }

    pub fn with_spatial(mut self, radius: f32, height: f32) -> Self {
        self.spatial = Some(SpatialIndex::cylinder(radius, height));
        self
    }

    pub fn without_spatial(mut self) -> Self {
        self.spatial = None;
        self
    }

    pub fn with_lifecycle_hook<H: ActorLifecycleHook + 'static>(mut self, hook: H) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }

    /// Spawns the actor into the ECS world and executes the full Unreal lifecycle event chain.
    pub fn spawn_with_bundle<B: DynamicBundle>(
        mut self,
        ecs: &mut World,
        spatial_grid: &mut SpatialHashGrid,
        extra_components: B,
    ) -> Entity {
        // 1. Initialize bounds if spatial is present
        let mut spatial_comp = self.spatial;
        if let Some(ref mut spatial) = spatial_comp {
            spatial.update_from_position(self.transform.position);
            spatial.cell_coord = spatial_grid.point_to_cell(self.transform.position);
            spatial.is_indexed = true;
        }

        // 2. Allocate Entity in ECS Archetype storage
        self.state.lifecycle_phase = LifecyclePhase::Uninitialized;
        let entity = ecs.spawn((self.transform, self.velocity, self.state));

        // Insert spatial component if requested
        if let Some(spatial) = spatial_comp {
            let _ = ecs.insert_one(entity, spatial);
            spatial_grid.insert(entity, spatial.aabb_min, spatial.aabb_max);
        }

        // Insert additional archetype components
        let _ = ecs.insert(entity, extra_components);

        // 3. Lifecycle Sequence: PreBeginPlay -> BeginPlay -> PostBeginPlay -> Active
        if let Some(mut hook) = self.hook.take() {
            // PreBeginPlay
            if let Ok(mut state) = ecs.get::<&mut ActorState>(entity) {
                state.lifecycle_phase = LifecyclePhase::PreBeginPlayDone;
            }
            hook.pre_begin_play(entity, ecs);

            // BeginPlay
            if let Ok(mut state) = ecs.get::<&mut ActorState>(entity) {
                state.lifecycle_phase = LifecyclePhase::BeginPlayDone;
            }
            hook.begin_play(entity, ecs);

            // PostBeginPlay
            if let Ok(mut state) = ecs.get::<&mut ActorState>(entity) {
                state.lifecycle_phase = LifecyclePhase::PostBeginPlayDone;
            }
            hook.post_begin_play(entity, ecs);

            // Transition to active simulation state
            if let Ok(mut state) = ecs.get::<&mut ActorState>(entity) {
                state.lifecycle_phase = LifecyclePhase::Active;
            }
        } else {
            // Direct transition to Active if no hook is attached
            if let Ok(mut state) = ecs.get::<&mut ActorState>(entity) {
                state.lifecycle_phase = LifecyclePhase::Active;
            }
        }

        entity
    }

    /// Convenience spawn without extra components.
    pub fn spawn(self, ecs: &mut World, spatial_grid: &mut SpatialHashGrid) -> Entity {
        self.spawn_with_bundle(ecs, spatial_grid, ())
    }
}
