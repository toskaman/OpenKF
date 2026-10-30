//! Articulated Karma ragdoll for corpses without a death animation.
//!
//! The root integrates through [`KarmaPhysicsBody`] (gravity, damping) with a
//! 1-D tip-over (gravity torque, ground stop at 90 degrees) plus a decaying
//! yaw spin. Every bone is an independent 3-DOF damped pendulum: kicked by
//! the killing blow, sprung back toward the reference pose, clamped to a
//! joint limit. Orientation is exported as UE pitch/yaw/roll plus per-bone
//! local-rotation deltas (identity = reference pose).

use glam::{Quat, Vec3};
use openkf_physics::karma::KarmaPhysicsBody;

/// UE gravity (UU/s^2, -Z).
const GRAVITY: f32 = -950.0;
/// Simulated bone channels (skeleton-agnostic; bones past this keep ref pose).
pub const ARTIC_BONES: usize = 64;
/// Joint limit (radians from the reference orientation).
const JOINT_LIMIT: f32 = 1.1;
/// Spring toward reference (1/s^2) and angular damping (1/s).
const SPRING_K: f32 = 26.0;
const DAMPING_C: f32 = 4.5;

#[derive(Debug, Clone)]
pub struct RagdollState {
    body: KarmaPhysicsBody,
    /// Tilt from upright (radians, 0..PI/2).
    tilt: f32,
    tilt_vel: f32,
    /// World yaw (radians) of the fall direction.
    tilt_dir: f32,
    /// Extra yaw from spin (radians).
    yaw_off: f32,
    yaw_spin: f32,
    ground: f32,
    base_yaw: i32,
    /// Per-bone local-rotation delta from the reference pose.
    bones: [Quat; ARTIC_BONES],
    /// Per-bone angular velocity (rad/s, bone local space).
    bone_vel: [Vec3; ARTIC_BONES],
}

fn hash01(mut x: u64) -> f32 {
    x ^= x >> 29;
    x = x.wrapping_mul(0x9fb21c651e98df25);
    x ^= x >> 32;
    (x % 10000) as f32 / 10000.0
}

impl RagdollState {
    /// `loc` = death location (UE), `yaw_ue` = death yaw, `seed` varies the fall.
    pub fn spawn(loc: [f32; 3], yaw_ue: i32, collision_height: f32, seed: u64) -> Self {
        let mut body = KarmaPhysicsBody::new(80.0);
        body.state.position = Vec3::from(loc);
        // Random shove + pop-up from the killing blow.
        let a = (seed % 628) as f32 / 100.0;
        body.state.linear_velocity = Vec3::new(a.cos() * 140.0, a.sin() * 140.0, 170.0);
        body.linear_damping = 0.4;
        body.angular_damping = 1.2;
        let tilt_dir = (seed.wrapping_mul(7919) % 628) as f32 / 100.0;
        // Limbs are kicked by the killing blow (root stays rigid, it carries
        // the whole-body tip-over instead).
        let mut bone_vel = [Vec3::ZERO; ARTIC_BONES];
        for (i, w) in bone_vel.iter_mut().enumerate().skip(1) {
            let s = seed.wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15));
            *w = Vec3::new(hash01(s) - 0.5, hash01(s ^ 0x1234) - 0.5, hash01(s ^ 0x5678) - 0.5) * 9.0;
        }
        Self {
            body,
            tilt: 0.06,
            tilt_vel: 2.2 + ((seed >> 3) % 100) as f32 / 100.0,
            tilt_dir,
            yaw_off: 0.0,
            yaw_spin: (((seed >> 5) % 200) as f32 / 100.0 - 1.0) * 3.0,
            ground: loc[2] - collision_height * 0.55,
            base_yaw: yaw_ue,
            bones: [Quat::IDENTITY; ARTIC_BONES],
            bone_vel,
        }
    }

    pub fn step(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        // Translation: real Karma integration, then a ground plane stop.
        self.body.step(dt, Vec3::new(0.0, 0.0, GRAVITY));
        let p = &mut self.body.state.position;
        let v = &mut self.body.state.linear_velocity;
        if p.z < self.ground {
            p.z = self.ground;
            if v.z < 0.0 {
                v.z = -v.z * 0.22;
                v.x *= 0.65;
                v.y *= 0.65;
                self.tilt_vel *= 0.7;
                self.yaw_spin *= 0.6;
                if v.z < 25.0 {
                    v.z = 0.0;
                    self.body.is_awake = v.x.abs() + v.y.abs() > 12.0 || self.tilt < 1.5;
                }
            }
        }
        // Tip-over: inverted-pendulum torque, hard stop lying flat.
        self.tilt_vel += 6.5 * self.tilt.sin() * dt;
        self.tilt_vel *= (1.0 - 0.6 * dt).max(0.0);
        self.tilt += self.tilt_vel * dt;
        if self.tilt >= std::f32::consts::FRAC_PI_2 {
            self.tilt = std::f32::consts::FRAC_PI_2;
            self.tilt_vel = -self.tilt_vel * 0.2;
            if self.tilt_vel.abs() < 0.15 {
                self.tilt_vel = 0.0;
            }
        }
        self.yaw_off += self.yaw_spin * dt;
        self.yaw_spin *= (1.0 - 2.2 * dt).max(0.0);
        // Articulation: damped pendulum per bone toward the reference pose.
        for i in 1..ARTIC_BONES {
            let q = self.bones[i];
            // Spring torque ~ -k * rotation vector (imaginary part direction).
            let axis = Vec3::new(q.x, q.y, q.z);
            let w = &mut self.bone_vel[i];
            *w += (-SPRING_K * axis - DAMPING_C * *w) * dt;
            // Integrate rotation: q += 0.5 * (w,0) * q * dt.
            let wq = Quat::from_xyzw(w.x, w.y, w.z, 0.0);
            let nq = (q + (wq * q) * (0.5 * dt)).normalize();
            // Joint limit: clamp the angle from identity.
            let ang = 2.0 * nq.w.clamp(-1.0, 1.0).acos();
            self.bones[i] = if ang > JOINT_LIMIT { Quat::IDENTITY.slerp(nq, JOINT_LIMIT / ang) } else { nq };
        }
    }

    /// Physics-driven `(location, yaw, pitch, roll)` from the death yaw.
    pub fn pose(&self) -> ([f32; 3], i32, i32, i32) {
        const K: f32 = 65536.0 / std::f32::consts::TAU;
        let yaw = self.base_yaw as f32 / K + self.yaw_off;
        // Tilt decomposed along / across the facing: pitch vs roll.
        let diff = self.tilt_dir - yaw;
        let pitch = self.tilt * diff.cos();
        let roll = self.tilt * diff.sin();
        let p = self.body.state.position;
        ([p.x, p.y, p.z], (yaw * K) as i32, (pitch * K) as i32, (roll * K) as i32)
    }

    /// Per-bone local-rotation deltas (mesh bone order, identity = ref pose).
    pub fn bones(&self) -> Vec<[f32; 4]> {
        self.bones.iter().map(|q| q.to_array()).collect()
    }

    /// Largest bone angle from the reference pose (radians).
    pub fn max_bone_angle(&self) -> f32 {
        self.bones[1..].iter().map(|q| 2.0 * q.w.clamp(-1.0, 1.0).acos()).fold(0.0, f32::max)
    }

    pub fn settled(&self) -> bool {
        !self.body.is_awake && self.tilt_vel == 0.0 && self.tilt >= std::f32::consts::FRAC_PI_2 - 0.01 && self.max_bone_angle() < 0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_and_settles_flat() {
        let mut r = RagdollState::spawn([0.0, 0.0, 100.0], 0, 44.0, 12345);
        for _ in 0..200 {
            r.step(1.0 / 60.0);
        }
        let (loc, _, _, _) = r.pose();
        assert!(loc[2] <= 100.0 && loc[2] >= 100.0 - 44.0 * 0.55 - 1.0, "resting height {loc:?}");
        assert!((r.tilt - std::f32::consts::FRAC_PI_2).abs() < 0.05, "lying flat, tilt={}", r.tilt);
        assert!(r.settled());
    }

    #[test]
    fn seed_varies_fall_direction() {
        let a = RagdollState::spawn([0.0; 3], 0, 44.0, 1);
        let b = RagdollState::spawn([0.0; 3], 0, 44.0, 2);
        assert!((a.tilt_dir - b.tilt_dir).abs() > 0.01);
    }

    #[test]
    fn limbs_flail_then_settle() {
        let mut r = RagdollState::spawn([0.0; 3], 0, 44.0, 7);
        for _ in 0..10 {
            r.step(1.0 / 60.0);
        }
        // Kicked limbs move away from the reference pose...
        assert!(r.max_bone_angle() > 0.05, "bones should flail, angle={}", r.max_bone_angle());
        for _ in 0..400 {
            r.step(1.0 / 60.0);
        }
        // ...then the spring pulls them back inside the joint limit.
        assert!(r.max_bone_angle() < 0.05, "bones should settle, angle={}", r.max_bone_angle());
        assert!(r.settled());
        assert_eq!(r.bones().len(), super::ARTIC_BONES);
    }

    #[test]
    fn joint_limit_holds() {
        let mut r = RagdollState::spawn([0.0; 3], 0, 44.0, 99);
        for _ in 0..120 {
            r.step(1.0 / 60.0);
            assert!(r.max_bone_angle() <= super::JOINT_LIMIT + 1e-3, "angle={}", r.max_bone_angle());
        }
    }
}
