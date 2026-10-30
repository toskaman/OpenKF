//! UStaticMesh root asset container representation.
//!
//! Specified in `scratch/recon_09_static_meshes.md`:
//! - Container for LOD models (`FStaticMeshLODModel`), material assignments, bounds, and kDOP collision.

use crate::kdop::KdopTree;
use crate::lod_model::FStaticMeshLODModel;
use crate::section::{FBox, FSphere};
use glam::Vec3;

/// Unified bounding box and sphere container (`FBoxSphereBounds`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FBoxSphereBounds {
    pub origin: Vec3,
    pub box_extent: Vec3,
    pub sphere_radius: f32,
}

impl FBoxSphereBounds {
    pub fn new(origin: Vec3, box_extent: Vec3, sphere_radius: f32) -> Self {
        Self {
            origin,
            box_extent,
            sphere_radius,
        }
    }

    pub fn from_box_and_sphere(bbox: &FBox, sphere: &FSphere) -> Self {
        let origin = (bbox.min + bbox.max) * 0.5;
        let box_extent = (bbox.max - bbox.min) * 0.5;
        Self {
            origin,
            box_extent,
            sphere_radius: sphere.radius,
        }
    }
}

impl From<FBox> for FBoxSphereBounds {
    fn from(bbox: FBox) -> Self {
        let origin = (bbox.min + bbox.max) * 0.5;
        let box_extent = (bbox.max - bbox.min) * 0.5;
        let sphere_radius = box_extent.length();
        Self {
            origin,
            box_extent,
            sphere_radius,
        }
    }
}

impl From<(Vec3, Vec3)> for FBoxSphereBounds {
    fn from((min, max): (Vec3, Vec3)) -> Self {
        FBox::new(min, max).into()
    }
}

/// Static mesh material link record (`FStaticMeshMaterial`).
#[derive(Clone, Debug, PartialEq)]
pub struct FStaticMeshMaterial {
    /// Object index or material slot index.
    pub material_index: i32,
    /// Whether collision should be tested against surfaces using this material.
    pub enable_collision: bool,
}

impl FStaticMeshMaterial {
    pub fn new(material_index: i32, enable_collision: bool) -> Self {
        Self {
            material_index,
            enable_collision,
        }
    }
}

/// The core static mesh asset container (`UStaticMesh`).
#[derive(Clone, Debug, PartialEq)]
pub struct UStaticMesh {
    /// Asset name in package.
    pub name: String,
    /// Object-space bounds (box extent and bounding sphere).
    pub bounds: FBoxSphereBounds,
    /// Array of Level of Detail models (LOD 0, LOD 1, etc.).
    pub lod_models: Vec<FStaticMeshLODModel>,
    /// Array of material assignments.
    pub materials: Vec<FStaticMeshMaterial>,
    /// Optional precomputed kDOP-18 or kDOP-26 collision acceleration tree.
    pub kdop_tree: Option<KdopTree>,
}

impl UStaticMesh {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bounds: FBoxSphereBounds::new(Vec3::ZERO, Vec3::ZERO, 0.0),
            lod_models: Vec::new(),
            materials: Vec::new(),
            kdop_tree: None,
        }
    }

    /// Primary LOD level (LOD 0).
    pub fn base_lod(&self) -> Option<&FStaticMeshLODModel> {
        self.lod_models.first()
    }

    /// Select active LOD model based on distance to camera.
    pub fn select_lod(&self, camera_distance: f32) -> Option<&FStaticMeshLODModel> {
        if self.lod_models.is_empty() {
            return None;
        }

        let mut selected = &self.lod_models[0];
        for lod in &self.lod_models {
            if camera_distance >= lod.lod_distance {
                selected = lod;
            }
        }
        Some(selected)
    }
}
