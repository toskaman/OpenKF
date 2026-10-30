use glam::Vec3;
use openkf_world::components::{MonsterType, SpatialIndex, Transform, Velocity, ZedAI};
use openkf_world::pipeline::TickType;
use openkf_world::types::Rotator;
use openkf_world::World;
use std::time::Instant;

#[test]
fn test_multithreaded_actor_iteration_at_scale() {
    let mut world = World::with_config(Default::default(), 512.0);

    const ACTOR_COUNT: usize = 10_000;
    println!("Spawning {} dynamic actors for multithreaded iteration test...", ACTOR_COUNT);

    let spawn_start = Instant::now();
    for i in 0..ACTOR_COUNT {
        let x = (i % 100) as f32 * 64.0;
        let y = (i / 100) as f32 * 64.0;
        let z = 100.0;

        let monster_type = match i % 4 {
            0 => MonsterType::Clot,
            1 => MonsterType::Crawler,
            2 => MonsterType::Gorefast,
            _ => MonsterType::Scrake,
        };

        let entity = world.spawn_zed(
            monster_type,
            Vec3::new(x, y, z),
            Rotator::ZERO,
        );

        // Assign a linear velocity (all moving forward along +X at 200 uu/s)
        if let Ok(mut vel) = world.ecs_mut().get::<&mut Velocity>(entity) {
            vel.linear = Vec3::new(200.0, 0.0, 0.0);
        }

        // Damage Scrakes so they enrage during multithreaded simulation
        if monster_type == MonsterType::Scrake {
            if let Ok(mut ai) = world.ecs_mut().get::<&mut ZedAI>(entity) {
                ai.health = 500.0; // Below 750.0 rage threshold
            }
        }
    }
    println!("Spawned {} actors in {:?}", ACTOR_COUNT, spawn_start.elapsed());

    assert_eq!(world.dynamic_actor_count(), ACTOR_COUNT);

    // Run parallel simulation tick
    let sim_start = Instant::now();
    let dt = 0.05f32; // 50ms step
    let report = world.tick(dt, TickType::All);
    let sim_duration = sim_start.elapsed();

    println!(
        "Ticked {} dynamic actors in {:?} (report duration: {:?})",
        report.dynamic_actors_ticked, sim_duration, report.tick_duration
    );

    assert_eq!(report.dynamic_actors_ticked, ACTOR_COUNT);
    assert!((report.effective_delta_seconds - dt).abs() < 1e-5);

    // Verify positions:
    // Every non-Scrake moved by: 200.0 * 0.05 = 10.0 units along +X.
    // Every Scrake enraged: velocity boosted by 1.8x -> 360.0 uu/s -> moved by 360 * 0.05 = 18.0 units!
    let mut non_scrake_count = 0;
    let mut enraged_scrake_count = 0;

    for (entity, (transform, zed_ai)) in world.ecs().query::<(&Transform, &ZedAI)>().iter() {
        if zed_ai.monster_type == MonsterType::Scrake {
            assert!(zed_ai.is_enraged, "Scrake should have enraged during parallel AI update");
            enraged_scrake_count += 1;
        } else {
            non_scrake_count += 1;
        }

        // Check bounds in spatial grid
        if let Ok(spatial) = world.ecs().get::<&SpatialIndex>(entity) {
            let expected_cell = world.spatial_grid().point_to_cell(transform.position);
            assert_eq!(spatial.cell_coord, expected_cell);
        }
    }

    assert_eq!(enraged_scrake_count, ACTOR_COUNT / 4);
    assert_eq!(non_scrake_count, ACTOR_COUNT - (ACTOR_COUNT / 4));
}

#[test]
fn test_generic_par_for_each_dynamic_custom_system() {
    let mut world = World::new();

    // Spawn 1,000 actors
    for i in 0..1000 {
        world.spawn_zed(
            MonsterType::Clot,
            Vec3::new(i as f32, 0.0, 0.0),
            Rotator::ZERO,
        );
    }

    // Run custom multithreaded system modifying velocity
    world.par_for_each_dynamic(|_entity, _transform, velocity, _ai| {
        velocity.linear = Vec3::new(500.0, 100.0, 0.0);
    });

    // Verify all 1,000 actors received the updated velocity
    for (_, velocity) in world.ecs().query::<&Velocity>().iter() {
        assert_eq!(velocity.linear, Vec3::new(500.0, 100.0, 0.0));
    }
}
