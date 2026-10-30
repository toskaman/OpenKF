use glam::Vec3;
use hecs::{Entity, World as HecsWorld};
use openkf_world::components::{ActorState, LifecyclePhase, Transform, Velocity};
use openkf_world::lifecycle::{ActorLifecycleHook, SpawnActorBuilder};
use openkf_world::types::Rotator;
use openkf_world::World;
use std::sync::{Arc, Mutex};

struct TestLifecycleTracker {
    pub events: Arc<Mutex<Vec<String>>>,
}

impl ActorLifecycleHook for TestLifecycleTracker {
    fn pre_begin_play(&mut self, _entity: Entity, _world: &mut HecsWorld) {
        self.events.lock().unwrap().push("PreBeginPlay".to_string());
    }

    fn begin_play(&mut self, _entity: Entity, _world: &mut HecsWorld) {
        self.events.lock().unwrap().push("BeginPlay".to_string());
    }

    fn post_begin_play(&mut self, _entity: Entity, _world: &mut HecsWorld) {
        self.events.lock().unwrap().push("PostBeginPlay".to_string());
    }

    fn on_destroy(&mut self, _entity: Entity, _world: &mut HecsWorld) {
        self.events.lock().unwrap().push("OnDestroy".to_string());
    }
}

#[test]
fn test_actor_spawning_lifecycle_chain() {
    let mut world = World::new();
    let events = Arc::new(Mutex::new(Vec::new()));

    let tracker = TestLifecycleTracker {
        events: Arc::clone(&events),
    };

    let builder = SpawnActorBuilder::new()
        .with_position(Vec3::new(10.0, 20.0, 30.0))
        .with_rotator(Rotator::from_degrees(0.0, 90.0, 0.0))
        .with_linear_velocity(Vec3::new(100.0, 0.0, 0.0))
        .with_lifecycle_hook(tracker);

    let entity = world.spawn_actor(builder);

    // Verify events executed in strict Unreal sequence
    let recorded = events.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec!["PreBeginPlay", "BeginPlay", "PostBeginPlay"]
    );

    // Verify final lifecycle state is Active
    let state = world.ecs().get::<&ActorState>(entity).unwrap();
    assert_eq!(state.lifecycle_phase, LifecyclePhase::Active);
    assert!(!state.b_static);
    assert!(!state.b_delete_me);

    // Verify initial transform & velocity
    let transform = world.ecs().get::<&Transform>(entity).unwrap();
    assert_eq!(transform.position, Vec3::new(10.0, 20.0, 30.0));
    let velocity = world.ecs().get::<&Velocity>(entity).unwrap();
    assert_eq!(velocity.linear, Vec3::new(100.0, 0.0, 0.0));
}
