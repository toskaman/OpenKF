//! GPU Indirect Instancing Batch Renderer for Unreal Engine 2.5 Static Meshes (`.usx`).
//!
//! Provides hardware-accelerated indirect draw submission, camera frustum culling,
//! and multi-material submesh batching for `UStaticMesh` assets.
//!
//! Architecture:
//! 1. `Frustum`: 6-plane camera frustum with Gribb-Hartmann extraction from View-Projection
//!    matrices and fast transformed AABB / sphere rejection (Arvo's method).
//! 2. `MeshBatchRenderer`: Multi-mesh and multi-material submesh batching engine.
//!    - Accepts per-instance transforms and bounding volumes.
//!    - Performs CPU frustum culling prior to GPU buffer upload.
//!    - Builds contiguous 80-byte `MeshInstanceData` GPU buffers.
//!    - Generates 20-byte `DrawIndexedIndirectCommand` arrays directly compatible with
//!      Vulkan (`vkCmdDrawIndexedIndirect`) and Direct3D 12 (`ExecuteIndirect`).

use std::collections::BTreeMap;
use std::ops::Deref;
use glam::{Mat4, Vec3, Vec4};

use crate::instancing::{DrawIndexedIndirectCommand, MeshInstanceData};
use crate::lod_model::FStaticMeshLODModel;
use crate::mesh::{FBoxSphereBounds, UStaticMesh};
use crate::section::{FBox, FStaticMeshSection};

/// Frustum plane enumeration.
#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrustumPlane {
    Left = 0,
    Right = 1,
    Bottom = 2,
    Top = 3,
    Near = 4,
    Far = 5,
}

/// 6-plane Camera Frustum for view-frustum culling.
///
/// Planes are represented in Hessian normal form: `ax + by + cz + d = 0` where
/// `(a, b, c)` is a unit normal pointing into the interior (visible half-space) of the frustum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    /// 6 normalized frustum planes: [Left, Right, Bottom, Top, Near, Far].
    pub planes: [Vec4; 6],
}

impl Frustum {
    /// Create a frustum directly from 6 pre-normalized planes.
    pub fn from_planes(mut planes: [Vec4; 6]) -> Self {
        for plane in &mut planes {
            normalize_plane(plane);
        }
        Self { planes }
    }

    /// Extract 6 camera frustum planes from a combined View-Projection matrix (`P * V`)
    /// using the Gribb-Hartmann method for Vulkan / DirectX clip space (`depth in [0, 1]`).
    pub fn from_view_projection(view_proj: Mat4) -> Self {
        // Extract matrix rows from column-major glam::Mat4
        let row0 = Vec4::new(view_proj.x_axis.x, view_proj.y_axis.x, view_proj.z_axis.x, view_proj.w_axis.x);
        let row1 = Vec4::new(view_proj.x_axis.y, view_proj.y_axis.y, view_proj.z_axis.y, view_proj.w_axis.y);
        let row2 = Vec4::new(view_proj.x_axis.z, view_proj.y_axis.z, view_proj.z_axis.z, view_proj.w_axis.z);
        let row3 = Vec4::new(view_proj.x_axis.w, view_proj.y_axis.w, view_proj.z_axis.w, view_proj.w_axis.w);

        let mut planes = [
            row3 + row0, // Left:   w + x >= 0
            row3 - row0, // Right:  w - x >= 0
            row3 + row1, // Bottom: w + y >= 0
            row3 - row1, // Top:    w - y >= 0
            row2,        // Near:   z >= 0 (Vulkan / D3D12 depth [0, 1])
            row3 - row2, // Far:    w - z >= 0
        ];

        for plane in &mut planes {
            normalize_plane(plane);
        }

        Self { planes }
    }

    /// Extract frustum planes for OpenGL clip space (`depth in [-1, 1]`).
    pub fn from_view_projection_opengl(view_proj: Mat4) -> Self {
        let row0 = Vec4::new(view_proj.x_axis.x, view_proj.y_axis.x, view_proj.z_axis.x, view_proj.w_axis.x);
        let row1 = Vec4::new(view_proj.x_axis.y, view_proj.y_axis.y, view_proj.z_axis.y, view_proj.w_axis.y);
        let row2 = Vec4::new(view_proj.x_axis.z, view_proj.y_axis.z, view_proj.z_axis.z, view_proj.w_axis.z);
        let row3 = Vec4::new(view_proj.x_axis.w, view_proj.y_axis.w, view_proj.z_axis.w, view_proj.w_axis.w);

        let mut planes = [
            row3 + row0, // Left:   w + x >= 0
            row3 - row0, // Right:  w - x >= 0
            row3 + row1, // Bottom: w + y >= 0
            row3 - row1, // Top:    w - y >= 0
            row3 + row2, // Near:   w + z >= 0 (OpenGL depth [-1, 1])
            row3 - row2, // Far:    w - z >= 0
        ];

        for plane in &mut planes {
            normalize_plane(plane);
        }

        Self { planes }
    }

    /// Extract frustum from separate View and Projection matrices.
    pub fn from_view_and_proj(view: Mat4, proj: Mat4) -> Self {
        Self::from_view_projection(proj * view)
    }

    /// Access a specific frustum plane by identifier.
    #[inline]
    pub fn plane(&self, plane: FrustumPlane) -> Vec4 {
        self.planes[plane as usize]
    }

    /// Test if a single 3D point in world space is inside the frustum.
    pub fn test_point(&self, point: Vec3) -> bool {
        for plane in &self.planes {
            let dist = plane.x * point.x + plane.y * point.y + plane.z * point.z + plane.w;
            if dist < 0.0 {
                return false;
            }
        }
        true
    }

    /// Test if a bounding sphere in world space is inside or intersects the frustum.
    pub fn test_sphere(&self, center: Vec3, radius: f32) -> bool {
        for plane in &self.planes {
            let dist = plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w;
            if dist < -radius {
                return false;
            }
        }
        true
    }

    /// Test if an axis-aligned bounding box (center, extent) in world space intersects the frustum.
    pub fn test_aabb(&self, center: Vec3, extent: Vec3) -> bool {
        for plane in &self.planes {
            let r = extent.x * plane.x.abs() + extent.y * plane.y.abs() + extent.z * plane.z.abs();
            let dist = plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w;
            if dist < -r {
                return false;
            }
        }
        true
    }

    /// Test an object-space bounding box `FBox` transformed by `transform` into world space.
    pub fn test_box(&self, bbox: &FBox, transform: &Mat4) -> bool {
        if !bbox.is_valid {
            return true;
        }
        let bounds: FBoxSphereBounds = (*bbox).into();
        self.test_bounds(&bounds, transform)
    }

    /// Test an object-space `FBoxSphereBounds` transformed by `transform` into world space.
    ///
    /// Uses Arvo's fast bounding box transform method combined with plane slab distance testing.
    pub fn test_bounds(&self, bounds: &FBoxSphereBounds, transform: &Mat4) -> bool {
        // Fast early-out rejection using bounding sphere if radius is set
        if bounds.sphere_radius > 0.0 {
            let world_center = (transform.mul_vec4(Vec4::new(bounds.origin.x, bounds.origin.y, bounds.origin.z, 1.0))).truncate();
            let scale_x = transform.x_axis.truncate().length();
            let scale_y = transform.y_axis.truncate().length();
            let scale_z = transform.z_axis.truncate().length();
            let max_scale = scale_x.max(scale_y).max(scale_z);
            let world_radius = bounds.sphere_radius * max_scale;

            let mut sphere_culled = false;
            for plane in &self.planes {
                let dist = plane.x * world_center.x + plane.y * world_center.y + plane.z * world_center.z + plane.w;
                if dist < -world_radius {
                    sphere_culled = true;
                    break;
                }
            }
            if sphere_culled {
                return false;
            }
        }

        // Exact transformed AABB test (Arvo's algorithm)
        let world_center = (transform.mul_vec4(Vec4::new(bounds.origin.x, bounds.origin.y, bounds.origin.z, 1.0))).truncate();

        let x_col = transform.x_axis.truncate();
        let y_col = transform.y_axis.truncate();
        let z_col = transform.z_axis.truncate();

        let world_extent = Vec3::new(
            x_col.x.abs() * bounds.box_extent.x + y_col.x.abs() * bounds.box_extent.y + z_col.x.abs() * bounds.box_extent.z,
            x_col.y.abs() * bounds.box_extent.x + y_col.y.abs() * bounds.box_extent.y + z_col.y.abs() * bounds.box_extent.z,
            x_col.z.abs() * bounds.box_extent.x + y_col.z.abs() * bounds.box_extent.y + z_col.z.abs() * bounds.box_extent.z,
        );

        for plane in &self.planes {
            let r = world_extent.x * plane.x.abs()
                + world_extent.y * plane.y.abs()
                + world_extent.z * plane.z.abs();
            let dist = plane.x * world_center.x
                + plane.y * world_center.y
                + plane.z * world_center.z
                + plane.w;
            if dist < -r {
                return false;
            }
        }

        true
    }
}

#[inline]
fn normalize_plane(plane: &mut Vec4) {
    let normal_len = (plane.x * plane.x + plane.y * plane.y + plane.z * plane.z).sqrt();
    if normal_len > 1e-6 {
        let inv_len = 1.0 / normal_len;
        plane.x *= inv_len;
        plane.y *= inv_len;
        plane.z *= inv_len;
        plane.w *= inv_len;
    }
}

/// Instance submitted for drawing in the current frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubmittedInstance {
    pub mesh_id: u32,
    pub transform: Mat4,
    pub bounds: FBoxSphereBounds,
    pub lightmap_offset_scale: Vec4,
}

/// Metadata describing a single submesh draw call in the generated batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmeshDrawBatch {
    /// Mesh identifier.
    pub mesh_id: u32,
    /// Section index within the mesh LOD model.
    pub section_index: usize,
    /// Unreal Engine material index assigned to this section.
    pub material_index: i32,
    /// GPU indirect draw command for this submesh.
    pub command: DrawIndexedIndirectCommand,
}

/// Resulting GPU indirect draw commands and instance buffer produced by `generate_indirect_draws()`.
#[derive(Clone, Debug, Default)]
pub struct IndirectDrawBatch {
    /// 20-byte GPU indirect commands (`VkDrawIndexedIndirectCommand` / `D3D12_DRAW_INDEXED_ARGUMENTS`).
    pub commands: Vec<DrawIndexedIndirectCommand>,
    /// Contiguous 80-byte `MeshInstanceData` records (World matrix + Lightmap transform).
    pub instances: Vec<MeshInstanceData>,
    /// High-level draw call descriptors with mesh and material metadata.
    pub batches: Vec<SubmeshDrawBatch>,
    /// Total number of instances submitted before culling.
    pub total_submitted: usize,
    /// Number of visible instances passing frustum culling.
    pub total_visible: usize,
    /// Number of culled instances rejected by frustum planes.
    pub total_culled: usize,
}

impl Deref for IndirectDrawBatch {
    type Target = [DrawIndexedIndirectCommand];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.commands
    }
}

impl IndirectDrawBatch {
    /// Access packed instance data as raw bytes ready for GPU instance buffer upload.
    #[inline]
    pub fn instance_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.instances)
    }

    /// Access indirect draw commands as raw bytes ready for GPU indirect argument buffer.
    #[inline]
    pub fn command_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.commands)
    }

    /// Total number of indirect draw commands.
    #[inline]
    pub fn draw_count(&self) -> u32 {
        self.commands.len() as u32
    }

    /// Check if no draw calls were generated.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Group draw batches by material index for minimized shader pipeline state changes.
    pub fn batches_by_material(&self) -> BTreeMap<i32, Vec<&SubmeshDrawBatch>> {
        let mut map = BTreeMap::new();
        for batch in &self.batches {
            map.entry(batch.material_index).or_insert_with(Vec::new).push(batch);
        }
        map
    }
}

/// High-performance GPU Instanced Draw Submission for Unreal Engine 2.5 static meshes.
///
/// Features:
/// - Batched transform matrix instance buffers (`MeshInstanceData`, 80 bytes).
/// - 6-plane view-frustum culling with Arvo transformed AABB testing.
/// - Multi-material submesh batching (`DrawIndexedIndirectCommand`, 20 bytes).
/// - Shared instance buffer indices across multi-material sections of each visible mesh instance.
pub struct MeshBatchRenderer {
    /// Registered sections per mesh ID.
    mesh_sections: BTreeMap<u32, Vec<FStaticMeshSection>>,
    /// Registered default bounds per mesh ID.
    mesh_bounds: BTreeMap<u32, FBoxSphereBounds>,
    /// Submitted instances awaiting batching and indirect draw generation.
    submitted_instances: Vec<SubmittedInstance>,
    /// Optional camera frustum for culling. If None, all submitted instances are rendered.
    camera_frustum: Option<Frustum>,
}

impl Default for MeshBatchRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshBatchRenderer {
    /// Create a new empty static mesh batch renderer.
    pub fn new() -> Self {
        Self {
            mesh_sections: BTreeMap::new(),
            mesh_bounds: BTreeMap::new(),
            submitted_instances: Vec::new(),
            camera_frustum: None,
        }
    }

    /// Create with preallocated capacity for known mesh count and expected instances.
    pub fn with_capacity(_mesh_capacity: usize, instance_capacity: usize) -> Self {
        Self {
            mesh_sections: BTreeMap::new(),
            mesh_bounds: BTreeMap::new(),
            submitted_instances: Vec::with_capacity(instance_capacity),
            camera_frustum: None,
        }
    }

    /// Register a `UStaticMesh` asset, extracting LOD 0 sections and mesh bounds.
    pub fn register_mesh(&mut self, mesh_id: u32, mesh: &UStaticMesh) {
        if let Some(lod0) = mesh.base_lod() {
            self.mesh_sections.insert(mesh_id, lod0.sections.clone());
        } else {
            self.mesh_sections.insert(mesh_id, Vec::new());
        }
        self.mesh_bounds.insert(mesh_id, mesh.bounds);
    }

    /// Register a specific LOD model for a mesh.
    pub fn register_lod(&mut self, mesh_id: u32, lod: &FStaticMeshLODModel) {
        self.mesh_sections.insert(mesh_id, lod.sections.clone());
    }

    /// Register custom render sections for a mesh ID.
    pub fn register_sections(&mut self, mesh_id: u32, sections: Vec<FStaticMeshSection>) {
        self.mesh_sections.insert(mesh_id, sections);
    }

    /// Register a single section for a mesh ID.
    pub fn register_section(&mut self, mesh_id: u32, section: FStaticMeshSection) {
        self.mesh_sections.entry(mesh_id).or_default().push(section);
    }

    /// Register default bounding box/sphere for a mesh ID.
    pub fn register_bounds(&mut self, mesh_id: u32, bounds: impl Into<FBoxSphereBounds>) {
        self.mesh_bounds.insert(mesh_id, bounds.into());
    }

    /// Set camera frustum for view-frustum culling.
    pub fn set_frustum(&mut self, frustum: Frustum) {
        self.camera_frustum = Some(frustum);
    }

    /// Set camera view and projection matrices (automatically extracts frustum).
    pub fn set_camera(&mut self, view: Mat4, proj: Mat4) {
        self.camera_frustum = Some(Frustum::from_view_and_proj(view, proj));
    }

    /// Set combined View-Projection matrix (automatically extracts frustum).
    pub fn set_view_projection(&mut self, view_proj: Mat4) {
        self.camera_frustum = Some(Frustum::from_view_projection(view_proj));
    }

    /// Clear camera frustum (disables frustum culling, rendering all instances).
    pub fn clear_frustum(&mut self) {
        self.camera_frustum = None;
    }

    /// Access currently active camera frustum.
    pub fn camera_frustum(&self) -> Option<&Frustum> {
        self.camera_frustum.as_ref()
    }

    /// Submit a single mesh instance with its object-to-world transform and bounding volume.
    ///
    /// The bounds parameter accepts `FBoxSphereBounds`, `FBox`, or a `(Vec3, Vec3)` min-max tuple.
    pub fn submit_instance(
        &mut self,
        mesh_id: u32,
        transform: Mat4,
        bounds: impl Into<FBoxSphereBounds>,
    ) {
        self.submitted_instances.push(SubmittedInstance {
            mesh_id,
            transform,
            bounds: bounds.into(),
            lightmap_offset_scale: Vec4::new(0.0, 0.0, 1.0, 1.0),
        });
    }

    /// Submit an instance with custom lightmap atlas coordinates (UV offset X,Y and scale Z,W).
    pub fn submit_instance_with_lightmap(
        &mut self,
        mesh_id: u32,
        transform: Mat4,
        bounds: impl Into<FBoxSphereBounds>,
        lightmap_offset_scale: Vec4,
    ) {
        self.submitted_instances.push(SubmittedInstance {
            mesh_id,
            transform,
            bounds: bounds.into(),
            lightmap_offset_scale,
        });
    }

    /// Submit an instance using the mesh's pre-registered bounds.
    ///
    /// Returns `true` if mesh bounds were found and submitted, or `false` if the mesh has no registered bounds.
    pub fn submit_mesh_instance(&mut self, mesh_id: u32, transform: Mat4) -> bool {
        if let Some(&bounds) = self.mesh_bounds.get(&mesh_id) {
            self.submit_instance(mesh_id, transform, bounds);
            true
        } else {
            false
        }
    }

    /// Generate GPU indirect draw commands and instance buffers for all submitted instances.
    ///
    /// Executes frustum culling against the active camera frustum (if set),
    /// packs surviving instances into a contiguous `MeshInstanceData` buffer,
    /// and generates `DrawIndexedIndirectCommand`s for each material section.
    pub fn generate_indirect_draws(&self) -> IndirectDrawBatch {
        self.generate_indirect_draws_with_frustum(self.camera_frustum.as_ref())
    }

    /// Generate indirect draw commands with an explicit frustum override (or `None` to bypass culling).
    pub fn generate_indirect_draws_with_frustum(&self, frustum: Option<&Frustum>) -> IndirectDrawBatch {
        let total_submitted = self.submitted_instances.len();
        let mut total_culled = 0;

        // Group visible instances by mesh_id (deterministic BTreeMap)
        let mut visible_by_mesh: BTreeMap<u32, Vec<MeshInstanceData>> = BTreeMap::new();

        for inst in &self.submitted_instances {
            let is_visible = match frustum {
                Some(f) => f.test_bounds(&inst.bounds, &inst.transform),
                None => true,
            };

            if is_visible {
                let data = MeshInstanceData::new(inst.transform, inst.lightmap_offset_scale);
                visible_by_mesh.entry(inst.mesh_id).or_default().push(data);
            } else {
                total_culled += 1;
            }
        }

        let total_visible = total_submitted - total_culled;

        let mut commands = Vec::new();
        let mut instances = Vec::with_capacity(total_visible);
        let mut batches = Vec::new();

        for (&mesh_id, mesh_insts) in &visible_by_mesh {
            if mesh_insts.is_empty() {
                continue;
            }

            let instance_count = mesh_insts.len() as u32;
            let first_instance = instances.len() as u32;
            instances.extend_from_slice(mesh_insts);

            if let Some(sections) = self.mesh_sections.get(&mesh_id) {
                if sections.is_empty() {
                    let cmd = DrawIndexedIndirectCommand::new(0, instance_count, 0, 0, first_instance);
                    commands.push(cmd);
                    batches.push(SubmeshDrawBatch {
                        mesh_id,
                        section_index: 0,
                        material_index: 0,
                        command: cmd,
                    });
                } else {
                    for (section_index, section) in sections.iter().enumerate() {
                        let cmd = DrawIndexedIndirectCommand::new(
                            section.index_count(),
                            instance_count,
                            section.first_index,
                            0,
                            first_instance,
                        );
                        commands.push(cmd);
                        batches.push(SubmeshDrawBatch {
                            mesh_id,
                            section_index,
                            material_index: section.material_index,
                            command: cmd,
                        });
                    }
                }
            } else {
                let cmd = DrawIndexedIndirectCommand::new(0, instance_count, 0, 0, first_instance);
                commands.push(cmd);
                batches.push(SubmeshDrawBatch {
                    mesh_id,
                    section_index: 0,
                    material_index: 0,
                    command: cmd,
                });
            }
        }

        IndirectDrawBatch {
            commands,
            instances,
            batches,
            total_submitted,
            total_visible,
            total_culled,
        }
    }

    /// Generate indirect draw commands sorted primarily by `material_index` to minimize PSO changes.
    pub fn generate_indirect_draws_sorted_by_material(&self) -> IndirectDrawBatch {
        let mut batch = self.generate_indirect_draws();
        // Sort submesh batches by material_index, then mesh_id, then section_index
        batch.batches.sort_by(|a, b| {
            a.material_index
                .cmp(&b.material_index)
                .then(a.mesh_id.cmp(&b.mesh_id))
                .then(a.section_index.cmp(&b.section_index))
        });
        batch.commands = batch.batches.iter().map(|b| b.command).collect();
        batch
    }

    /// Clear all submitted instances for the next frame, retaining registered meshes.
    pub fn clear(&mut self) {
        self.submitted_instances.clear();
    }

    /// Clear all instances and registered mesh templates.
    pub fn clear_all(&mut self) {
        self.submitted_instances.clear();
        self.mesh_sections.clear();
        self.mesh_bounds.clear();
        self.camera_frustum = None;
    }

    /// Number of submitted instances currently queued for drawing.
    #[inline]
    pub fn submitted_count(&self) -> usize {
        self.submitted_instances.len()
    }

    /// Number of registered static mesh types.
    #[inline]
    pub fn registered_mesh_count(&self) -> usize {
        self.mesh_sections.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frustum_point_and_box_culling() {
        // Look from origin down +Z, near=1, far=100
        let view = Mat4::look_to_rh(Vec3::ZERO, Vec3::Z, Vec3::Y);
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 100.0);
        let frustum = Frustum::from_view_and_proj(view, proj);

        // Point right in front of camera
        assert!(frustum.test_point(Vec3::new(0.0, 0.0, 10.0)));
        // Point behind camera
        assert!(!frustum.test_point(Vec3::new(0.0, 0.0, -10.0)));
        // Point past far plane
        assert!(!frustum.test_point(Vec3::new(0.0, 0.0, 150.0)));

        // Box inside frustum
        let box_inside = FBox::new(Vec3::new(-1.0, -1.0, 5.0), Vec3::new(1.0, 1.0, 7.0));
        assert!(frustum.test_box(&box_inside, &Mat4::IDENTITY));

        // Box behind camera
        let box_behind = FBox::new(Vec3::new(-1.0, -1.0, -10.0), Vec3::new(1.0, 1.0, -8.0));
        assert!(!frustum.test_box(&box_behind, &Mat4::IDENTITY));

        // Box far to the right (culled)
        let box_right = FBox::new(Vec3::new(50.0, -1.0, 5.0), Vec3::new(60.0, 1.0, 7.0));
        assert!(!frustum.test_box(&box_right, &Mat4::IDENTITY));
    }

    #[test]
    fn test_multi_material_submesh_batching() {
        let mut renderer = MeshBatchRenderer::new();

        // Mesh 1: 2 sections (Section 0 with material 0, Section 1 with material 1)
        let s0 = FStaticMeshSection::new(0, 0, 100, 0, 99);
        let s1 = FStaticMeshSection::new(1, 300, 50, 100, 149);
        renderer.register_sections(1, vec![s0, s1]);

        // Mesh 2: 1 section (Section 0 with material 2)
        let s2 = FStaticMeshSection::new(2, 0, 200, 0, 199);
        renderer.register_sections(2, vec![s2]);

        // Submit 2 instances of Mesh 1 and 1 instance of Mesh 2
        let bounds = FBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        renderer.submit_instance(1, Mat4::from_translation(Vec3::new(0.0, 0.0, 10.0)), bounds);
        renderer.submit_instance(1, Mat4::from_translation(Vec3::new(5.0, 0.0, 10.0)), bounds);
        renderer.submit_instance(2, Mat4::from_translation(Vec3::new(0.0, 0.0, 20.0)), bounds);

        let batch = renderer.generate_indirect_draws();

        // Mesh 1 has 2 sections, Mesh 2 has 1 section => 3 commands total
        assert_eq!(batch.commands.len(), 3);
        assert_eq!(batch.instances.len(), 3);

        // Section 0 of Mesh 1: 300 indices (100 triangles), 2 instances, first_instance 0
        assert_eq!(batch.commands[0].index_count, 300);
        assert_eq!(batch.commands[0].instance_count, 2);
        assert_eq!(batch.commands[0].first_instance, 0);

        // Section 1 of Mesh 1: 150 indices (50 triangles), 2 instances, first_instance 0
        assert_eq!(batch.commands[1].index_count, 150);
        assert_eq!(batch.commands[1].instance_count, 2);
        assert_eq!(batch.commands[1].first_instance, 0);

        // Section 0 of Mesh 2: 600 indices (200 triangles), 1 instance, first_instance 2
        assert_eq!(batch.commands[2].index_count, 600);
        assert_eq!(batch.commands[2].instance_count, 1);
        assert_eq!(batch.commands[2].first_instance, 2);

        // Instance data bytes: 3 instances * 80 bytes = 240 bytes
        assert_eq!(batch.instance_bytes().len(), 240);
        // Command bytes: 3 commands * 20 bytes = 60 bytes
        assert_eq!(batch.command_bytes().len(), 60);
    }
}
