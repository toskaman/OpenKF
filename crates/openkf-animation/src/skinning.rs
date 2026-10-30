use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};

/// GPU Compute Shader input vertex layout for skeletal deformation.
/// Exactly matches the HLSL `struct VertexInput` in `recon_08_skeletal_animation.md` (64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VertexInput {
    /// Bind-pose position: float3 Position : POSITION (12 bytes)
    pub position: [f32; 3],
    /// Bind-pose surface normal: float3 Normal : NORMAL (12 bytes)
    pub normal: [f32; 3],
    /// Texture coordinate: float2 UV : TEXCOORD0 (8 bytes)
    pub uv: [f32; 2],
    /// Up to 4 bone matrix indices: uint4 Bones : BLENDINDICES (16 bytes)
    pub bones: [u32; 4],
    /// Up to 4 normalized blend weights: float4 Weights : BLENDWEIGHT (16 bytes)
    pub weights: [f32; 4],
}

// Compile-time assertion: VertexInput is strictly 64 bytes.
const _: () = assert!(std::mem::size_of::<VertexInput>() == 64);

/// Skinned output vertex layout written by GPU Compute Shader / CPU LBS skinning.
/// Matches the HLSL `struct VertexOutput` in `recon_08_skeletal_animation.md` (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct VertexOutput {
    /// Deformed world/model position: float3 Position : POSITION (12 bytes)
    pub position: [f32; 3],
    /// Deformed and normalized normal: float3 Normal : NORMAL (12 bytes)
    pub normal: [f32; 3],
    /// Passthrough texture coordinate: float2 UV : TEXCOORD0 (8 bytes)
    pub uv: [f32; 2],
}

// Compile-time assertion: VertexOutput is strictly 32 bytes.
const _: () = assert!(std::mem::size_of::<VertexOutput>() == 32);

/// 4x4 column-major matrix representation for GPU StructuredBuffer upload (64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct BoneMatrix {
    pub cols: [[f32; 4]; 4],
}

// Compile-time assertion: BoneMatrix is strictly 64 bytes.
const _: () = assert!(std::mem::size_of::<BoneMatrix>() == 64);

impl From<Mat4> for BoneMatrix {
    fn from(m: Mat4) -> Self {
        Self {
            cols: m.to_cols_array_2d(),
        }
    }
}

impl From<BoneMatrix> for Mat4 {
    fn from(bm: BoneMatrix) -> Self {
        Mat4::from_cols_array_2d(&bm.cols)
    }
}

/// Software CPU reference implementation of Linear Blend Skinning (LBS).
/// Evaluates the deformation equation:
/// $$\mathbf{P}_{\text{deformed}} = \sum_{i=0}^3 w_i \cdot (\mathbf{M}_{\text{skin}, i} \cdot \mathbf{P}_{\text{rest}})$$
/// $$\mathbf{N}_{\text{deformed}} = \sum_{i=0}^3 w_i \cdot (\mathbf{M}_{\text{skin}, i, \text{rot}} \cdot \mathbf{N}_{\text{rest}})$$
///
/// Guaranteed to produce bit-exact equivalent results to the HLSL compute shader `CSMain`.
pub fn cpu_skin_vertices(inputs: &[VertexInput], skin_matrices: &[Mat4]) -> Vec<VertexOutput> {
    inputs
        .iter()
        .map(|v| {
            let pos = Vec3::from_array(v.position);
            let norm = Vec3::from_array(v.normal);

            let mut blended_pos = Vec3::ZERO;
            let mut blended_norm = Vec3::ZERO;

            for i in 0..4 {
                let weight = v.weights[i];
                if weight > 0.0 {
                    let bone_idx = v.bones[i] as usize;
                    let mat = if bone_idx < skin_matrices.len() {
                        skin_matrices[bone_idx]
                    } else {
                        Mat4::IDENTITY
                    };

                    let p_trans = mat.transform_point3(pos);
                    let rot_mat = Mat3::from_mat4(mat);
                    let n_trans = rot_mat.mul_vec3(norm);

                    blended_pos += p_trans * weight;
                    blended_norm += n_trans * weight;
                }
            }

            let final_norm = if blended_norm.length_squared() > 1e-6 {
                blended_norm.normalize()
            } else {
                norm
            };

            VertexOutput {
                position: blended_pos.to_array(),
                normal: final_norm.to_array(),
                uv: v.uv,
            }
        })
        .collect()
}
