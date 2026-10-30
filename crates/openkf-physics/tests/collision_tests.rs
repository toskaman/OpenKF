use glam::Vec3;
use openkf_physics::{
    metric_to_ue_direction, metric_to_ue_position, metric_to_ue_quat,
    ue_to_metric_direction, ue_to_metric_position, ue_to_metric_quat,
    Aabb, BoundingCylinder, BspNode, BspTree, CollisionActor, CollisionShape,
    CollisionWorld, KDop18, KDop26, KarmaPhysicsBody, Plane,
    Ray, Sphere, TraceFlags, UnrealRotator, UU_TO_METERS,
};

#[test]
fn test_coordinate_conversion_exactness() {
    // 1 UU = 0.02 m
    assert_eq!(1.0 * UU_TO_METERS, 0.02);

    // Exact formula: (x, y, z) -> (0.02x, 0.02z, -0.02y)
    let ue_pos = Vec3::new(100.0, 200.0, 300.0);
    let si_pos = ue_to_metric_position(ue_pos);
    assert_eq!(si_pos.x, 2.0);
    assert_eq!(si_pos.y, 6.0);
    assert_eq!(si_pos.z, -4.0);

    // Round-trip recovery
    let recovered_ue = metric_to_ue_position(si_pos);
    assert!((recovered_ue - ue_pos).length() < 1e-5);

    // Normal direction conversion
    let ue_norm = Vec3::new(0.0, 1.0, 0.0);
    let si_norm = ue_to_metric_direction(ue_norm);
    assert_eq!(si_norm, Vec3::new(0.0, 0.0, -1.0));
    assert_eq!(metric_to_ue_direction(si_norm), ue_norm);

    // Rotator conversion
    let rotator = UnrealRotator::new(16384, 0, 0); // 90 deg pitch
    let quat = rotator.to_quat();
    assert!(quat.is_normalized());

    // Quaternion basis change round-trip
    let q_si = ue_to_metric_quat(quat);
    let q_back = metric_to_ue_quat(q_si);
    assert!((q_back.dot(quat)).abs() > 0.999);
}

#[test]
fn test_raycast_bounding_cylinder() {
    // Standard KF Zed cylinder: Center at (0, 0, 50), Radius = 30, HalfHeight = 50 (height spans 0 to 100)
    let cyl = BoundingCylinder::new(Vec3::new(0.0, 0.0, 50.0), 30.0, 50.0);

    assert!(cyl.contains_point(Vec3::new(0.0, 0.0, 50.0)));
    assert!(cyl.contains_point(Vec3::new(25.0, 0.0, 90.0)));
    assert!(!cyl.contains_point(Vec3::new(35.0, 0.0, 50.0)));
    assert!(!cyl.contains_point(Vec3::new(0.0, 0.0, 105.0)));

    // 1. Ray hitting side horizontally from -X
    let ray_side = Ray::from_endpoints(Vec3::new(-100.0, 0.0, 50.0), Vec3::new(100.0, 0.0, 50.0));
    let hit_side = cyl.intersects_ray(&ray_side).expect("Side ray should hit");
    assert!((hit_side.distance - 70.0).abs() < 1e-4);
    assert!((hit_side.location - Vec3::new(-30.0, 0.0, 50.0)).length() < 1e-4);
    assert_eq!(hit_side.normal, Vec3::new(-1.0, 0.0, 0.0));

    // 2. Ray hitting top cap from above (+Z)
    let ray_top = Ray::from_endpoints(Vec3::new(0.0, 0.0, 200.0), Vec3::new(0.0, 0.0, 0.0));
    let hit_top = cyl.intersects_ray(&ray_top).expect("Top ray should hit");
    assert!((hit_top.distance - 100.0).abs() < 1e-4);
    assert!((hit_top.location - Vec3::new(0.0, 0.0, 100.0)).length() < 1e-4);
    assert_eq!(hit_top.normal, Vec3::new(0.0, 0.0, 1.0));

    // 3. Ray missing cylinder
    let ray_miss = Ray::from_endpoints(Vec3::new(-100.0, 50.0, 50.0), Vec3::new(100.0, 50.0, 50.0));
    assert!(cyl.intersects_ray(&ray_miss).is_none());
}

#[test]
fn test_raycast_box() {
    let aabb = Aabb::from_center_extents(Vec3::new(0.0, 0.0, 0.0), Vec3::new(50.0, 50.0, 50.0));

    let ray = Ray::from_endpoints(Vec3::new(0.0, -100.0, 0.0), Vec3::new(0.0, 100.0, 0.0));
    let hit = aabb.intersects_ray(&ray).expect("Box should hit");
    assert!((hit.distance - 50.0).abs() < 1e-4);
    assert_eq!(hit.location, Vec3::new(0.0, -50.0, 0.0));
    assert_eq!(hit.normal, Vec3::new(0.0, -1.0, 0.0));

    // Ray origin inside box
    let inside_ray = Ray::from_endpoints(Vec3::new(0.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 0.0));
    let inside_hit = aabb.intersects_ray(&inside_ray).expect("Inside hit");
    assert_eq!(inside_hit.time, 0.0);
}

#[test]
fn test_raycast_sphere() {
    let sphere = Sphere::new(Vec3::new(100.0, 0.0, 0.0), 25.0);

    let ray = Ray::from_endpoints(Vec3::new(0.0, 0.0, 0.0), Vec3::new(200.0, 0.0, 0.0));
    let hit = sphere.intersects_ray(&ray).expect("Sphere should hit");
    assert!((hit.distance - 75.0).abs() < 1e-4);
    assert_eq!(hit.location, Vec3::new(75.0, 0.0, 0.0));
    assert_eq!(hit.normal, Vec3::new(-1.0, 0.0, 0.0));
}

#[test]
fn test_kdop_18_and_26_hulls() {
    let cube_verts = [
        Vec3::new(-10.0, -10.0, -10.0),
        Vec3::new(10.0, -10.0, -10.0),
        Vec3::new(-10.0, 10.0, -10.0),
        Vec3::new(10.0, 10.0, -10.0),
        Vec3::new(-10.0, -10.0, 10.0),
        Vec3::new(10.0, -10.0, 10.0),
        Vec3::new(-10.0, 10.0, 10.0),
        Vec3::new(10.0, 10.0, 10.0),
    ];

    let kdop18 = KDop18::from_vertices(&cube_verts);
    let kdop26 = KDop26::from_vertices(&cube_verts);

    assert!(kdop18.contains_point(Vec3::ZERO));
    assert!(kdop26.contains_point(Vec3::ZERO));
    assert!(!kdop18.contains_point(Vec3::new(20.0, 0.0, 0.0)));
    assert!(!kdop26.contains_point(Vec3::new(20.0, 0.0, 0.0)));

    // Raycast through center
    let ray = Ray::from_endpoints(Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0));
    let hit18 = kdop18.intersects_ray(&ray).expect("kDOP-18 raycast");
    let hit26 = kdop26.intersects_ray(&ray).expect("kDOP-26 raycast");

    assert!((hit18.distance - 40.0).abs() < 1e-4);
    assert!((hit26.distance - 40.0).abs() < 1e-4);
    assert_eq!(hit18.normal, Vec3::new(-1.0, 0.0, 0.0));
    assert_eq!(hit26.normal, Vec3::new(-1.0, 0.0, 0.0));
}

#[test]
fn test_bsp_tree_clipping() {
    let mut bsp = BspTree::new();
    // Outward plane normal pointing from solid wall (X >= 100) into empty room (X < 100) is (-1, 0, 0)
    let wall_plane = Plane::from_point_normal(Vec3::new(100.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
    let solid_leaf = bsp.add_node(BspNode::new_leaf(true, openkf_physics::surf_flags::PF_BLOCKING));
    let empty_leaf = bsp.add_node(BspNode::new_leaf(false, 0));
    let split_node = bsp.add_node(BspNode::new_split(
        wall_plane,
        Some(empty_leaf),
        Some(solid_leaf),
    ));
    bsp.root_index = Some(split_node);

    let ray = Ray::from_endpoints(Vec3::new(0.0, 0.0, 0.0), Vec3::new(200.0, 0.0, 0.0));
    let hit = bsp.raycast(&ray).expect("BSP should clip and hit wall at X = 100");
    assert!((hit.distance - 100.0).abs() < 1e-4);
    assert_eq!(hit.normal, Vec3::new(-1.0, 0.0, 0.0));
}

#[test]
fn test_single_and_multi_line_check() {
    let mut world = CollisionWorld::new();

    // Actor 1: Clot pawn with cylinder at (100, 0, 50)
    let clot = CollisionActor::new(
        1001,
        CollisionShape::Cylinder(BoundingCylinder::new(Vec3::new(100.0, 0.0, 50.0), 20.0, 50.0)),
        TraceFlags::PAWNS,
    );
    world.add_actor(clot);

    // Actor 2: Fleshpound pawn with cylinder at (200, 0, 60)
    let fp = CollisionActor::new(
        1002,
        CollisionShape::Cylinder(BoundingCylinder::new(Vec3::new(200.0, 0.0, 60.0), 35.0, 60.0)),
        TraceFlags::PAWNS,
    );
    world.add_actor(fp);

    // 1. SingleLineCheck hitting the nearest pawn (Clot)
    let hit = world.single_line_check(
        Vec3::new(0.0, 0.0, 50.0),
        Vec3::new(300.0, 0.0, 50.0),
        TraceFlags::PAWNS,
        Vec3::ZERO,
        None,
    ).expect("Should hit nearest actor");

    assert_eq!(hit.actor_id, Some(1001));
    assert!((hit.location.x - 80.0).abs() < 1e-4);

    // 2. MultiLineCheck returning both pawns ordered by distance
    let hits = world.multi_line_check(
        Vec3::new(0.0, 0.0, 50.0),
        Vec3::new(300.0, 0.0, 50.0),
        TraceFlags::PAWNS,
        Vec3::ZERO,
        None,
    );

    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].actor_id, Some(1001));
    assert_eq!(hits[1].actor_id, Some(1002));
    assert!(hits[0].time < hits[1].time);

    // 3. Ignore Actor filter
    let hit_ignoring_clot = world.single_line_check(
        Vec3::new(0.0, 0.0, 50.0),
        Vec3::new(300.0, 0.0, 50.0),
        TraceFlags::PAWNS,
        Vec3::ZERO,
        Some(1001),
    ).expect("Should hit Fleshpound when ignoring Clot");

    assert_eq!(hit_ignoring_clot.actor_id, Some(1002));
}

#[test]
fn test_swept_box_trace() {
    let mut world = CollisionWorld::new();

    // Doorway obstacle box at X = 100
    let door = CollisionActor::new(
        500,
        CollisionShape::Box(Aabb::from_center_extents(Vec3::new(100.0, 0.0, 50.0), Vec3::new(10.0, 50.0, 50.0))),
        TraceFlags::MOVERS,
    );
    world.add_actor(door);

    // Swept pawn extent: radius 20, half-height 44 -> extent (20, 20, 44)
    let extent = Vec3::new(20.0, 20.0, 44.0);

    let hit = world.single_line_check(
        Vec3::new(0.0, 0.0, 50.0),
        Vec3::new(200.0, 0.0, 50.0),
        TraceFlags::MOVERS,
        extent,
        None,
    ).expect("Swept trace should hit earlier due to extent");

    // Door min X is 100 - 10 = 90.
    // With extent 20, impact should be detected at X = 90 - 20 = 70.
    assert!((hit.location.x - 70.0).abs() < 1e-4);
}

#[test]
fn test_karma_physics_body_impulse_and_step() {
    let mut body = KarmaPhysicsBody::new(80.0);
    body.state.position = Vec3::new(0.0, 0.0, 100.0);

    // Apply linear impulse
    body.k_add_impulse(Vec3::new(800.0, 0.0, 0.0), None);
    assert_eq!(body.state.linear_velocity, Vec3::new(10.0, 0.0, 0.0));

    // Step physics with gravity -980 UU/s^2 (Unreal standard gravity: -980 UU/s^2)
    body.step(0.1, Vec3::new(0.0, 0.0, -980.0));

    assert!(body.state.linear_velocity.z < 0.0);
    assert!(body.state.position.x > 0.0);
}
