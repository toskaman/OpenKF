//! # openkf-package
//!
//! Clean-room, high-performance, zero-copy parser and serializer for Unreal Engine 2.5
//! package format (Engine Version 128 / Licensee Version 29: Killing Floor v1065).
//!
//! Implements:
//! - Exact bitwise `FCompactIndex` serialization/deserialization from `Core.dll` (0x10147af0).
//! - Zero-copy memory-mapped file package reader (`UnrealPackage`, `MmapPackage`).
//! - Complete Name Table, Import Table, and Export Table serialization and deserialization.
//! - Round-trip package synthesis with `PackageWriter`.

pub mod animation;
pub mod class_fields;
pub mod compact_index;
pub mod error;
pub mod export_table;
pub mod header;
pub mod level;
pub mod model;
pub mod import_table;
pub mod name_table;
pub mod navigation;
pub mod properties;
pub mod reader;
pub mod skeletal;
pub mod sound;
pub mod static_mesh;
pub mod terrain;
pub mod texture;
pub mod uc_defaults;
pub mod writer;

pub use animation::{read_mesh_animation, AnalogTrack, AnimNotify, AnimSequence, MeshAnimation, MotionChunk};
pub use compact_index::{
    encoded_compact_index_len, read_compact_index, write_compact_index,
};
pub use error::PackageError;
pub use export_table::{ExportTable, FObjectExport, ObjectFlags};
pub use header::{
    FGenerationInfo, FPackageFileSummary, PackageFlags, ENGINE_VERSION_KF,
    LICENSEE_VERSION_KF, PACKAGE_TAG, PACKAGE_TAG_STANDARD,
};
pub use import_table::{FObjectImport, ImportTable};
pub use level::{read_instance_colors, export_class_name, read_level_model, read_object_properties, rotation_matrix, static_mesh_placements, MeshPlacement, RF_HAS_STACK};
pub use model::{read_model, BspSection, BspSectionVertex, LightMapTexture, BspNode, BspPolygon, BspSurf, BspVert, Model, PF_FAKE_BACKDROP, PF_INVISIBLE, PF_MASKED, PF_MODULATED, PF_PORTAL, PF_TRANSLUCENT, PF_TWO_SIDED, PF_UNLIT};
pub use name_table::{FNameEntry, NameTable};
pub use properties::{read_fstring, read_properties, PropType, PropValue, Property, PropertyList};
pub use reader::{MmapPackage, ObjectRef, UnrealPackage};
pub use skeletal::{read_skeletal_mesh, Bone, Influence, SkeletalMesh, Triangle, Wedge};
pub use static_mesh::{read_static_mesh, static_mesh_exports, BoundingBox, StaticMesh, StaticMeshMaterial, StaticMeshSection};
pub use terrain::{read_terrain, read_terrains, Terrain, TerrainLayer};
pub use texture::{decode_bc_rgba, read_palette, read_texture, texture_exports, Mip, Texture, TextureFormat};
pub use writer::PackageWriter;
