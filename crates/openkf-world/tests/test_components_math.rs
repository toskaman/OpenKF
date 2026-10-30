use glam::Vec3;
use openkf_world::components::{Transform, Velocity};
use openkf_world::types::Rotator;
use std::f32::consts::PI;

#[test]
fn test_rotator_degree_radian_conversions() {
    let r_yaw_90 = Rotator::from_degrees(0.0, 90.0, 0.0);
    // 90 degrees = 65536 / 4 = 16384 units
    assert_eq!(r_yaw_90.yaw, 16384);

    let (p, y, r) = r_yaw_90.to_degrees();
    assert!((p - 0.0).abs() < 1e-4);
    assert!((y - 90.0).abs() < 1e-4);
    assert!((r - 0.0).abs() < 1e-4);

    let (_pr, yr, _rr) = r_yaw_90.to_radians();
    assert!((yr - (PI / 2.0)).abs() < 1e-4);
}

#[test]
fn test_rotator_quaternion_left_handed_axes() {
    // In UE2 Left-Handed Z-Up:
    // +X is Forward, +Y is Right, +Z is Up
    let rot_identity = Rotator::ZERO;
    let (fwd, right, up) = rot_identity.get_axes();
    assert!((fwd - Vec3::X).length() < 1e-4);
    assert!((right - Vec3::Y).length() < 1e-4);
    assert!((up - Vec3::Z).length() < 1e-4);

    // Turn 90 degrees Yaw to the right (around +Z)
    let rot_yaw_90 = Rotator::from_degrees(0.0, 90.0, 0.0);
    let (fwd_90, right_90, up_90) = rot_yaw_90.get_axes();
    // After turning +90 deg yaw in UE2 (+X forward rotated toward +Y right):
    assert!((fwd_90 - Vec3::Y).length() < 1e-4);
    assert!((right_90 - (-Vec3::X)).length() < 1e-4);
    assert!((up_90 - Vec3::Z).length() < 1e-4);
}

#[test]
fn test_transform_matrix_and_direction_vectors() {
    let pos = Vec3::new(100.0, 200.0, 300.0);
    let rot = Rotator::from_degrees(0.0, 180.0, 0.0);
    let transform = Transform::from_pos_rotator(pos, rot);

    assert_eq!(transform.position, pos);
    let fwd = transform.forward();
    assert!((fwd - (-Vec3::X)).length() < 1e-4);

    let mat = transform.to_matrix();
    let transformed_origin = mat.transform_point3(Vec3::ZERO);
    assert!((transformed_origin - pos).length() < 1e-4);
}

#[test]
fn test_velocity_euler_integration() {
    let mut transform = Transform::from_position(Vec3::new(0.0, 0.0, 100.0));
    let velocity = Velocity::from_linear(Vec3::new(300.0, 0.0, 0.0));

    assert_eq!(velocity.speed(), 300.0);
    assert_eq!(velocity.speed_2d(), 300.0);

    velocity.integrate(&mut transform, 0.5);
    assert_eq!(transform.position, Vec3::new(150.0, 0.0, 100.0));
}
