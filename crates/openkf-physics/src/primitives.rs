//! Collision geometric primitives: Ray, AABB, Sphere, Cylinder, Capsule, and Plane.

use glam::Vec3;

/// A 3D ray segment defined by start origin, normalized direction, and length.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub length: f32,
    pub inv_direction: Vec3,
}

impl Ray {
    /// Construct a ray from start point and end point.
    pub fn from_endpoints(start: Vec3, end: Vec3) -> Self {
        let delta = end - start;
        let length = delta.length();
        let direction = if length > 1e-8 {
            delta / length
        } else {
            Vec3::X
        };
        let inv_direction = Vec3::new(
            if direction.x.abs() > 1e-8 { 1.0 / direction.x } else { f32::INFINITY },
            if direction.y.abs() > 1e-8 { 1.0 / direction.y } else { f32::INFINITY },
            if direction.z.abs() > 1e-8 { 1.0 / direction.z } else { f32::INFINITY },
        );
        Self {
            origin: start,
            direction,
            length,
            inv_direction,
        }
    }

    /// Construct a ray from origin, normalized direction, and length.
    pub fn new(origin: Vec3, direction: Vec3, length: f32) -> Self {
        let dir = direction.normalize_or_zero();
        let inv_direction = Vec3::new(
            if dir.x.abs() > 1e-8 { 1.0 / dir.x } else { f32::INFINITY },
            if dir.y.abs() > 1e-8 { 1.0 / dir.y } else { f32::INFINITY },
            if dir.z.abs() > 1e-8 { 1.0 / dir.z } else { f32::INFINITY },
        );
        Self {
            origin,
            direction: dir,
            length,
            inv_direction,
        }
    }

    /// Get world point at normalized distance `t` in `[0.0, 1.0]`.
    #[inline]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * (t * self.length)
    }

    /// Get world point at absolute distance `dist`.
    #[inline]
    pub fn at_dist(&self, dist: f32) -> Vec3 {
        self.origin + self.direction * dist
    }
}

/// Result of an intersection between a Ray and a Primitive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// Normalized time along trace segment `[0.0 .. 1.0]`.
    pub time: f32,
    /// Absolute distance in world units from ray origin.
    pub distance: f32,
    /// World coordinates of impact.
    pub location: Vec3,
    /// Outward unit normal at impact.
    pub normal: Vec3,
    /// Optional sub-element index (e.g. triangle, plane, face).
    pub item: i32,
}

/// Axis-Aligned Bounding Box (AABB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_center_extents(center: Vec3, extents: Vec3) -> Self {
        Self {
            min: center - extents.abs(),
            max: center + extents.abs(),
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        p.x >= self.min.x && p.x <= self.max.x &&
        p.y >= self.min.y && p.y <= self.max.y &&
        p.z >= self.min.z && p.z <= self.max.z
    }

    /// Fast Ray vs AABB intersection via Slab Method.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let mut t_min = 0.0f32;
        let mut t_max = ray.length;
        let mut enter_normal = -ray.direction;

        // Check if origin is already inside
        if self.contains_point(ray.origin) {
            return Some(RayHit {
                time: 0.0,
                distance: 0.0,
                location: ray.origin,
                normal: -ray.direction,
                item: -1,
            });
        }

        for axis in 0..3 {
            let (origin_i, dir_i, inv_i, min_i, max_i) = match axis {
                0 => (ray.origin.x, ray.direction.x, ray.inv_direction.x, self.min.x, self.max.x),
                1 => (ray.origin.y, ray.direction.y, ray.inv_direction.y, self.min.y, self.max.y),
                _ => (ray.origin.z, ray.direction.z, ray.inv_direction.z, self.min.z, self.max.z),
            };

            let mut normal_axis = Vec3::ZERO;
            if dir_i.abs() < 1e-8 {
                if origin_i < min_i || origin_i > max_i {
                    return None;
                }
            } else {
                let mut t1 = (min_i - origin_i) * inv_i;
                let mut t2 = (max_i - origin_i) * inv_i;
                let mut sign = -1.0;

                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                    sign = 1.0;
                }

                if t1 > t_min {
                    t_min = t1;
                    match axis {
                        0 => normal_axis.x = sign,
                        1 => normal_axis.y = sign,
                        _ => normal_axis.z = sign,
                    }
                    enter_normal = normal_axis;
                }
                if t2 < t_max {
                    t_max = t2;
                }

                if t_min > t_max {
                    return None;
                }
            }
        }

        if t_min >= 0.0 && t_min <= ray.length {
            let hit_loc = ray.at_dist(t_min);
            let time = if ray.length > 0.0 { t_min / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: t_min,
                location: hit_loc,
                normal: enter_normal.normalize_or_zero(),
                item: -1,
            })
        } else {
            None
        }
    }

    /// Swept box intersection: Sweeping an AABB with extent `extent` against this box.
    /// Equivalent to raycasting against an expanded AABB with `self.extents() + extent`.
    pub fn swept_aabb_intersect(&self, ray: &Ray, extent: Vec3) -> Option<RayHit> {
        if extent == Vec3::ZERO {
            return self.intersects_ray(ray);
        }
        let center = self.center();
        let expanded_extents = self.extents() + extent.abs();
        let expanded_aabb = Aabb::from_center_extents(center, expanded_extents);
        expanded_aabb.intersects_ray(ray)
    }
}

/// 3D Sphere primitive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn aabb(&self) -> Aabb {
        Aabb::from_center_extents(self.center, Vec3::splat(self.radius))
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        (p - self.center).length_squared() <= self.radius * self.radius
    }

    /// Analytical Ray vs Sphere intersection.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let m = ray.origin - self.center;
        let c = m.length_squared() - self.radius * self.radius;

        // Inside sphere
        if c <= 0.0 {
            return Some(RayHit {
                time: 0.0,
                distance: 0.0,
                location: ray.origin,
                normal: -ray.direction,
                item: -1,
            });
        }

        let b = m.dot(ray.direction);
        if b > 0.0 {
            return None; // Heading away from sphere
        }

        let disc = b * b - c;
        if disc < 0.0 {
            return None; // Ray misses sphere
        }

        let t = -b - disc.sqrt();
        if t >= 0.0 && t <= ray.length {
            let hit_loc = ray.at_dist(t);
            let normal = (hit_loc - self.center).normalize_or_zero();
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

/// Unreal Engine 2.5 Axis-Aligned Bounding Cylinder.
///
/// In UE2.5, Pawns (players, specimens) and dynamic actors collide primarily via
/// vertical axis-aligned cylinders along the Z axis:
/// - `radius`: Radial extent in the horizontal XY plane.
/// - `half_height`: CollisionHeight along vertical Z axis (total height = 2 * half_height).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingCylinder {
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
}

impl BoundingCylinder {
    pub fn new(center: Vec3, radius: f32, half_height: f32) -> Self {
        Self {
            center,
            radius,
            half_height,
        }
    }

    pub fn aabb(&self) -> Aabb {
        Aabb::from_center_extents(
            self.center,
            Vec3::new(self.radius, self.radius, self.half_height),
        )
    }

    pub fn contains_point(&self, p: Vec3) -> bool {
        let dz = (p.z - self.center.z).abs();
        if dz > self.half_height {
            return false;
        }
        let dx = p.x - self.center.x;
        let dy = p.y - self.center.y;
        (dx * dx + dy * dy) <= (self.radius * self.radius)
    }

    /// Analytical Ray vs Axis-Aligned Bounding Cylinder intersection.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        // Check if origin is already inside cylinder
        if self.contains_point(ray.origin) {
            return Some(RayHit {
                time: 0.0,
                distance: 0.0,
                location: ray.origin,
                normal: -ray.direction,
                item: -1,
            });
        }

        let mut closest_t = f32::INFINITY;
        let mut hit_normal = Vec3::ZERO;

        // 1. Check side cylinder surface: (x - cx)^2 + (y - cy)^2 = r^2
        let ox = ray.origin.x - self.center.x;
        let oy = ray.origin.y - self.center.y;
        let dx = ray.direction.x;
        let dy = ray.direction.y;

        let a = dx * dx + dy * dy;
        let b = 2.0 * (ox * dx + oy * dy);
        let c = ox * ox + oy * oy - self.radius * self.radius;

        if a > 1e-8 {
            let disc = b * b - 4.0 * a * c;
            if disc >= 0.0 {
                let sqrt_disc = disc.sqrt();
                let t1 = (-b - sqrt_disc) / (2.0 * a);
                let t2 = (-b + sqrt_disc) / (2.0 * a);

                for t in [t1, t2] {
                    if t >= 0.0 && t <= ray.length && t < closest_t {
                        let z = ray.origin.z + t * ray.direction.z;
                        if z >= (self.center.z - self.half_height) && z <= (self.center.z + self.half_height) {
                            closest_t = t;
                            let hit_x = ox + t * dx;
                            let hit_y = oy + t * dy;
                            hit_normal = Vec3::new(hit_x / self.radius, hit_y / self.radius, 0.0).normalize_or_zero();
                        }
                    }
                }
            }
        }

        // 2. Check top cap (z = center.z + half_height)
        if ray.direction.z.abs() > 1e-8 {
            let z_top = self.center.z + self.half_height;
            let t_top = (z_top - ray.origin.z) / ray.direction.z;
            if t_top >= 0.0 && t_top <= ray.length && t_top < closest_t {
                let hx = ray.origin.x + t_top * ray.direction.x - self.center.x;
                let hy = ray.origin.y + t_top * ray.direction.y - self.center.y;
                if hx * hx + hy * hy <= self.radius * self.radius {
                    closest_t = t_top;
                    hit_normal = Vec3::new(0.0, 0.0, 1.0);
                }
            }

            // 3. Check bottom cap (z = center.z - half_height)
            let z_bot = self.center.z - self.half_height;
            let t_bot = (z_bot - ray.origin.z) / ray.direction.z;
            if t_bot >= 0.0 && t_bot <= ray.length && t_bot < closest_t {
                let hx = ray.origin.x + t_bot * ray.direction.x - self.center.x;
                let hy = ray.origin.y + t_bot * ray.direction.y - self.center.y;
                if hx * hx + hy * hy <= self.radius * self.radius {
                    closest_t = t_bot;
                    hit_normal = Vec3::new(0.0, 0.0, -1.0);
                }
            }
        }

        if closest_t <= ray.length {
            let hit_loc = ray.at_dist(closest_t);
            let time = if ray.length > 0.0 { closest_t / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: closest_t,
                location: hit_loc,
                normal: hit_normal,
                item: -1,
            })
        } else {
            None
        }
    }

    /// Swept box intersection against cylinder:
    /// In UE2.5, non-zero trace extent expands pawn cylinder radius and half-height.
    pub fn swept_box_intersect(&self, ray: &Ray, extent: Vec3) -> Option<RayHit> {
        if extent == Vec3::ZERO {
            return self.intersects_ray(ray);
        }
        let horizontal_extent = extent.x.abs().max(extent.y.abs());
        let vertical_extent = extent.z.abs();
        let expanded = BoundingCylinder::new(
            self.center,
            self.radius + horizontal_extent,
            self.half_height + vertical_extent,
        );
        expanded.intersects_ray(ray)
    }
}

/// Sphyl (Capsule) primitive: Cylinder along Z capped with two hemispheres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32, // Length between hemisphere centers is 2 * half_height
}

impl Capsule {
    pub fn new(center: Vec3, radius: f32, half_height: f32) -> Self {
        Self {
            center,
            radius,
            half_height,
        }
    }

    /// Ray vs Capsule intersection test.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let p_top = self.center + Vec3::new(0.0, 0.0, self.half_height);
        let p_bot = self.center - Vec3::new(0.0, 0.0, self.half_height);

        let sphere_top = Sphere::new(p_top, self.radius);
        let sphere_bot = Sphere::new(p_bot, self.radius);

        let mut best_hit: Option<RayHit> = None;

        // Check cylinder segment
        let cylinder = BoundingCylinder::new(self.center, self.radius, self.half_height);
        if let Some(hit) = cylinder.intersects_ray(ray) {
            // Only accept side hits on cylinder
            if hit.normal.z.abs() < 1e-4 {
                best_hit = Some(hit);
            }
        }

        // Check top hemisphere
        if let Some(hit) = sphere_top.intersects_ray(ray) {
            if hit.location.z >= p_top.z
                && best_hit.is_none_or(|b| hit.distance < b.distance) {
                    best_hit = Some(hit);
                }
        }

        // Check bottom hemisphere
        if let Some(hit) = sphere_bot.intersects_ray(ray) {
            if hit.location.z <= p_bot.z
                && best_hit.is_none_or(|b| hit.distance < b.distance) {
                    best_hit = Some(hit);
                }
        }

        best_hit
    }
}

/// 3D Hyperplane primitive: `n . p - dist = 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub dist: f32,
}

impl Plane {
    pub fn new(normal: Vec3, dist: f32) -> Self {
        let len = normal.length();
        let inv_len = if len > 1e-8 { 1.0 / len } else { 1.0 };
        Self {
            normal: normal * inv_len,
            dist: dist * inv_len,
        }
    }

    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Self {
        let n = normal.normalize_or_zero();
        let dist = n.dot(point);
        Self { normal: n, dist }
    }

    /// Signed distance from point to plane.
    #[inline]
    pub fn distance_to_point(&self, p: Vec3) -> f32 {
        self.normal.dot(p) - self.dist
    }

    /// Intersect ray with infinite plane.
    pub fn intersects_ray(&self, ray: &Ray) -> Option<RayHit> {
        let denom = self.normal.dot(ray.direction);
        if denom.abs() < 1e-8 {
            return None; // Ray is parallel to plane
        }

        let t = (self.dist - self.normal.dot(ray.origin)) / denom;
        if t >= 0.0 && t <= ray.length {
            let hit_loc = ray.at_dist(t);
            let hit_norm = if denom < 0.0 { self.normal } else { -self.normal };
            let time = if ray.length > 0.0 { t / ray.length } else { 0.0 };
            Some(RayHit {
                time,
                distance: t,
                location: hit_loc,
                normal: hit_norm,
                item: -1,
            })
        } else {
            None
        }
    }
}
