//! Single-body Karma ragdoll for corpses without a death animation.
//!
//! Translation integrates through [`KarmaPhysicsBody`] (gravity, damping);
//! the tip-over uses a 1-D tilt (gravity torque, ground stop at 90 degrees)
//! plus a decaying yaw spin. Orientation is exported as UE pitch/yaw/roll.

use glam::Vec3;
use openkf_physics::karma::KarmaPhysicsBody;

/// UE gravity (UU/s^2, -Z).
const GRAVITY: f32 = -950.0;

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
        Self {
            body,
            tilt: 0.06,
            tilt_vel: 2.2 + ((seed >> 3) % 100) as f32 / 100.0,
            tilt_dir,
            yaw_off: 0.0,
            yaw_spin: (((seed >> 5) % 200) as f32 / 100.0 - 1.0) * 3.0,
            ground: loc[2] - collision_height * 0.55,
            base_yaw: yaw_ue,
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

    pub fn settled(&self) -> bool {
        !self.body.is_awake && self.tilt_vel == 0.0 && self.tilt >= std::f32::consts::FRAC_PI_2 - 0.01
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
}
