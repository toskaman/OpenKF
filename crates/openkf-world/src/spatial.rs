use glam::{IVec3, Vec3};
use hecs::Entity;
use std::collections::{HashMap, HashSet};

/// Broadphase 3D Spatial Partitioning Hash Grid.
/// Directly replaces UE2's legacy `FCollisionHash` (`ULevel::Hash` at offset `0x24`).
#[derive(Debug, Clone)]
pub struct SpatialHashGrid {
    cell_size: f32,
    inv_cell_size: f32,
    cells: HashMap<IVec3, Vec<Entity>>,
}

impl Default for SpatialHashGrid {
    fn default() -> Self {
        Self::new(512.0) // 512 Unreal Units cell size
    }
}

impl SpatialHashGrid {
    pub fn new(cell_size: f32) -> Self {
        let size = if cell_size <= 0.0 { 512.0 } else { cell_size };
        Self {
            cell_size: size,
            inv_cell_size: 1.0 / size,
            cells: HashMap::new(),
        }
    }

    #[inline]
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    #[inline]
    pub fn point_to_cell(&self, point: Vec3) -> IVec3 {
        IVec3::new(
            (point.x * self.inv_cell_size).floor() as i32,
            (point.y * self.inv_cell_size).floor() as i32,
            (point.z * self.inv_cell_size).floor() as i32,
        )
    }

    #[inline]
    pub fn aabb_to_cells(&self, min: Vec3, max: Vec3) -> (IVec3, IVec3) {
        let min_cell = self.point_to_cell(min);
        let max_cell = self.point_to_cell(max);
        (min_cell, max_cell)
    }

    /// Insert entity into all overlapping spatial cells.
    pub fn insert(&mut self, entity: Entity, min: Vec3, max: Vec3) {
        let (min_cell, max_cell) = self.aabb_to_cells(min, max);
        for x in min_cell.x..=max_cell.x {
            for y in min_cell.y..=max_cell.y {
                for z in min_cell.z..=max_cell.z {
                    let cell = IVec3::new(x, y, z);
                    let list = self.cells.entry(cell).or_default();
                    if !list.contains(&entity) {
                        list.push(entity);
                    }
                }
            }
        }
    }

    /// Remove entity from overlapping spatial cells.
    pub fn remove(&mut self, entity: Entity, min: Vec3, max: Vec3) {
        let (min_cell, max_cell) = self.aabb_to_cells(min, max);
        for x in min_cell.x..=max_cell.x {
            for y in min_cell.y..=max_cell.y {
                for z in min_cell.z..=max_cell.z {
                    let cell = IVec3::new(x, y, z);
                    if let Some(list) = self.cells.get_mut(&cell) {
                        list.retain(|&e| e != entity);
                        if list.is_empty() {
                            self.cells.remove(&cell);
                        }
                    }
                }
            }
        }
    }

    /// Update entity bounds in grid.
    pub fn update(&mut self, entity: Entity, old_min: Vec3, old_max: Vec3, new_min: Vec3, new_max: Vec3) {
        let (old_min_c, old_max_c) = self.aabb_to_cells(old_min, old_max);
        let (new_min_c, new_max_c) = self.aabb_to_cells(new_min, new_max);

        if old_min_c != new_min_c || old_max_c != new_max_c {
            self.remove(entity, old_min, old_max);
            self.insert(entity, new_min, new_max);
        }
    }

    /// Query unique entities within an Axis-Aligned Bounding Box.
    pub fn query_aabb(&self, min: Vec3, max: Vec3) -> Vec<Entity> {
        let (min_cell, max_cell) = self.aabb_to_cells(min, max);
        let mut results = HashSet::new();

        for x in min_cell.x..=max_cell.x {
            for y in min_cell.y..=max_cell.y {
                for z in min_cell.z..=max_cell.z {
                    let cell = IVec3::new(x, y, z);
                    if let Some(entities) = self.cells.get(&cell) {
                        for &e in entities {
                            results.insert(e);
                        }
                    }
                }
            }
        }

        results.into_iter().collect()
    }

    /// Query unique entities within a radial sphere around a center point.
    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<Entity> {
        let r_vec = Vec3::splat(radius);
        self.query_aabb(center - r_vec, center + r_vec)
    }

    /// Broadphase raycast returning candidate entities whose cells intersect the ray.
    pub fn raycast_candidates(&self, start: Vec3, dir: Vec3, max_dist: f32) -> Vec<Entity> {
        let dir_norm = dir.normalize_or_zero();
        if dir_norm == Vec3::ZERO || max_dist <= 0.0 {
            return Vec::new();
        }

        let end = start + dir_norm * max_dist;
        let min = start.min(end);
        let max = start.max(end);
        self.query_aabb(min, max)
    }

    /// Total count of occupied cells.
    #[inline]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Clear all spatial records.
    pub fn clear(&mut self) {
        self.cells.clear();
    }
}
