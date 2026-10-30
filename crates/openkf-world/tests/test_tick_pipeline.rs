use glam::Vec3;
use openkf_world::components::{ActorState, MonsterType};
use openkf_world::pipeline::{TickPipeline, TickType, MAX_DELTA_SECONDS, MIN_DELTA_SECONDS};
use openkf_world::types::Rotator;
use openkf_world::World;

#[test]
fn test_delta_time_clamping_math() {
    // 1. Underflow clamping (< 0.0005s)
    let small_dt = 0.00005;
    assert_eq!(TickPipeline::compute_effective_dt(small_dt, 1.0), MIN_DELTA_SECONDS);

    // 2. Overflow lag spike clamping (> 0.20s)
    let lag_spike = 1.5;
    assert_eq!(TickPipeline::compute_effective_dt(lag_spike, 1.0), MAX_DELTA_SECONDS);

    // 3. Normal 60 FPS frame (1/60s ~= 0.016667s)
    let normal_dt = 0.016667;
    assert!((TickPipeline::compute_effective_dt(normal_dt, 1.0) - normal_dt).abs() < 1e-6);

    // 4. Zed Time Dilation (0.20x speed)
    let zed_time_dilation = 0.20;
    let effective = TickPipeline::compute_effective_dt(0.016, zed_time_dilation);
    assert!((effective - (0.016 * 0.20)).abs() < 1e-5);

    // 5. Extreme slow motion underflow clamping with dilation
    let extreme_slow = TickPipeline::compute_effective_dt(0.001, 0.10);
    assert_eq!(extreme_slow, MIN_DELTA_SECONDS);
}

#[test]
fn test_simulation_tick_pipeline_and_clock_accumulation() {
    let mut world = World::new();

    // Spawn 2 dynamic zeds
    let zed1 = world.spawn_zed(MonsterType::Clot, Vec3::new(0.0, 0.0, 0.0), Rotator::ZERO);
    let zed2 = world.spawn_zed(MonsterType::Crawler, Vec3::new(100.0, 0.0, 0.0), Rotator::ZERO);

    // Spawn 1 static actor
    let _static_actor = world.spawn_static_actor(Vec3::new(0.0, 500.0, 0.0), Rotator::ZERO, 50.0, 50.0);

    // Tick 1: 0.02s
    let report1 = world.tick(0.02, TickType::All);
    assert_eq!(report1.dynamic_actors_ticked, 2);
    assert_eq!(report1.static_actors_skipped, 1);
    assert_eq!(report1.deleted_actors_skipped, 0);
    assert!((report1.effective_delta_seconds - 0.02).abs() < 1e-5);
    assert!((world.level_info().time_seconds - 0.02).abs() < 1e-5);

    // Mark zed1 for destruction
    assert!(world.destroy_actor(zed1));

    // Tick 2: 0.05s
    let report2 = world.tick(0.05, TickType::All);
    assert_eq!(report2.deleted_actors_skipped, 1); // zed1 skipped during actor iteration
    assert_eq!(report2.dynamic_actors_ticked, 1);  // only zed2 ticked
    assert_eq!(report2.static_actors_skipped, 1);  // static_actor skipped
    assert_eq!(report2.actors_despawned_cleanup, 1); // zed1 cleaned up in post-tick

    // After cleanup, zed1 is no longer alive or in ECS
    assert!(!world.is_alive(zed1));
    assert!(world.is_alive(zed2));
    assert_eq!(world.actor_count(), 2); // zed2 and static_actor remain
}

#[test]
fn test_actor_timer_countdown_and_looping() {
    let mut world = World::new();

    let entity = world.spawn_zed(MonsterType::Stalker, Vec3::ZERO, Rotator::ZERO);

    // Set a one-shot timer for 0.05s and a looping timer for 0.02s
    if let Ok(mut state) = world.ecs_mut().get::<&mut ActorState>(entity) {
        state.set_timer("CloakCheck", 0.05, false);
        state.set_timer("Heartbeat", 0.02, true);
    }

    // Tick 0.015s -> neither timer triggered yet
    world.tick(0.015, TickType::All);
    {
        let state = world.ecs().get::<&ActorState>(entity).unwrap();
        let cloak = state.timers.iter().find(|t| t.name == "CloakCheck").unwrap();
        let hb = state.timers.iter().find(|t| t.name == "Heartbeat").unwrap();
        assert!(!cloak.triggered);
        assert!(!hb.triggered);
    }

    // Tick another 0.01s (total 0.025s) -> Heartbeat triggered, CloakCheck not yet
    world.tick(0.010, TickType::All);
    {
        let state = world.ecs().get::<&ActorState>(entity).unwrap();
        let cloak = state.timers.iter().find(|t| t.name == "CloakCheck").unwrap();
        let hb = state.timers.iter().find(|t| t.name == "Heartbeat").unwrap();
        assert!(!cloak.triggered);
        assert!(hb.triggered);
        assert!(hb.is_looping);
    }

    // Tick another 0.03s (total 0.055s) -> CloakCheck now triggered
    world.tick(0.030, TickType::All);
    {
        let state = world.ecs().get::<&ActorState>(entity).unwrap();
        let cloak = state.timers.iter().find(|t| t.name == "CloakCheck").unwrap();
        assert!(cloak.triggered);
    }
}
