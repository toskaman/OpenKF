//! GPU Indirect Instancing buffer generator for static meshes.
//!
//! Specified in `scratch/recon_09_static_meshes.md`:
//! - `MeshInstanceData`: 80 bytes (64-byte world matrix + 16-byte lightmap offset/scale)
//! - `DrawIndexedIndirectCommand`: 20 bytes (Vulkan / D3D12 indirect draw command)

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use crate::section::FStaticMeshSection;

/// Per-instance GPU data layout for hardware instanced rendering (`sizeof = 80 bytes`).
///
/// Matches memory layout from `scratch/recon_09_static_meshes.md`:
/// - `world_matrix`: 64 bytes (Object-to-World 4x4 matrix, column-major)
/// - `lightmap_offset_scale`: 16 bytes (X, Y = atlas UV offset; Z, W = atlas UV scale)
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MeshInstanceData {
    /// Object-to-World transformation matrix (64 bytes, 16-byte aligned).
    pub world_matrix: Mat4,
    /// Lightmap atlas offset (X, Y) and scale (Z, W) (16 bytes, 16-byte aligned).
    pub lightmap_offset_scale: Vec4,
}

// Compile-time static assertions for GPU alignment and size
const _: () = assert!(std::mem::size_of::<MeshInstanceData>() == 80);
const _: () = assert!(std::mem::align_of::<MeshInstanceData>() == 16);

impl MeshInstanceData {
    /// Create new instance data with transform and lightmap atlas transform.
    pub fn new(world_matrix: Mat4, lightmap_offset_scale: Vec4) -> Self {
        Self {
            world_matrix,
            lightmap_offset_scale,
        }
    }

    /// Create new instance data with identity lightmap coordinates (no atlas offset, scale 1.0).
    pub fn with_identity_lightmap(world_matrix: Mat4) -> Self {
        Self {
            world_matrix,
            lightmap_offset_scale: Vec4::new(0.0, 0.0, 1.0, 1.0),
        }
    }
}

/// Standard GPU indirect draw command (`sizeof = 20 bytes`).
///
/// Directly compatible with Vulkan `VkDrawIndexedIndirectCommand` and DirectX 12 `D3D12_DRAW_INDEXED_ARGUMENTS`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct DrawIndexedIndirectCommand {
    /// Number of indices to draw (`num_triangles * 3`).
    pub index_count: u32,
    /// Number of instances to draw.
    pub instance_count: u32,
    /// Offset to the first index in the index buffer.
    pub first_index: u32,
    /// Offset added to vertex index values before indexing into vertex buffer.
    pub vertex_offset: i32,
    /// Offset to the first instance in the instance buffer.
    pub first_instance: u32,
}

const _: () = assert!(std::mem::size_of::<DrawIndexedIndirectCommand>() == 20);
const _: () = assert!(std::mem::align_of::<DrawIndexedIndirectCommand>() == 4);

impl DrawIndexedIndirectCommand {
    pub fn new(
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) -> Self {
        Self {
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        }
    }
}

/// Generator for GPU indirect instancing buffers and draw call parameters.
pub struct IndirectInstancingBufferGenerator;

impl IndirectInstancingBufferGenerator {
    /// Cast a slice of instance records to raw bytes ready for GPU buffer transfer.
    pub fn instance_bytes(instances: &[MeshInstanceData]) -> &[u8] {
        bytemuck::cast_slice(instances)
    }

    /// Cast a slice of indirect draw commands to raw bytes ready for GPU indirect buffer.
    pub fn command_bytes(commands: &[DrawIndexedIndirectCommand]) -> &[u8] {
        bytemuck::cast_slice(commands)
    }

    /// Build indirect draw commands for all sections of a static mesh LOD model.
    pub fn generate_section_commands(
        sections: &[FStaticMeshSection],
        instance_count: u32,
        first_instance: u32,
    ) -> Vec<DrawIndexedIndirectCommand> {
        let mut commands = Vec::with_capacity(sections.len());
        for section in sections {
            commands.push(DrawIndexedIndirectCommand {
                index_count: section.index_count(),
                instance_count,
                first_index: section.first_index,
                vertex_offset: 0,
                first_instance,
            });
        }
        commands
    }

    /// Build batched instance data and corresponding draw commands across multiple meshes.
    pub fn build_instanced_batch(
        section: &FStaticMeshSection,
        instances: &[MeshInstanceData],
        first_instance: u32,
    ) -> (Vec<u8>, DrawIndexedIndirectCommand) {
        let cmd = DrawIndexedIndirectCommand {
            index_count: section.index_count(),
            instance_count: instances.len() as u32,
            first_index: section.first_index,
            vertex_offset: 0,
            first_instance,
        };
        let bytes = Self::instance_bytes(instances).to_vec();
        (bytes, cmd)
    }
}
