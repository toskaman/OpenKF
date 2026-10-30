//! Binary Space Partitioning (BSP) collision tree and hyperplane structures.
//!
//! In Unreal Engine 2.5, static level architecture (`UModel`) is stored as a BSP tree.
//! Tracing bullets or actor sweeps against BSP geometry traverses front-to-back,
//! clipping the ray segment against splitting planes and testing solid coplanar surfaces.

use glam::Vec3;
use crate::primitives::{Plane, Ray, RayHit};
use crate::kdop::KDopCollisionTriangle;

/// Flags defining surface properties in Unreal Engine BSP.
pub mod surf_flags {
    pub const PF_BLOCKING: u32 = 0x00000001;
    pub const PF_INVISIBLE: u32 = 0x00000002;
    pub const PF_TWO_SIDED: u32 = 0x00000100;
    pub const PF_PORTAL: u32 = 0x04000000;
}

/// A node within the BSP tree.
#[derive(Debug, Clone)]
pub struct BspNode {
    pub plane: Plane,
    pub front_child: Option<usize>,
    pub back_child: Option<usize>,
    pub triangles: Vec<KDopCollisionTriangle>,
    pub is_leaf: bool,
    pub is_solid: bool,
    pub surface_flags: u32,
    pub material_id: Option<u32>,
}

impl BspNode {
    pub fn new_leaf(is_solid: bool, surface_flags: u32) -> Self {
        Self {
            plane: Plane::new(Vec3::Z, 0.0),
            front_child: None,
            back_child: None,
            triangles: Vec::new(),
            is_leaf: true,
            is_solid,
            surface_flags,
            material_id: None,
        }
    }

    pub fn new_split(plane: Plane, front: Option<usize>, back: Option<usize>) -> Self {
        Self {
            plane,
            front_child: front,
            back_child: back,
            triangles: Vec::new(),
            is_leaf: false,
            is_solid: false,
            surface_flags: surf_flags::PF_BLOCKING,
            material_id: None,
        }
    }
}

/// BSP Tree managing level geometry collision.
#[derive(Debug, Clone, Default)]
pub struct BspTree {
    pub nodes: Vec<BspNode>,
    pub root_index: Option<usize>,
}

impl BspTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the BSP tree and return its index.
    pub fn add_node(&mut self, node: BspNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        if self.root_index.is_none() {
            self.root_index = Some(idx);
        }
        idx
    }

    /// Trace a ray against the BSP tree using front-to-back segment clipping.
    pub fn raycast(&self, ray: &Ray) -> Option<RayHit> {
        let root = self.root_index?;
        self.clip_segment(root, ray, 0.0, ray.length)
    }

    fn clip_segment(&self, node_idx: usize, ray: &Ray, t0: f32, t1: f32) -> Option<RayHit> {
        if t0 > t1 {
            return None;
        }

        let node = &self.nodes[node_idx];

        if node.is_leaf {
            if node.is_solid {
                // Hit solid leaf
                let p = ray.at_dist(t0);
                let time = if ray.length > 0.0 { t0 / ray.length } else { 0.0 };
                return Some(RayHit {
                    time,
                    distance: t0,
                    location: p,
                    normal: -ray.direction,
                    item: node_idx as i32,
                });
            }
            return None;
        }

        let p0 = ray.at_dist(t0);
        let p1 = ray.at_dist(t1);

        let d0 = node.plane.distance_to_point(p0);
        let d1 = node.plane.distance_to_point(p1);

        const EPSILON: f32 = 1e-4;

        if d0 >= -EPSILON && d1 >= -EPSILON {
            // Entirely in front
            if let Some(front) = node.front_child {
                return self.clip_segment(front, ray, t0, t1);
            }
        } else if d0 <= EPSILON && d1 <= EPSILON {
            // Entirely in back
            if let Some(back) = node.back_child {
                return self.clip_segment(back, ray, t0, t1);
            }
        } else {
            // Segment crosses splitting plane: calculate intersection t_mid
            let frac = d0 / (d0 - d1);
            let t_mid = t0 + (t1 - t0) * frac.clamp(0.0, 1.0);

            let (first_child, second_child, hit_normal) = if d0 > 0.0 {
                (node.front_child, node.back_child, node.plane.normal)
            } else {
                (node.back_child, node.front_child, -node.plane.normal)
            };

            // 1. Traverse near side first
            if let Some(first) = first_child {
                if let Some(hit) = self.clip_segment(first, ray, t0, t_mid) {
                    return Some(hit);
                }
            }

            // 2. Check coplanar triangles if present on this splitting node
            for (i, tri) in node.triangles.iter().enumerate() {
                if let Some(mut hit) = tri.intersects_ray(ray) {
                    if hit.distance >= t0 && hit.distance <= t1 {
                        hit.item = i as i32;
                        return Some(hit);
                    }
                }
            }

            // If blocking surface without triangles, plane hit is registered
            if (node.surface_flags & surf_flags::PF_BLOCKING) != 0 && t_mid >= 0.0 && t_mid <= ray.length {
                // If it's a solid blocking boundary
                if node.back_child.is_some_and(|b| self.nodes[b].is_solid) {
                    let hit_loc = ray.at_dist(t_mid);
                    let time = if ray.length > 0.0 { t_mid / ray.length } else { 0.0 };
                    return Some(RayHit {
                        time,
                        distance: t_mid,
                        location: hit_loc,
                        normal: hit_normal,
                        item: node_idx as i32,
                    });
                }
            }

            // 3. Traverse far side
            if let Some(second) = second_child {
                if let Some(hit) = self.clip_segment(second, ray, t_mid, t1) {
                    return Some(hit);
                }
            }
        }

        None
    }
}
