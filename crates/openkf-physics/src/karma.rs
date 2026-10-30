//! MathEngine Karma physics asset structures and native runtime bindings.
//!
//! Unreal Engine 2.5 uses Karma for ragdoll and rigid body dynamics configured via
//! KAT `.ka` XML definitions and exposed through 42 native UnrealScript functions on `AActor`.

use glam::{Quat, Vec3};

/// Primitive shape definition in Karma `.ka` authoring assets.
#[derive(Debug, Clone, PartialEq)]
pub enum KarmaPrimitiveShape {
    Sphere {
        radius: f32,
    },
    Box {
        extents: Vec3, // Half extents (X, Y, Z)
    },
    Sphyl {
        radius: f32,
        height: f32, // Length between cylinder centers
    },
    Convex {
        vertices: Vec<Vec3>,
    },
}

/// A physical collision body within a Karma ragdoll or prop.
#[derive(Debug, Clone, PartialEq)]
pub struct KarmaCollisionElement {
    pub shape: KarmaPrimitiveShape,
    pub local_pos: Vec3,
    pub local_rot: Quat,
    pub bone_name: Option<String>,
}

/// Dynamic rigid body state corresponding to `KRigidBodyState` in `Engine.dll`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KRigidBodyState {
    pub position: Vec3,
    pub quaternion: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

impl Default for KRigidBodyState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            quaternion: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }
}

/// Upward buoyancy lifter applied to a ragdoll bone.
#[derive(Debug, Clone, PartialEq)]
pub struct BoneLifter {
    pub bone_name: String,
    pub lift_force: f32,
    pub damping: f32,
}

/// Actor-level Karma physics controller encapsulating the 42 native Karma operations.
#[derive(Debug, Clone, PartialEq)]
pub struct KarmaPhysicsBody {
    pub state: KRigidBodyState,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub b_stay_upright: bool,
    pub stay_upright_stiffness: f32,
    pub stay_upright_damping: f32,
    pub is_awake: bool,
    pub block_karma: bool,
    pub com_offset: Vec3,
    pub is_ragdoll_frozen: bool,
    pub bone_lifters: Vec<BoneLifter>,
    pub ignored_actors: Vec<u32>,
}

impl Default for KarmaPhysicsBody {
    fn default() -> Self {
        Self {
            state: KRigidBodyState::default(),
            mass: 80.0, // Standard 80kg pawn/human
            friction: 0.8,
            restitution: 0.1,
            linear_damping: 0.1,
            angular_damping: 0.1,
            gravity_scale: 1.0,
            b_stay_upright: false,
            stay_upright_stiffness: 50.0,
            stay_upright_damping: 5.0,
            is_awake: true,
            block_karma: true,
            com_offset: Vec3::ZERO,
            is_ragdoll_frozen: false,
            bone_lifters: Vec::new(),
            ignored_actors: Vec::new(),
        }
    }
}

impl KarmaPhysicsBody {
    pub fn new(mass: f32) -> Self {
        Self {
            mass,
            ..Default::default()
        }
    }

    /// `KAddImpulse(Vector Impulse, optional Vector Position)`
    pub fn k_add_impulse(&mut self, impulse: Vec3, position: Option<Vec3>) {
        if self.is_ragdoll_frozen || self.mass <= 1e-4 {
            return;
        }
        self.k_wake();
        self.state.linear_velocity += impulse / self.mass;

        if let Some(pos) = position {
            let com = self.k_get_com_position();
            let r = pos - com;
            let torque = r.cross(impulse);
            // Approximate inertia tensor with sphere of radius 0.5m
            let inertia = 0.4 * self.mass * 0.25;
            self.state.angular_velocity += torque / inertia;
        }
    }

    /// `KAddAngularImpulse(Vector AngImpulse)`
    pub fn k_add_angular_impulse(&mut self, ang_impulse: Vec3) {
        if self.is_ragdoll_frozen || self.mass <= 1e-4 {
            return;
        }
        self.k_wake();
        let inertia = 0.4 * self.mass * 0.25;
        self.state.angular_velocity += ang_impulse / inertia;
    }

    /// `KWake()`
    pub fn k_wake(&mut self) {
        self.is_awake = true;
    }

    /// `KIsAwake() -> bool`
    pub fn k_is_awake(&self) -> bool {
        self.is_awake
    }

    /// `KFreezeRagdoll()`
    pub fn k_freeze_ragdoll(&mut self) {
        self.is_ragdoll_frozen = true;
        self.state.linear_velocity = Vec3::ZERO;
        self.state.angular_velocity = Vec3::ZERO;
        self.is_awake = false;
    }

    /// `KSetActorGravScale(float GravScale)`
    pub fn k_set_actor_grav_scale(&mut self, scale: f32) {
        self.gravity_scale = scale;
    }

    /// `KGetActorGravScale() -> float`
    pub fn k_get_actor_grav_scale(&self) -> f32 {
        self.gravity_scale
    }

    /// `KSetBlockKarma(bool bBlock)`
    pub fn k_set_block_karma(&mut self, block: bool) {
        self.block_karma = block;
    }

    /// `KDisableCollision(Actor Other)`
    pub fn k_disable_collision(&mut self, other_id: u32) {
        if !self.ignored_actors.contains(&other_id) {
            self.ignored_actors.push(other_id);
        }
    }

    /// `KEnableCollision(Actor Other)`
    pub fn k_enable_collision(&mut self, other_id: u32) {
        self.ignored_actors.retain(|&id| id != other_id);
    }

    /// `KSetMass(float Mass)`
    pub fn k_set_mass(&mut self, mass: f32) {
        self.mass = mass.max(0.001);
    }

    /// `KGetMass() -> float`
    pub fn k_get_mass(&self) -> f32 {
        self.mass
    }

    /// `KSetFriction(float Friction)`
    pub fn k_set_friction(&mut self, friction: f32) {
        self.friction = friction.max(0.0);
    }

    /// `KSetRestitution(float Restitution)`
    pub fn k_set_restitution(&mut self, restitution: f32) {
        self.restitution = restitution.clamp(0.0, 1.0);
    }

    /// `KSetDampingProps(float LinDamp, float AngDamp)`
    pub fn k_set_damping_props(&mut self, lin_damp: f32, ang_damp: f32) {
        self.linear_damping = lin_damp.max(0.0);
        self.angular_damping = ang_damp.max(0.0);
    }

    /// `KSetStayUpright(bool bStayUpright)`
    pub fn k_set_stay_upright(&mut self, stay_upright: bool) {
        self.b_stay_upright = stay_upright;
    }

    /// `KSetStayUprightParams(float Stiffness, float Damping)`
    pub fn k_set_stay_upright_params(&mut self, stiffness: f32, damping: f32) {
        self.stay_upright_stiffness = stiffness;
        self.stay_upright_damping = damping;
    }

    /// `KAddBoneLifter(Name BoneName, ...)`
    pub fn k_add_bone_lifter(&mut self, bone_name: String, lift_force: f32, damping: f32) {
        self.bone_lifters.retain(|b| b.bone_name != bone_name);
        self.bone_lifters.push(BoneLifter {
            bone_name,
            lift_force,
            damping,
        });
    }

    /// `KRemoveBoneLifter(Name BoneName)`
    pub fn k_remove_bone_lifter(&mut self, bone_name: &str) {
        self.bone_lifters.retain(|b| b.bone_name != bone_name);
    }

    /// `KRemoveAllBoneLifters()`
    pub fn k_remove_all_bone_lifters(&mut self) {
        self.bone_lifters.clear();
    }

    /// `KGetCOMPosition() -> Vector`
    pub fn k_get_com_position(&self) -> Vec3 {
        self.state.position + self.state.quaternion.mul_vec3(self.com_offset)
    }

    /// `KGetCOMOffset() -> Vector`
    pub fn k_get_com_offset(&self) -> Vec3 {
        self.com_offset
    }

    /// `KSetCOMOffset(Vector Offset)`
    pub fn k_set_com_offset(&mut self, offset: Vec3) {
        self.com_offset = offset;
    }

    /// `KGetRigidBodyState(out KRigidBodyState RBState)`
    pub fn k_get_rigid_body_state(&self) -> KRigidBodyState {
        self.state
    }

    /// Set dynamic state
    pub fn k_set_rigid_body_state(&mut self, state: KRigidBodyState) {
        self.state = state;
    }

    /// `KRBVecFromVector(Vector InVec) -> Vector`
    /// Coordinate transformation from Unreal coordinates to Karma local frame.
    pub fn krb_vec_from_vector(&self, in_vec: Vec3) -> Vec3 {
        self.state.quaternion.inverse().mul_vec3(in_vec - self.state.position)
    }

    /// `KRBVecToVector(Vector InRBVec) -> Vector`
    /// Coordinate transformation from Karma local frame to Unreal world coordinates.
    pub fn krb_vec_to_vector(&self, in_rb_vec: Vec3) -> Vec3 {
        self.state.position + self.state.quaternion.mul_vec3(in_rb_vec)
    }

    /// Symplectic Euler numerical integration step.
    pub fn step(&mut self, dt: f32, gravity: Vec3) {
        if !self.is_awake || self.is_ragdoll_frozen {
            return;
        }

        // Apply gravity
        let total_accel = gravity * self.gravity_scale;
        self.state.linear_velocity += total_accel * dt;

        // Apply stay-upright spring torque if active
        if self.b_stay_upright {
            let current_up = self.state.quaternion.mul_vec3(Vec3::Z);
            let target_up = Vec3::Z;
            let torque_axis = current_up.cross(target_up);
            let spring_torque = torque_axis * self.stay_upright_stiffness;
            let damp_torque = -self.state.angular_velocity * self.stay_upright_damping;
            self.state.angular_velocity += (spring_torque + damp_torque) * dt;
        }

        // Apply linear & angular damping
        let lin_drag = (1.0 - self.linear_damping * dt).max(0.0);
        let ang_drag = (1.0 - self.angular_damping * dt).max(0.0);
        self.state.linear_velocity *= lin_drag;
        self.state.angular_velocity *= ang_drag;

        // Position update
        self.state.position += self.state.linear_velocity * dt;

        // Orientation update via angular velocity quaternion derivative
        let omega = self.state.angular_velocity;
        let omega_quat = Quat::from_xyzw(omega.x, omega.y, omega.z, 0.0);
        let q_dot = omega_quat * self.state.quaternion * 0.5;
        self.state.quaternion = (self.state.quaternion + q_dot * dt).normalize();

        // Dormancy check: put to sleep if velocity is negligible
        if self.state.linear_velocity.length_squared() < 1e-4 && self.state.angular_velocity.length_squared() < 1e-4 {
            self.is_awake = false;
        }
    }
}
