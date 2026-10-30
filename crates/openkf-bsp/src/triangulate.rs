//! Convex polygon fan-triangulator for Unreal BSP coplanar polygons.
//!
//! Converts convex N-gons into (N - 2) indexed triangles:
//! `(0, 1, 2), (0, 2, 3), ..., (0, i, i + 1), ..., (0, N - 2, N - 1)`.

use glam::Vec3;

/// Triangulate a convex polygon using triangle fan decomposition.
/// Appends triangle indices into the provided `out_indices` buffer starting from `base_index`.
///
/// Returns the number of triangles added (which is `max(0, num_vertices - 2)`).
pub fn triangulate_fan(num_vertices: usize, base_index: u32, out_indices: &mut Vec<u32>) -> usize {
    if num_vertices < 3 {
        return 0;
    }

    let num_triangles = num_vertices - 2;
    out_indices.reserve(num_triangles * 3);

    for i in 1..(num_vertices - 1) {
        out_indices.push(base_index);
        out_indices.push(base_index + i as u32);
        out_indices.push(base_index + (i + 1) as u32);
    }

    num_triangles
}

/// Triangulate a convex polygon and return an array of 3-index tuples.
pub fn triangulate_polygon_indices(num_vertices: usize) -> Vec<[u32; 3]> {
    if num_vertices < 3 {
        return Vec::new();
    }
    let mut triangles = Vec::with_capacity(num_vertices - 2);
    for i in 1..(num_vertices - 1) {
        triangles.push([0, i as u32, (i + 1) as u32]);
    }
    triangles
}

/// Generic triangle fan decomposition for a slice of items.
pub fn triangulate_slice<T: Copy>(items: &[T]) -> Vec<[T; 3]> {
    if items.len() < 3 {
        return Vec::new();
    }
    let mut triangles = Vec::with_capacity(items.len() - 2);
    for i in 1..(items.len() - 1) {
        triangles.push([items[0], items[i], items[i + 1]]);
    }
    triangles
}

/// Calculates the surface area of a 3D triangle given 3 vertices.
#[inline]
pub fn triangle_area(p0: Vec3, p1: Vec3, p2: Vec3) -> f32 {
    let cross = (p1 - p0).cross(p2 - p0);
    0.5 * cross.length()
}

/// Calculates the total surface area of a convex polygon by summing fan triangle areas.
pub fn polygon_area(vertices: &[Vec3]) -> f32 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut total_area = 0.0;
    for i in 1..(vertices.len() - 1) {
        total_area += triangle_area(vertices[0], vertices[i], vertices[i + 1]);
    }
    total_area
}

/// Checks if a 3D polygon is convex and planar with respect to the given normal.
pub fn is_convex_polygon(vertices: &[Vec3], normal: Vec3) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }

    let norm = normal.normalize();
    for i in 0..n {
        let p0 = vertices[i];
        let p1 = vertices[(i + 1) % n];
        let p2 = vertices[(i + 2) % n];

        let edge1 = p1 - p0;
        let edge2 = p2 - p1;
        let cross = edge1.cross(edge2);

        // If the cross product has a negative dot with normal, polygon is not convex / concave
        if cross.dot(norm) < -1e-4 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangulate_fan_quad() {
        let mut indices = Vec::new();
        let count = triangulate_fan(4, 10, &mut indices);
        assert_eq!(count, 2);
        assert_eq!(indices, vec![10, 11, 12, 10, 12, 13]);
    }

    #[test]
    fn test_triangulate_fan_triangle() {
        let mut indices = Vec::new();
        let count = triangulate_fan(3, 0, &mut indices);
        assert_eq!(count, 1);
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_triangulate_fan_degenerate() {
        let mut indices = Vec::new();
        assert_eq!(triangulate_fan(0, 0, &mut indices), 0);
        assert_eq!(triangulate_fan(1, 0, &mut indices), 0);
        assert_eq!(triangulate_fan(2, 0, &mut indices), 0);
        assert!(indices.is_empty());
    }

    #[test]
    fn test_triangulate_fan_hexagon() {
        let mut indices = Vec::new();
        let count = triangulate_fan(6, 0, &mut indices);
        assert_eq!(count, 4);
        assert_eq!(indices.len(), 12);
        assert_eq!(
            indices,
            vec![
                0, 1, 2,
                0, 2, 3,
                0, 3, 4,
                0, 4, 5
            ]
        );
    }

    #[test]
    fn test_polygon_area_square() {
        // 100x100 square in XY plane
        let vertices = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0),
            Vec3::new(0.0, 100.0, 0.0),
        ];
        let area = polygon_area(&vertices);
        assert!((area - 10000.0).abs() < 1e-3);
        assert!(is_convex_polygon(&vertices, Vec3::new(0.0, 0.0, 1.0)));
    }
}
