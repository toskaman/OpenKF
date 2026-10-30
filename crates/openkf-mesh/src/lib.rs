//! `openkf-mesh`: Static Mesh Architecture, kDOP Collision & GPU Indirect Instancing for OpenKF.
//!
//! Based on Unreal Engine 2.5 retail binary analysis of Killing Floor (`Engine.dll`, `.usx` static mesh packages).
//! Implements:
//! 1. `UStaticMesh` container and `.usx` package deserialization.
//! 2. `FStaticMeshLODModel` and `FStaticMeshSection` with 36-byte interleaved vertex streams.
//! 3. `kDOP-18` and `kDOP-26` discrete oriented polytope collision trees with ray-slab intersection.
//! 4. Modern GPU Indirect Instancing (`MeshInstanceData` 80 bytes + `DrawIndexedIndirectCommand` 20 bytes).

pub mod instancing;
pub mod kdop;
pub mod lod_model;
pub mod mesh;
pub mod package_parser;
pub mod renderer;
pub mod section;
pub mod vertex;

// Re-export primary types
pub use instancing::{DrawIndexedIndirectCommand, IndirectInstancingBufferGenerator, MeshInstanceData};
pub use kdop::{KdopCollisionTriangle, KdopNode, KdopRayHit, KdopTree, KdopType, KDOP_NORMALS_26};
pub use lod_model::FStaticMeshLODModel;
pub use mesh::{FBoxSphereBounds, FStaticMeshMaterial, UStaticMesh};
pub use package_parser::{read_compact_index, MeshParseError, ObjectExport, ObjectImport, PackageSummary, UsxPackageParser, PACKAGE_MAGIC};
pub use renderer::{Frustum, FrustumPlane, IndirectDrawBatch, MeshBatchRenderer, SubmeshDrawBatch, SubmittedInstance};
pub use section::{FBox, FSphere, FStaticMeshSection};
pub use vertex::{f16_to_f32, f32_to_f16, StaticMeshVertex, StaticMeshVertexExt};
