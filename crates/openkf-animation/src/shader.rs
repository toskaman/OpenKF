/// HLSL Compute Shader (Direct3D 11/12 / Vulkan via DXC/glslang) for GPU Skeletal Skinning.
/// Direct implementation of Section 4 in `recon_08_skeletal_animation.md`.
pub const COMPUTE_SKINNING_HLSL: &str = r#"
struct VertexInput {
    float3 Position : POSITION;
    float3 Normal   : NORMAL;
    float2 UV       : TEXCOORD0;
    uint4  Bones    : BLENDINDICES;
    float4 Weights  : BLENDWEIGHT;
};

struct VertexOutput {
    float3 Position : POSITION;
    float3 Normal   : NORMAL;
    float2 UV       : TEXCOORD0;
};

StructuredBuffer<VertexInput> InputVertices   : register(t0);
StructuredBuffer<float4x4>    BoneMatrices    : register(t1);
RWStructuredBuffer<VertexOutput> OutputBuffer : register(u0);

[numthreads(64, 1, 1)]
void CSMain(uint3 DispatchThreadID : SV_DispatchThreadID) {
    uint idx = DispatchThreadID.x;
    VertexInput v = InputVertices[idx];

    float4x4 skinMatrix = 
        BoneMatrices[v.Bones.x] * v.Weights.x +
        BoneMatrices[v.Bones.y] * v.Weights.y +
        BoneMatrices[v.Bones.z] * v.Weights.z +
        BoneMatrices[v.Bones.w] * v.Weights.w;

    float4 skinnedPos = mul(skinMatrix, float4(v.Position, 1.0));
    float3 skinnedNorm = mul((float3x3)skinMatrix, v.Normal);

    OutputBuffer[idx].Position = skinnedPos.xyz;
    OutputBuffer[idx].Normal   = normalize(skinnedNorm);
    OutputBuffer[idx].UV       = v.UV;
}
"#;

/// WebGPU Shading Language (WGSL) variant for modern Vulkan/Metal/DirectX 12 backends.
pub const COMPUTE_SKINNING_WGSL: &str = r#"
struct VertexInput {
    position: vec3<f32>,
    _pad0: f32,
    normal: vec3<f32>,
    _pad1: f32,
    uv: vec2<f32>,
    _pad2: vec2<f32>,
    bones: vec4<u32>,
    weights: vec4<f32>,
};

struct VertexOutput {
    position: vec3<f32>,
    _pad0: f32,
    normal: vec3<f32>,
    _pad1: f32,
    uv: vec2<f32>,
    _pad2: vec2<f32>,
};

@group(0) @binding(0) var<storage, read> input_vertices: array<VertexInput>;
@group(0) @binding(1) var<storage, read> bone_matrices: array<mat4x4<f32>>;
@group(0) @binding(2) var<storage, read_write> output_vertices: array<VertexOutput>;

@compute @workgroup_size(64, 1, 1)
fn cs_main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    if (idx >= arrayLength(&input_vertices)) {
        return;
    }

    let v = input_vertices[idx];

    let skin_matrix = 
        bone_matrices[v.bones.x] * v.weights.x +
        bone_matrices[v.bones.y] * v.weights.y +
        bone_matrices[v.bones.z] * v.weights.z +
        bone_matrices[v.bones.w] * v.weights.w;

    let skinned_pos = skin_matrix * vec4<f32>(v.position, 1.0);
    let skin_mat3 = mat3x3<f32>(skin_matrix[0].xyz, skin_matrix[1].xyz, skin_matrix[2].xyz);
    let skinned_norm = normalize(skin_mat3 * v.normal);

    output_vertices[idx].position = skinned_pos.xyz;
    output_vertices[idx].normal = skinned_norm;
    output_vertices[idx].uv = v.uv;
}
"#;
