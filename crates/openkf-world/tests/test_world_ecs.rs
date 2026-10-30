use glam::Vec3;
use openkf_world::components::MonsterType;
use openkf_world::types::Rotator;
use openkf_world::World;

#[test]
fn test_archetype_grouping_and_soa_layout() {
    let mut world = World::new();

    // Spawn 10 dynamic Zeds (Transform + Velocity + ActorState + SpatialIndex + ZedAI)
    for i in 0..10 {
        world.spawn_zed(
            MonsterType::Clot,
            Vec3::new(i as f32 * 50.0, 0.0, 0.0),
            Rotator::ZERO,
        );
    }

    // Spawn 5 static decoration actors (Transform + Velocity + ActorState + SpatialIndex)
    for i in 0..5 {
        world.spawn_static_actor(
            Vec3::new(0.0, i as f32 * 100.0, 0.0),
            Rotator::ZERO,
            30.0,
            60.0,
        );
    }

    assert_eq!(world.actor_count(), 15);
    assert_eq!(world.dynamic_actor_count(), 10);
    assert_eq!(world.static_actor_count(), 5);

    // Active archetypes containing living entities
    let active_archetypes: Vec<_> = world.archetypes().filter(|a| a.len() > 0).collect();
    assert!(active_archetypes.len() >= 2);

    for arch in active_archetypes {
        assert!(arch.len() > 0);
    }
}

#[test]
fn test_spatial_hash_queries() {
    let mut world = World::with_config(Default::default(), 256.0);

    // Spawn Zed at origin (0, 0, 0)
    let e_center = world.spawn_zed(MonsterType::Gorefast, Vec3::new(0.0, 0.0, 0.0), Rotator::ZERO);

    // Spawn Zed nearby at (100, 100, 0)
    let e_near = world.spawn_zed(MonsterType::Crawler, Vec3::new(100.0, 100.0, 0.0), Rotator::ZERO);

    // Spawn Zed far away at (5000, 5000, 0)
    let e_far = world.spawn_zed(MonsterType::Scrake, Vec3::new(5000.0, 5000.0, 0.0), Rotator::ZERO);

    // Radius query of 200 units around origin
    let nearby = world.query_radius(Vec3::ZERO, 200.0);
    assert!(nearby.contains(&e_center));
    assert!(nearby.contains(&e_near));
    assert!(!nearby.contains(&e_far));

    // AABB query around far specimen
    let far_box = world.query_aabb(Vec3::new(4900.0, 4900.0, -100.0), Vec3::new(5100.0, 5100.0, 100.0));
    assert!(far_box.contains(&e_far));
    assert!(!far_box.contains(&e_center));

    // Raycast broadphase candidates along line from origin to far target
    let ray_candidates = world.raycast_candidates(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), 300.0);
    assert!(ray_candidates.contains(&e_center));
    assert!(ray_candidates.contains(&e_near));
    assert!(!ray_candidates.contains(&e_far));
}
