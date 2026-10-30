use std::io::{Cursor, Read, Write};

use glam::{Vec2, Vec3};

use crate::bone::{FBone, FBoneBinary, Skeleton};
use crate::error::AnimationError;
use crate::skinning::VertexInput;

pub const USKELMESH_MAGIC: [u8; 4] = *b"USKM";
pub const USKELMESH_VERSION: u32 = 1;

/// Bounding sphere and oriented bounding box for frustum culling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FBoxSphereBounds {
    pub origin: Vec3,
    pub box_extent: Vec3,
    pub sphere_radius: f32,
}

impl Default for FBoxSphereBounds {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            box_extent: Vec3::splat(100.0),
            sphere_radius: 100.0,
        }
    }
}

/// Material slot assigned to a skeletal mesh section.
#[derive(Clone, Debug, PartialEq)]
pub struct FSkelMaterial {
    pub material_index: u32,
    pub material_name: String,
}

/// Renderable mesh partition sharing a common material and bone palette.
#[derive(Clone, Debug, PartialEq)]
pub struct FSkelMeshSection {
    pub material_index: u16,
    pub first_index: u32,
    pub num_triangles: u32,
    pub min_stream_index: u32,
    pub max_stream_index: u32,
    pub bone_map: Vec<u16>,
}

/// Single dynamic skinned vertex with up to 4 bone influences.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FSoftVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
    pub bone_indices: [u8; 4],
    pub bone_weights: [f32; 4],
}

impl FSoftVertex {
    pub fn to_vertex_input(&self) -> VertexInput {
        VertexInput {
            position: self.position.to_array(),
            normal: self.normal.to_array(),
            uv: self.uv.to_array(),
            bones: [
                self.bone_indices[0] as u32,
                self.bone_indices[1] as u32,
                self.bone_indices[2] as u32,
                self.bone_indices[3] as u32,
            ],
            weights: self.bone_weights,
        }
    }
}

/// Level of Detail (LOD) model containing partitioned sections, vertex buffers and triangle indices.
#[derive(Clone, Debug, PartialEq)]
pub struct FSkelMeshLODModel {
    pub sections: Vec<FSkelMeshSection>,
    pub vertices: Vec<FSoftVertex>,
    pub raw_indices: Vec<u16>,
    pub num_bones: i32,
}

impl FSkelMeshLODModel {
    /// Converts all vertices in this LOD model directly into GPU compute shader `VertexInput` structures.
    pub fn to_vertex_inputs(&self) -> Vec<VertexInput> {
        self.vertices.iter().map(|v| v.to_vertex_input()).collect()
    }
}

/// Complete retail Unreal Engine 2 `USkeletalMesh` representation (disassembled from `Engine.dll:0x104f2eb0`).
#[derive(Clone, Debug, PartialEq)]
pub struct USkeletalMesh {
    /// Reference bind-pose skeleton containing full hierarchy and 48-byte bone records
    pub ref_skeleton: Skeleton,
    /// LOD models from LOD0 (full fidelity) to simplified LODs
    pub lod_models: Vec<FSkelMeshLODModel>,
    /// Material slots
    pub materials: Vec<FSkelMaterial>,
    /// Bounding box and sphere for collision and culling
    pub bounds: FBoxSphereBounds,
    /// Optional default animation set name
    pub default_anim_name: Option<String>,
}

impl USkeletalMesh {
    pub fn new(ref_skeleton: Skeleton) -> Self {
        Self {
            ref_skeleton,
            lod_models: Vec::new(),
            materials: Vec::new(),
            bounds: FBoxSphereBounds::default(),
            default_anim_name: None,
        }
    }

    /// Serializes the skeletal mesh into OpenKF binary format.
    pub fn to_bytes(&self) -> Result<Vec<u8>, AnimationError> {
        let mut buf = Vec::new();
        // Magic & Version
        buf.write_all(&USKELMESH_MAGIC)?;
        buf.write_all(&USKELMESH_VERSION.to_le_bytes())?;

        // Bounds
        buf.write_all(&self.bounds.origin.x.to_le_bytes())?;
        buf.write_all(&self.bounds.origin.y.to_le_bytes())?;
        buf.write_all(&self.bounds.origin.z.to_le_bytes())?;
        buf.write_all(&self.bounds.box_extent.x.to_le_bytes())?;
        buf.write_all(&self.bounds.box_extent.y.to_le_bytes())?;
        buf.write_all(&self.bounds.box_extent.z.to_le_bytes())?;
        buf.write_all(&self.bounds.sphere_radius.to_le_bytes())?;

        // Default anim name
        match &self.default_anim_name {
            Some(name) => {
                let name_bytes = name.as_bytes();
                buf.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
                buf.write_all(name_bytes)?;
            }
            None => {
                buf.write_all(&0u32.to_le_bytes())?;
            }
        }

        // RefSkeleton
        buf.write_all(&(self.ref_skeleton.bones.len() as u32).to_le_bytes())?;
        for (i, bone) in self.ref_skeleton.bones.iter().enumerate() {
            let name_bytes = bone.name.as_bytes();
            buf.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
            buf.write_all(name_bytes)?;

            let binary = bone.to_binary(i as i32, 0);
            let bytes = bytemuck::bytes_of(&binary);
            buf.write_all(bytes)?;
        }

        // Materials
        buf.write_all(&(self.materials.len() as u32).to_le_bytes())?;
        for mat in &self.materials {
            buf.write_all(&mat.material_index.to_le_bytes())?;
            let name_bytes = mat.material_name.as_bytes();
            buf.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
            buf.write_all(name_bytes)?;
        }

        // LODModels
        buf.write_all(&(self.lod_models.len() as u32).to_le_bytes())?;
        for lod in &self.lod_models {
            buf.write_all(&lod.num_bones.to_le_bytes())?;

            // Sections
            buf.write_all(&(lod.sections.len() as u32).to_le_bytes())?;
            for sec in &lod.sections {
                buf.write_all(&sec.material_index.to_le_bytes())?;
                buf.write_all(&sec.first_index.to_le_bytes())?;
                buf.write_all(&sec.num_triangles.to_le_bytes())?;
                buf.write_all(&sec.min_stream_index.to_le_bytes())?;
                buf.write_all(&sec.max_stream_index.to_le_bytes())?;
                buf.write_all(&(sec.bone_map.len() as u32).to_le_bytes())?;
                for &b in &sec.bone_map {
                    buf.write_all(&b.to_le_bytes())?;
                }
            }

            // Raw indices
            buf.write_all(&(lod.raw_indices.len() as u32).to_le_bytes())?;
            for &idx in &lod.raw_indices {
                buf.write_all(&idx.to_le_bytes())?;
            }

            // Vertices
            buf.write_all(&(lod.vertices.len() as u32).to_le_bytes())?;
            for v in &lod.vertices {
                buf.write_all(&v.position.x.to_le_bytes())?;
                buf.write_all(&v.position.y.to_le_bytes())?;
                buf.write_all(&v.position.z.to_le_bytes())?;
                buf.write_all(&v.normal.x.to_le_bytes())?;
                buf.write_all(&v.normal.y.to_le_bytes())?;
                buf.write_all(&v.normal.z.to_le_bytes())?;
                buf.write_all(&v.uv.x.to_le_bytes())?;
                buf.write_all(&v.uv.y.to_le_bytes())?;
                buf.write_all(&v.bone_indices)?;
                for &w in &v.bone_weights {
                    buf.write_all(&w.to_le_bytes())?;
                }
            }
        }

        Ok(buf)
    }

    /// Deserializes a skeletal mesh from binary bytes.
    pub fn from_bytes(slice: &[u8]) -> Result<Self, AnimationError> {
        let mut cur = Cursor::new(slice);

        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic)?;
        if magic != USKELMESH_MAGIC {
            return Err(AnimationError::InvalidMagic {
                expected: USKELMESH_MAGIC,
                got: magic,
            });
        }

        let mut v_bytes = [0u8; 4];
        cur.read_exact(&mut v_bytes)?;
        let version = u32::from_le_bytes(v_bytes);
        if version != USKELMESH_VERSION {
            return Err(AnimationError::InvalidVersion {
                expected: USKELMESH_VERSION,
                got: version,
            });
        }

        // Bounds
        let mut f_bytes = [0u8; 4];
        let read_f32 = |cur: &mut Cursor<&[u8]>, f_bytes: &mut [u8; 4]| -> Result<f32, AnimationError> {
            cur.read_exact(f_bytes)?;
            Ok(f32::from_le_bytes(*f_bytes))
        };

        let ox = read_f32(&mut cur, &mut f_bytes)?;
        let oy = read_f32(&mut cur, &mut f_bytes)?;
        let oz = read_f32(&mut cur, &mut f_bytes)?;
        let ex = read_f32(&mut cur, &mut f_bytes)?;
        let ey = read_f32(&mut cur, &mut f_bytes)?;
        let ez = read_f32(&mut cur, &mut f_bytes)?;
        let radius = read_f32(&mut cur, &mut f_bytes)?;
        let bounds = FBoxSphereBounds {
            origin: Vec3::new(ox, oy, oz),
            box_extent: Vec3::new(ex, ey, ez),
            sphere_radius: radius,
        };

        // Default anim name
        cur.read_exact(&mut v_bytes)?;
        let anim_name_len = u32::from_le_bytes(v_bytes) as usize;
        let default_anim_name = if anim_name_len > 0 {
            let mut name_buf = vec![0u8; anim_name_len];
            cur.read_exact(&mut name_buf)?;
            Some(String::from_utf8_lossy(&name_buf).to_string())
        } else {
            None
        };

        // RefSkeleton — hard cap prevents OOM from corrupted 0*2-style FArray (retail GPF: FArray::Realloc 0*2)
        cur.read_exact(&mut v_bytes)?;
        let bone_count = u32::from_le_bytes(v_bytes) as usize;
        if bone_count > 1024 { return Err(AnimationError::InvalidVersion { expected: 0, got: bone_count as u32 }); }
        let mut skeleton = Skeleton::with_capacity(bone_count);

        for _ in 0..bone_count {
            cur.read_exact(&mut v_bytes)?;
            let name_len = u32::from_le_bytes(v_bytes) as usize;
            let mut name_buf = vec![0u8; name_len];
            cur.read_exact(&mut name_buf)?;
            let bone_name = String::from_utf8_lossy(&name_buf).to_string();

            let mut bone_raw = [0u8; std::mem::size_of::<FBoneBinary>()];
            cur.read_exact(&mut bone_raw)?;
            let binary: FBoneBinary = *bytemuck::from_bytes(&bone_raw);
            skeleton.add_bone(FBone::from_binary(&binary, bone_name));
        }

        // Materials — cap 128 (retail max ~16; 128 guards against FArray corruption)
        cur.read_exact(&mut v_bytes)?;
        let mat_count = u32::from_le_bytes(v_bytes) as usize;
        if mat_count > 128 { return Err(AnimationError::InvalidVersion { expected: 0, got: mat_count as u32 }); }
        let mut materials = Vec::with_capacity(mat_count);
        for _ in 0..mat_count {
            cur.read_exact(&mut v_bytes)?;
            let mat_idx = u32::from_le_bytes(v_bytes);

            cur.read_exact(&mut v_bytes)?;
            let name_len = u32::from_le_bytes(v_bytes) as usize;
            let mut name_buf = vec![0u8; name_len];
            cur.read_exact(&mut name_buf)?;
            let mat_name = String::from_utf8_lossy(&name_buf).to_string();

            materials.push(FSkelMaterial {
                material_index: mat_idx,
                material_name: mat_name,
            });
        }

        // LODModels — cap 8 (retail 1-3; 8 covers worst case without OOM)
        cur.read_exact(&mut v_bytes)?;
        let lod_count = u32::from_le_bytes(v_bytes) as usize;
        if lod_count > 8 { return Err(AnimationError::InvalidVersion { expected: 0, got: lod_count as u32 }); }
        let mut lod_models = Vec::with_capacity(lod_count);

        for _ in 0..lod_count {
            let mut i_bytes = [0u8; 4];
            cur.read_exact(&mut i_bytes)?;
            let num_bones = i32::from_le_bytes(i_bytes);

            // Sections — cap 64
            cur.read_exact(&mut v_bytes)?;
            let sec_count = u32::from_le_bytes(v_bytes) as usize;
            if sec_count > 64 { return Err(AnimationError::InvalidVersion { expected: 0, got: sec_count as u32 }); }
            let mut sections = Vec::with_capacity(sec_count);
            for _ in 0..sec_count {
                let mut u16_bytes = [0u8; 2];
                cur.read_exact(&mut u16_bytes)?;
                let material_index = u16::from_le_bytes(u16_bytes);

                cur.read_exact(&mut v_bytes)?;
                let first_index = u32::from_le_bytes(v_bytes);

                cur.read_exact(&mut v_bytes)?;
                let num_triangles = u32::from_le_bytes(v_bytes);

                cur.read_exact(&mut v_bytes)?;
                let min_stream_index = u32::from_le_bytes(v_bytes);

                cur.read_exact(&mut v_bytes)?;
                let max_stream_index = u32::from_le_bytes(v_bytes);

                cur.read_exact(&mut v_bytes)?;
                let bone_map_count = u32::from_le_bytes(v_bytes) as usize;
                let mut bone_map = Vec::with_capacity(bone_map_count);
                for _ in 0..bone_map_count {
                    cur.read_exact(&mut u16_bytes)?;
                    bone_map.push(u16::from_le_bytes(u16_bytes));
                }

                sections.push(FSkelMeshSection {
                    material_index,
                    first_index,
                    num_triangles,
                    min_stream_index,
                    max_stream_index,
                    bone_map,
                });
            }

            // Raw indices — cap 500k (retail <50k; 0*2 GPF guard)
            cur.read_exact(&mut v_bytes)?;
            let idx_count = u32::from_le_bytes(v_bytes) as usize;
            if idx_count > 500_000 { return Err(AnimationError::InvalidVersion { expected: 0, got: idx_count as u32 }); }
            let mut raw_indices = Vec::with_capacity(idx_count);
            let mut u16_bytes = [0u8; 2];
            for _ in 0..idx_count {
                cur.read_exact(&mut u16_bytes)?;
                raw_indices.push(u16::from_le_bytes(u16_bytes));
            }

            // Vertices — cap 200k
            cur.read_exact(&mut v_bytes)?;
            let vert_count = u32::from_le_bytes(v_bytes) as usize;
            if vert_count > 200_000 { return Err(AnimationError::InvalidVersion { expected: 0, got: vert_count as u32 }); }
            let mut vertices = Vec::with_capacity(vert_count);
            for _ in 0..vert_count {
                let px = read_f32(&mut cur, &mut f_bytes)?;
                let py = read_f32(&mut cur, &mut f_bytes)?;
                let pz = read_f32(&mut cur, &mut f_bytes)?;
                let nx = read_f32(&mut cur, &mut f_bytes)?;
                let ny = read_f32(&mut cur, &mut f_bytes)?;
                let nz = read_f32(&mut cur, &mut f_bytes)?;
                let u = read_f32(&mut cur, &mut f_bytes)?;
                let v = read_f32(&mut cur, &mut f_bytes)?;

                let mut bone_indices = [0u8; 4];
                cur.read_exact(&mut bone_indices)?;

                let mut bone_weights = [0.0f32; 4];
                for w in &mut bone_weights {
                    *w = read_f32(&mut cur, &mut f_bytes)?;
                }

                vertices.push(FSoftVertex {
                    position: Vec3::new(px, py, pz),
                    normal: Vec3::new(nx, ny, nz),
                    uv: Vec2::new(u, v),
                    bone_indices,
                    bone_weights,
                });
            }

            lod_models.push(FSkelMeshLODModel {
                sections,
                vertices,
                raw_indices,
                num_bones,
            });
        }

        Ok(Self {
            ref_skeleton: skeleton,
            lod_models,
            materials,
            bounds,
            default_anim_name,
        })
    }
}
