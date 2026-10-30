//! Discrete Oriented Polytope (k-DOP) collision bounding volumes (kDOP-18 and kDOP-26).
//!
//! In Unreal Engine 2.5, static meshes utilize kDOP bounding trees for fast collision rejection.
//! k-DOPs are convex polytopes formed by intersecting pairs of parallel slabs oriented
//! along fixed normal directions.

use glam::Vec3;
use crate::primitives::{Ray, RayHit};

const INV_SQRT2: f32 = 0.707_106_77; // 1 / sqrt(2)
const INV_SQRT3: f32 = 0.577_350_26; // 1 / sqrt(3)

/// The 9 fixed unit normal vectors defining the slab pairs for kDOP-18.
pub const KDOP18_DIRECTIONS: [Vec3; 9] = [
    // 3 Axis-aligned plane pairs
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
    // 6 Edge-diagonal plane pairs
    Vec3::new(INV_SQRT2, INV_SQRT2, 0.0),
    Vec3::new(INV_SQRT2, -INV_SQRT2, 0.0),
    Vec3::new(INV_SQRT2, 0.0, INV_SQRT2),
    Vec3::new(INV_SQRT2, 0.0, -INV_SQRT2),
    Vec3::new(0.0, INV_SQRT2, INV_SQRT2),
    Vec3::new(0.0, INV_SQRT2, -INV_SQRT2),
];

/// The 13 fixed unit normal vectors defining the slab pairs for kDOP-26 (9 of kDOP-18 + 4 corner pairs).
pub const KDOP26_DIRECTIONS: [Vec3; 13] = [
    // 3 Axis-aligned plane pairs
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
    // 6 Edge-diagonal plane pairs
    Vec3::new(INV_SQRT2, INV_SQRT2, 0.0),
    Vec3::new(INV_SQRT2, -INV_SQRT2, 0.0),
    Vec3::new(INV_SQRT2, 0.0, INV_SQRT2),
    Vec3::new(INV_SQRT2, 0.0, -INV_SQRT2),
    Vec3::new(0.0, INV_SQRT2, INV_SQRT2),
    Vec3::new(0.0, INV_SQRT2, -INV_SQRT2),
    // 4 Corner-diagonal plane pairs
    Vec3::new(INV_SQRT3, INV_SQRT3, INV_SQRT3),
    Vec3::new(INV_SQRT3, INV_SQRT3, -INV_SQRT3),
    Vec3::new(INV_SQRT3, -INV_SQRT3, INV_SQRT3),
    Vec3::new(INV_SQRT3, -INV_SQRT3, -INV_SQRT3),
];

/// kDOP-18 bounding volume composed of 9 parallel slab pairs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KDop18 {
    pub min: [f32; 9],
    pub max: [f32; 9],
}

impl KDop18 {
    pub const fn new(min: [f32; 9], max: [f32; 9]) -> Self {
        Self { min, max }
    }

    /// Construct a tight kDOP-18 hull bounding a slice of 3D vertices.
    pub fn from_vertices(vertices: &[Vec3]) -> Self {
        let mut min = [f32::INFINITY; 9];
        let mut max = [f32::NEG_INFINITY; 9];

        for &v in vertices {
            for i in 0..9 {
                let proj = KDOP18_DIRECTIONS[i].dot(v);
                if proj < min[i] { min[i] = proj; }
                if proj > max[i] { max[i] = proj; }
            }
        }

        Self { min, max }
    }

    /// Point containment test.
    pub fn contains_point(&self, p: Vec3) -> bool {
        for i in 0..9 {
            let proj = KDOP18_DIRECTIONS[i].dot(p);
            if proj < self.min[i] - 1e-5 || proj > self.max[i] + 1e-5 {
                return false;
            }
        }
        true
    }

    /// Fast Ray vs kDOP-18 intersection via extended Kay-Kajiya slab method.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let mut t_enter = 0.0f32;
        let mut t_exit = ray.length;
        let mut enter_normal = -ray.direction;

        if self.contains_point(ray.origin) {
            return Some(RayHit {
                time: 0.0,
                distance: 0.0,
                location: ray.origin,
                normal: -ray.direction,
                item: -1,
            });
        }

        for i in 0..9 {
            let normal = KDOP18_DIRECTIONS[i];
            let d = normal.dot(ray.direction);
            let o = normal.dot(ray.origin);

            if d.abs() < 1e-8 {
                if o < self.min[i] - 1e-5 || o > self.max[i] + 1e-5 {
                    return None;
                }
            } else {
                let mut t1 = (self.min[i] - o) / d;
                let mut t2 = (self.max[i] - o) / d;
                let mut sign = -normal;

                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                    sign = normal;
                }

                if t1 > t_enter {
                    t_enter = t1;
                    enter_normal = sign;
                }
                if t2 < t_exit {
                    t_exit = t2;
                }

                if t_enter > t_exit {
                    return None;
                }
            }
        }

        if t_enter >= 0.0 && t_enter <= ray.length {
            let hit_loc = ray.at_dist(t_enter);
            let time = if ray.length > 0.0 { t_enter / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: t_enter,
                location: hit_loc,
                normal: enter_normal.normalize_or_zero(),
                item: -1,
            })
        } else {
            None
        }
    }
}

/// kDOP-26 bounding volume composed of 13 parallel slab pairs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KDop26 {
    pub min: [f32; 13],
    pub max: [f32; 13],
}

impl KDop26 {
    pub const fn new(min: [f32; 13], max: [f32; 13]) -> Self {
        Self { min, max }
    }

    /// Construct a tight kDOP-26 hull bounding a slice of 3D vertices.
    pub fn from_vertices(vertices: &[Vec3]) -> Self {
        let mut min = [f32::INFINITY; 13];
        let mut max = [f32::NEG_INFINITY; 13];

        for &v in vertices {
            for i in 0..13 {
                let proj = KDOP26_DIRECTIONS[i].dot(v);
                if proj < min[i] { min[i] = proj; }
                if proj > max[i] { max[i] = proj; }
            }
        }

        Self { min, max }
    }

    /// Point containment test.
    pub fn contains_point(&self, p: Vec3) -> bool {
        for i in 0..13 {
            let proj = KDOP26_DIRECTIONS[i].dot(p);
            if proj < self.min[i] - 1e-5 || proj > self.max[i] + 1e-5 {
                return false;
            }
        }
        true
    }

    /// Fast Ray vs kDOP-26 intersection via extended Kay-Kajiya slab method.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let mut t_enter = 0.0f32;
        let mut t_exit = ray.length;
        let mut enter_normal = -ray.direction;

        if self.contains_point(ray.origin) {
            return Some(RayHit {
                time: 0.0,
                distance: 0.0,
                location: ray.origin,
                normal: -ray.direction,
                item: -1,
            });
        }

        for i in 0..13 {
            let normal = KDOP26_DIRECTIONS[i];
            let d = normal.dot(ray.direction);
            let o = normal.dot(ray.origin);

            if d.abs() < 1e-8 {
                if o < self.min[i] - 1e-5 || o > self.max[i] + 1e-5 {
                    return None;
                }
            } else {
                let mut t1 = (self.min[i] - o) / d;
                let mut t2 = (self.max[i] - o) / d;
                let mut sign = -normal;

                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                    sign = normal;
                }

                if t1 > t_enter {
                    t_enter = t1;
                    enter_normal = sign;
                }
                if t2 < t_exit {
                    t_exit = t2;
                }

                if t_enter > t_exit {
                    return None;
                }
            }
        }

        if t_enter >= 0.0 && t_enter <= ray.length {
            let hit_loc = ray.at_dist(t_enter);
            let time = if ray.length > 0.0 { t_enter / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: t_enter,
                location: hit_loc,
                normal: enter_normal.normalize_or_zero(),
                item: -1,
            })
        } else {
            None
        }
    }
}

/// A collision triangle in the kDOP tree structure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KDopCollisionTriangle {
    pub vertices: [Vec3; 3],
    pub normal: Vec3,
    pub material_id: Option<u32>,
}

impl KDopCollisionTriangle {
    pub fn new(v0: Vec3, v1: Vec3, v2: Vec3, material_id: Option<u32>) -> Self {
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let normal = edge1.cross(edge2).normalize_or_zero();
        Self {
            vertices: [v0, v1, v2],
            normal,
            material_id,
        }
    }

    /// Möller–Trumbore ray-triangle intersection algorithm.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let edge1 = self.vertices[1] - self.vertices[0];
        let edge2 = self.vertices[2] - self.vertices[0];
        let h = ray.direction.cross(edge2);
        let a = edge1.dot(h);

        if a.abs() < 1e-8 {
            return None; // Parallel to triangle plane
        }

        let f = 1.0 / a;
        let s = ray.origin - self.vertices[0];
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(edge1);
        let v = f * ray.direction.dot(q);
        if v < 0.0 || (u + v) > 1.0 {
            return None;
        }

        let t = f * edge2.dot(q);
        if t >= 0.0 && t <= ray.length {
            let hit_loc = ray.at_dist(t);
            let normal = if a < 0.0 { -self.normal } else { self.normal };
            let time = if ray.length > 0.0 { t / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: t,
                location: hit_loc,
                normal,
                item: -1,
            })
        } else {
            None
        }
    }
}

/// Node within an Unreal Engine 2.5 `FkdOPTree`.
#[derive(Debug, Clone)]
pub struct KDopNode {
    pub bounding_volume: KDop26,
    pub children: [Option<usize>; 2],
    pub triangle_offset: usize,
    pub num_triangles: usize,
}

/// Complete hierarchical kDOP Tree for complex mesh collision queries.
#[derive(Debug, Clone, Default)]
pub struct KDopTree {
    pub nodes: Vec<KDopNode>,
    pub triangles: Vec<KDopCollisionTriangle>,
    pub root_index: Option<usize>,
}

impl KDopTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Traverse the kDOP tree to find the nearest ray intersection.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let root = self.root_index?;
        let mut closest_hit: Option<RayHit> = None;
        let mut stack = vec![root];

        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx];

            // Rejection test against kDOP bounding volume
            if let Some(vol_hit) = node.bounding_volume.intersects_ray(ray) {
                if let Some(ref best) = closest_hit {
                    if vol_hit.distance > best.distance {
                        continue;
                    }
                }

                // If leaf node, test triangles
                if node.num_triangles > 0 {
                    let end_tri = node.triangle_offset + node.num_triangles;
                    for (i, tri) in self.triangles[node.triangle_offset..end_tri].iter().enumerate() {
                        if let Some(mut tri_hit) = tri.intersects_ray(ray) {
                            if closest_hit.as_ref().is_none_or(|b| tri_hit.distance < b.distance) {
                                tri_hit.item = (node.triangle_offset + i) as i32;
                                closest_hit = Some(tri_hit);
                            }
                        }
                    }
                } else {
                    // Push children to stack
                    if let Some(child_b) = node.children[1] { stack.push(child_b); }
                    if let Some(child_a) = node.children[0] { stack.push(child_a); }
                }
            }
        }

        closest_hit
    }
}
