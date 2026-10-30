//! # openkf-bsp
//!
//! Binary Space Partitioning (BSP) tree parser, polygon fan-triangulation,
//! UV coordinate projection, and modern triangle mesh extractor for Unreal Engine 2
//! / Killing Floor (`Engine.dll` `UModel` / `Maps/*.rom`).
//!
//! ## Specifications (from scratch/recon_07_bsp_map_parser.md):
//! - `FBspNode`: Exactly 64 bytes (aligned), node in spatial partition tree
//! - `FBspSurf`: Exactly 60 bytes, surface polygon & material descriptor
//! - `FVert`: Exactly 8 bytes, vertex reference into shared point pool
//! - `FVector`: Exactly 12 bytes (3 x f32)
//! - `FPlane`: Exactly 16 bytes (4 x f32)
//!
//! ## Example
//! ```rust
//! use openkf_bsp::{UModel, extract_bsp_geometry};
//!
//! let model = UModel::new();
//! let meshes = extract_bsp_geometry(&model);
//! assert!(meshes.is_empty());
//! ```

pub mod extractor;
pub mod gpu_builder;
pub mod model;
pub mod parser;
pub mod triangulate;
pub mod types;
pub mod uv;

pub use extractor::{extract_bsp_geometry, extract_unified_mesh, BspMesh, BspVertex};
pub use gpu_builder::{
    build_gpu_mesh, BspMeshBuilder, GpuBspBatch, GpuBspGeometry, GpuBspVertex,
    VulkanVertexAttribute, VulkanVertexBinding, VulkanVertexFormat,
};
pub use model::{BspModel, UModel};
pub use parser::{
    decode_compact_index, encode_compact_index, read_fbsp_node_raw, read_fbsp_surf_raw,
    read_fvert_raw, BspError, UModelParser,
};
pub use triangulate::{
    is_convex_polygon, polygon_area, triangle_area, triangulate_fan, triangulate_polygon_indices,
    triangulate_slice,
};
pub use types::{
    BspZone, FBox, FBspNode, FBspSurf, FPlane, FSphere, FVector, FVert, NodeFlags, PolyFlags,
};
pub use uv::{compute_unreal_texture_axes, project_lightmap_uv, project_texture_uv};
