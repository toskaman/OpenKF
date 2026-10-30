//! In-memory representation of an Unreal Engine 2 `UModel` containing the compiled BSP tree.

use crate::types::{BspZone, FBox, FBspNode, FBspSurf, FSphere, FVector, FVert};

/// Full level map BSP geometry and spatial acceleration structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UModel {
    /// World-space axis-aligned bounding box of the brush geometry.
    pub bounding_box: FBox,
    /// World-space bounding sphere of the brush geometry.
    pub bounding_sphere: FSphere,
    /// Shared pool of 3D direction / normal / texture vectors.
    pub vectors: Vec<FVector>,
    /// Shared pool of 3D vertex positions.
    pub points: Vec<FVector>,
    /// Array of BSP nodes forming the partitioning binary tree.
    pub bsp_nodes: Vec<FBspNode>,
    /// Array of BSP surface descriptors (materials, texture axes, lightmaps).
    pub bsp_surfs: Vec<FBspSurf>,
    /// Array of polygon vertex references into the `points` array.
    pub verts: Vec<FVert>,
    /// Count of shared polygon sides.
    pub num_shared_sides: i32,
    /// Total count of visibility / zone partitioning volumes.
    pub num_zones: i32,
    /// Visibility and connectivity information for each zone.
    pub zones: Vec<BspZone>,
    /// Indices of leaf zones.
    pub leaves: Vec<i32>,
    /// Flag indicating whether the root node represents outside space.
    pub root_outside: i32,
    /// Linked BSP state flag.
    pub linked: i32,
}

/// Alias for `UModel` matching engine BSP model conventions.
pub type BspModel = UModel;

impl UModel {
    /// Create a new, empty UModel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Access a BSP node by index if valid.
    #[inline]
    pub fn node(&self, idx: usize) -> Option<&FBspNode> {
        self.bsp_nodes.get(idx)
    }

    /// Access a surface descriptor by index if valid.
    #[inline]
    pub fn surf(&self, idx: usize) -> Option<&FBspSurf> {
        self.bsp_surfs.get(idx)
    }

    /// Access a 3D vertex position by index if valid.
    #[inline]
    pub fn point(&self, idx: usize) -> Option<FVector> {
        self.points.get(idx).copied()
    }

    /// Access a 3D vector by index if valid.
    #[inline]
    pub fn vector(&self, idx: usize) -> Option<FVector> {
        self.vectors.get(idx).copied()
    }

    /// Access a vertex reference by index if valid.
    #[inline]
    pub fn vert(&self, idx: usize) -> Option<&FVert> {
        self.verts.get(idx)
    }

    /// Retrieve the slice of `FVert` references belonging to a specific `FBspNode`.
    pub fn node_verts(&self, node: &FBspNode) -> Option<&[FVert]> {
        if node.i_vert_pool < 0 {
            return None;
        }
        let start = node.i_vert_pool as usize;
        let count = node.num_vertices as usize;
        let end = start.checked_add(count)?;
        if end <= self.verts.len() {
            Some(&self.verts[start..end])
        } else {
            None
        }
    }

    /// Retrieve the 3D world-space positions for all vertices of a node.
    pub fn node_positions(&self, node: &FBspNode) -> Option<Vec<FVector>> {
        let verts = self.node_verts(node)?;
        let mut positions = Vec::with_capacity(verts.len());
        for v in verts {
            let pt = self.point(v.p_vertex as usize)?;
            positions.push(pt);
        }
        Some(positions)
    }

    /// Total count of valid nodes in the BSP tree.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.bsp_nodes.len()
    }

    /// Total count of unique surfaces in the model.
    #[inline]
    pub fn surf_count(&self) -> usize {
        self.bsp_surfs.len()
    }

    /// Total count of 3D points in the vertex pool.
    #[inline]
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Traverse the BSP tree to determine which leaf zone a point falls into.
    pub fn point_zone(&self, point: FVector) -> u8 {
        if self.bsp_nodes.is_empty() {
            return 0;
        }
        let mut curr_idx = 0;
        loop {
            let node = &self.bsp_nodes[curr_idx];
            let dist = node.plane.distance_to_point(point);
            let child_idx = if dist >= 0.0 {
                node.i_child[0]
            } else {
                node.i_child[1]
            };

            if child_idx < 0 {
                // Leaf reached: front child returns zone[0], back child returns zone[1]
                return if dist >= 0.0 {
                    node.i_zone[0]
                } else {
                    node.i_zone[1]
                };
            }
            curr_idx = child_idx as usize;
            if curr_idx >= self.bsp_nodes.len() {
                return 0;
            }
        }
    }
}
