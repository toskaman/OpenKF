//! Collision subsystem world and query engine (`SingleLineCheck`, `MultiLineCheck`, `PointCheck`).
//!
//! Provides the complete Unreal Engine 2.5 `ULevel` collision execution pipeline,
//! unifying BSP geometry, static mesh kDOP trees, and dynamic actor bounding volumes.

use glam::Vec3;
use crate::primitives::{Aabb, BoundingCylinder, Capsule, Ray, RayHit, Sphere};
use crate::kdop::{KDop18, KDop26, KDopTree};
use crate::bsp::BspTree;
use crate::raycast::{FCheckResult, TraceFlags};

/// Geometric collision shape attached to an actor or static entity.
#[derive(Debug, Clone)]
pub enum CollisionShape {
    Cylinder(BoundingCylinder),
    Box(Aabb),
    Sphere(Sphere),
    Capsule(Capsule),
    KDop18(KDop18),
    KDop26(KDop26),
    KDopTree(KDopTree),
}

impl CollisionShape {
    /// Compute the overall axis-aligned bounding box for broadphase rejection.
    pub fn aabb(&self) -> Aabb {
        match self {
            Self::Cylinder(c) => c.aabb(),
            Self::Box(b) => *b,
            Self::Sphere(s) => s.aabb(),
            Self::Capsule(cap) => Aabb::from_center_extents(
                cap.center,
                Vec3::new(cap.radius, cap.radius, cap.half_height + cap.radius),
            ),
            Self::KDop18(k) => Aabb::new(
                Vec3::new(k.min[0], k.min[1], k.min[2]),
                Vec3::new(k.max[0], k.max[1], k.max[2]),
            ),
            Self::KDop26(k) => Aabb::new(
                Vec3::new(k.min[0], k.min[1], k.min[2]),
                Vec3::new(k.max[0], k.max[1], k.max[2]),
            ),
            Self::KDopTree(tree) => {
                if let Some(root_idx) = tree.root_index {
                    let k = &tree.nodes[root_idx].bounding_volume;
                    Aabb::new(
                        Vec3::new(k.min[0], k.min[1], k.min[2]),
                        Vec3::new(k.max[0], k.max[1], k.max[2]),
                    )
                } else {
                    Aabb::new(Vec3::ZERO, Vec3::ZERO)
                }
            }
        }
    }

    /// Perform raycast or swept box check against shape.
    pub fn intersect(&self, ray: &Ray, extent: Vec3) -> Option<RayHit> {
        let is_sweep = extent != Vec3::ZERO;
        match self {
            Self::Cylinder(cyl) => {
                if is_sweep {
                    cyl.swept_box_intersect(ray, extent)
                } else {
                    cyl.intersects_ray(ray)
                }
            }
            Self::Box(box_aabb) => {
                if is_sweep {
                    box_aabb.swept_aabb_intersect(ray, extent)
                } else {
                    box_aabb.intersects_ray(ray)
                }
            }
            Self::Sphere(sph) => {
                if is_sweep {
                    let max_ext = extent.x.abs().max(extent.y.abs()).max(extent.z.abs());
                    let expanded = Sphere::new(sph.center, sph.radius + max_ext);
                    expanded.intersects_ray(ray)
                } else {
                    sph.intersects_ray(ray)
                }
            }
            Self::Capsule(cap) => {
                if is_sweep {
                    let max_ext = extent.x.abs().max(extent.y.abs()).max(extent.z.abs());
                    let expanded = Capsule::new(cap.center, cap.radius + max_ext, cap.half_height);
                    expanded.intersects_ray(ray)
                } else {
                    cap.intersects_ray(ray)
                }
            }
            Self::KDop18(k18) => {
                if is_sweep {
                    let mut min = k18.min;
                    let mut max = k18.max;
                    let ext = extent.length();
                    for i in 0..9 {
                        min[i] -= ext;
                        max[i] += ext;
                    }
                    KDop18::new(min, max).intersects_ray(ray)
                } else {
                    k18.intersects_ray(ray)
                }
            }
            Self::KDop26(k26) => {
                if is_sweep {
                    let mut min = k26.min;
                    let mut max = k26.max;
                    let ext = extent.length();
                    for i in 0..13 {
                        min[i] -= ext;
                        max[i] += ext;
                    }
                    KDop26::new(min, max).intersects_ray(ray)
                } else {
                    k26.intersects_ray(ray)
                }
            }
            Self::KDopTree(tree) => tree.intersects_ray(ray),
        }
    }
}

/// Actor collision descriptor matching UE2.5 AActor flags.
#[derive(Debug, Clone)]
pub struct CollisionActor {
    pub actor_id: u32,
    pub shape: CollisionShape,
    pub classification: TraceFlags,
    pub b_block_actors: bool,
    pub b_block_players: bool,
    pub b_block_zero_extent_traces: bool,
    pub b_block_non_zero_extent_traces: bool,
    pub material_id: Option<u32>,
}

impl CollisionActor {
    pub fn new(actor_id: u32, shape: CollisionShape, classification: TraceFlags) -> Self {
        Self {
            actor_id,
            shape,
            classification,
            b_block_actors: true,
            b_block_players: true,
            b_block_zero_extent_traces: true,
            b_block_non_zero_extent_traces: true,
            material_id: None,
        }
    }
}

/// Unified collision environment managing actors, BSP geometry, and spatial queries.
#[derive(Debug, Clone, Default)]
pub struct CollisionWorld {
    pub actors: Vec<CollisionActor>,
    pub bsp_tree: Option<BspTree>,
}

impl CollisionWorld {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an actor to the collision world.
    pub fn add_actor(&mut self, actor: CollisionActor) {
        self.actors.push(actor);
    }

    /// Set BSP level architecture geometry.
    pub fn set_bsp(&mut self, bsp: BspTree) {
        self.bsp_tree = Some(bsp);
    }

    /// `SingleLineCheck`: Fast ray or swept-box trace returning the nearest blocking hit.
    ///
    /// Mirrors UE2.5 `ULevel::SingleLineCheck`.
    pub fn single_line_check(
        &self,
        start: Vec3,
        end: Vec3,
        flags: TraceFlags,
        extent: Vec3,
        ignore_actor: Option<u32>,
    ) -> Option<FCheckResult> {
        let hits = self.trace_internal(start, end, flags, extent, ignore_actor, true);
        hits.into_iter().next()
    }

    /// `MultiLineCheck`: Multi-hit ray or swept-box trace returning all encountered hits sorted nearest-to-farthest.
    ///
    /// Mirrors UE2.5 `ULevel::MultiLineCheck`.
    pub fn multi_line_check(
        &self,
        start: Vec3,
        end: Vec3,
        flags: TraceFlags,
        extent: Vec3,
        ignore_actor: Option<u32>,
    ) -> Vec<FCheckResult> {
        self.trace_internal(start, end, flags, extent, ignore_actor, false)
    }

    /// `PointCheck`: Tests whether a point/extent overlaps with any blocking geometry.
    ///
    /// Mirrors UE2.5 `ULevel::PointCheck`.
    pub fn point_check(&self, point: Vec3, extent: Vec3, flags: TraceFlags) -> Option<FCheckResult> {
        // Point check is equivalent to a zero-length sweep trace at `point`
        let ray = Ray::new(point, Vec3::X, 0.0);
        for actor in &self.actors {
            if !actor.classification.intersects(flags) {
                continue;
            }
            if let Some(hit) = actor.shape.intersect(&ray, extent) {
                return Some(FCheckResult::new(
                    hit.location,
                    hit.normal,
                    0.0,
                    Some(actor.actor_id),
                    None,
                    hit.item,
                ));
            }
        }
        None
    }

    fn trace_internal(
        &self,
        start: Vec3,
        end: Vec3,
        flags: TraceFlags,
        extent: Vec3,
        ignore_actor: Option<u32>,
        single_result: bool,
    ) -> Vec<FCheckResult> {
        let ray = Ray::from_endpoints(start, end);
        let is_zero_extent = extent == Vec3::ZERO;
        let mut results = Vec::new();

        // 1. Trace BSP level architecture if requested
        if flags.intersects(TraceFlags::LEVEL | TraceFlags::WORLD) {
            if let Some(ref bsp) = self.bsp_tree {
                if let Some(hit) = bsp.raycast(&ray) {
                    results.push(FCheckResult {
                        location: hit.location,
                        normal: hit.normal,
                        time: hit.time,
                        actor_id: None,
                        primitive_id: None,
                        item: hit.item,
                        material_id: None,
                    });
                    if single_result && flags.contains(TraceFlags::STOP_AT_FIRST_HIT) {
                        return results;
                    }
                }
            }
        }

        // 2. Trace dynamic actors and static meshes
        for actor in &self.actors {
            if Some(actor.actor_id) == ignore_actor {
                continue;
            }
            if !actor.classification.intersects(flags) {
                continue;
            }
            if is_zero_extent && !actor.b_block_zero_extent_traces {
                continue;
            }
            if !is_zero_extent && !actor.b_block_non_zero_extent_traces {
                continue;
            }

            // Broadphase AABB rejection
            let broad_aabb = actor.shape.aabb();
            let expanded_broad = if is_zero_extent {
                broad_aabb
            } else {
                Aabb::from_center_extents(broad_aabb.center(), broad_aabb.extents() + extent.abs())
            };

            if expanded_broad.intersects_ray(&ray).is_none() {
                continue;
            }

            // Narrowphase primitive intersection
            if let Some(hit) = actor.shape.intersect(&ray, extent) {
                results.push(FCheckResult {
                    location: hit.location,
                    normal: hit.normal,
                    time: hit.time,
                    actor_id: Some(actor.actor_id),
                    primitive_id: None,
                    item: hit.item,
                    material_id: actor.material_id,
                });

                if single_result && flags.contains(TraceFlags::STOP_AT_FIRST_HIT) {
                    return results;
                }
            }
        }

        // Sort hits by normalized time (nearest first)
        results.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

        if single_result && !results.is_empty() {
            results.truncate(1);
        }

        results
    }
}
