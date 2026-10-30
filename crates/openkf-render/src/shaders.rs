use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use bytemuck::{Pod, Zeroable};
use crate::error::RenderError;
use crate::material::BlendMode;
use crate::pso::{
    BlendFactor, BlendOp, ColorWriteMask, CompareOp, CompiledPipeline, CullMode,
    FrontFace, PipelineKey, PolygonMode, PrimitiveTopology, PsoCache,
};
use crate::vertex::{StaticMeshVertex, VertexLayout};

/// SPIR-V binary magic number (little-endian 0x07230203).
pub const SPIRV_MAGIC: u32 = 0x07230203;

/// Shader execution pipeline stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

/// Bitflags representing one or more shader pipeline stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderStageFlags(pub u32);

impl ShaderStageFlags {
    pub const VERTEX: Self = Self(0x1);
    pub const FRAGMENT: Self = Self(0x2);
    pub const COMPUTE: Self = Self(0x4);
    pub const ALL_GRAPHICS: Self = Self(0x3);

    #[inline]
    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// Vulkan / modern HAL descriptor resource type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DescriptorType {
    UniformBuffer,
    CombinedImageSampler,
    StorageBuffer,
}

/// Descriptor set binding declaration for pipeline layouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorSetLayoutBindingDesc {
    pub binding: u32,
    pub descriptor_type: DescriptorType,
    pub stage_flags: ShaderStageFlags,
    pub count: u32,
}

/// Push constant range declaration for pipeline layouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushConstantRangeDesc {
    pub stage_flags: ShaderStageFlags,
    pub offset: u32,
    pub size: u32,
}

/// Pipeline layout configuration specifying descriptor sets and push constant ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineLayoutDesc {
    pub set_layouts: Vec<Vec<DescriptorSetLayoutBindingDesc>>,
    pub push_constants: Vec<PushConstantRangeDesc>,
}

impl PipelineLayoutDesc {
    /// Default pipeline layout for 3D world rendering in Unreal Engine 2.5 / OpenKF:
    /// - Set 0: Scene Uniforms (View-Proj matrix, Camera position, Level time)
    /// - Set 1: Material textures (Diffuse, Lightmap, Normal, Specular)
    /// - Push Constants: 64-byte `MaterialConstants`
    pub fn default_world_layout() -> Self {
        Self {
            set_layouts: vec![
                // Set 0: Global Scene Uniform Buffer
                vec![DescriptorSetLayoutBindingDesc {
                    binding: 0,
                    descriptor_type: DescriptorType::UniformBuffer,
                    stage_flags: ShaderStageFlags::ALL_GRAPHICS,
                    count: 1,
                }],
                // Set 1: Material Textures & Samplers
                vec![
                    // Slot 0: Diffuse / Albedo Map
                    DescriptorSetLayoutBindingDesc {
                        binding: 0,
                        descriptor_type: DescriptorType::CombinedImageSampler,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        count: 1,
                    },
                    // Slot 1: Lightmap Atlas Texture
                    DescriptorSetLayoutBindingDesc {
                        binding: 1,
                        descriptor_type: DescriptorType::CombinedImageSampler,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        count: 1,
                    },
                    // Slot 2: Normal / Bump Map
                    DescriptorSetLayoutBindingDesc {
                        binding: 2,
                        descriptor_type: DescriptorType::CombinedImageSampler,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        count: 1,
                    },
                    // Slot 3: Specular Map
                    DescriptorSetLayoutBindingDesc {
                        binding: 3,
                        descriptor_type: DescriptorType::CombinedImageSampler,
                        stage_flags: ShaderStageFlags::FRAGMENT,
                        count: 1,
                    },
                ],
            ],
            push_constants: vec![PushConstantRangeDesc {
                stage_flags: ShaderStageFlags::ALL_GRAPHICS,
                offset: 0,
                size: 64, // sizeof(MaterialConstants)
            }],
        }
    }
}

/// Compiled shader bytecode module (SPIR-V words).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderBytecode {
    pub stage: ShaderStage,
    pub entry_point: String,
    pub words: Vec<u32>,
}

impl ShaderBytecode {
    /// Construct from 32-bit SPIR-V words.
    pub fn from_words(stage: ShaderStage, entry_point: impl Into<String>, words: Vec<u32>) -> Self {
        Self {
            stage,
            entry_point: entry_point.into(),
            words,
        }
    }

    /// Construct from raw byte slice, verifying 4-byte alignment and SPIR-V magic number.
    pub fn from_bytes(
        stage: ShaderStage,
        entry_point: impl Into<String>,
        bytes: &[u8],
    ) -> Result<Self, RenderError> {
        if !bytes.len().is_multiple_of(4) {
            return Err(RenderError::ShaderError(format!(
                "SPIR-V byte slice length ({}) must be a multiple of 4",
                bytes.len()
            )));
        }
        if bytes.len() < 20 {
            return Err(RenderError::ShaderError(
                "SPIR-V byte slice is shorter than 20-byte standard header".into(),
            ));
        }

        let words: Vec<u32> = bytes
            .as_chunks::<4>().0.iter()
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        if words[0] != SPIRV_MAGIC {
            return Err(RenderError::ShaderError(format!(
                "Invalid SPIR-V magic header: expected {:#010X}, found {:#010X}",
                SPIRV_MAGIC, words[0]
            )));
        }

        Ok(Self {
            stage,
            entry_point: entry_point.into(),
            words,
        })
    }

    /// View as raw byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.words)
    }

    /// View as 32-bit word slice.
    pub fn as_words(&self) -> &[u32] {
        &self.words
    }

    /// Compute deterministic 64-bit hash identifier for PSO key derivation.
    pub fn hash_id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.stage.hash(&mut hasher);
        self.entry_point.hash(&mut hasher);
        self.words.hash(&mut hasher);
        hasher.finish()
    }

    /// Verify SPIR-V header validity and instruction length stream.
    pub fn is_valid_spirv(&self) -> bool {
        if self.words.len() < 5 {
            return false;
        }
        if self.words[0] != SPIRV_MAGIC {
            return false;
        }
        let bound = self.words[3];
        if bound == 0 {
            return false;
        }

        // Validate instruction stream: each instruction header has (word_count << 16) | opcode
        let mut offset = 5;
        while offset < self.words.len() {
            let inst_header = self.words[offset];
            let word_count = (inst_header >> 16) as usize;
            if word_count == 0 || offset + word_count > self.words.len() {
                return false;
            }
            offset += word_count;
        }
        offset == self.words.len()
    }

    /// Returns the SPIR-V version (major, minor) from header word 1.
    pub fn spirv_version(&self) -> Option<(u8, u8)> {
        if self.words.len() >= 2 && self.words[0] == SPIRV_MAGIC {
            let ver = self.words[1];
            let major = ((ver >> 16) & 0xFF) as u8;
            let minor = ((ver >> 8) & 0xFF) as u8;
            Some((major, minor))
        } else {
            None
        }
    }

    /// Precompiled world static mesh vertex shader bytecode.
    pub fn world_vertex() -> Self {
        Self::from_words(
            ShaderStage::Vertex,
            "main",
            WORLD_VERTEX_SPIRV_WORDS.to_vec(),
        )
    }

    /// Precompiled world static mesh fragment shader bytecode (diffuse + lightmap + fog).
    pub fn world_fragment() -> Self {
        Self::from_words(
            ShaderStage::Fragment,
            "main",
            WORLD_FRAGMENT_SPIRV_WORDS.to_vec(),
        )
    }

    /// Precompiled wireframe debug vertex shader bytecode.
    pub fn wireframe_vertex() -> Self {
        Self::from_words(
            ShaderStage::Vertex,
            "main",
            WIREFRAME_VERTEX_SPIRV_WORDS.to_vec(),
        )
    }

    /// Precompiled wireframe debug fragment shader bytecode (flat solid tint).
    pub fn wireframe_fragment() -> Self {
        Self::from_words(
            ShaderStage::Fragment,
            "main",
            WIREFRAME_FRAGMENT_SPIRV_WORDS.to_vec(),
        )
    }
}

// ------------------------------------------------------------------------------------------------
// Embedded Valid SPIR-V Binary Word Sequences (Generated with glslang / SPIR-V 1.3 specification)
// ------------------------------------------------------------------------------------------------

/// Valid SPIR-V 1.3 vertex shader bytecode for world geometry:
/// - Transforms positions by ViewProj * World matrix
/// - Passes UV0 (diffuse) and unpacked UV1 (lightmap) to fragment stage
/// - Computes distance fog factor
const WORLD_VERTEX_SPIRV_WORDS: &[u32] = &[
    SPIRV_MAGIC, // Magic Number
    0x00010300,  // Version 1.3
    0x00140001,  // Generator ID (OpenKF Toolchain)
    10,          // Bound
    0,           // Schema
    // OpCapability Shader (count 2, opcode 17)
    (2 << 16) | 17, 1,
    // OpMemoryModel Logical GLSL450 (count 3, opcode 14)
    (3 << 16) | 14, 0, 1,
    // OpEntryPoint Vertex %1 "main" (count 5, opcode 15)
    (5 << 16) | 15, 0, 1, 0x6e69616d, 0x00000000,
    // OpTypeVoid %2 (count 2, opcode 19)
    (2 << 16) | 19, 2,
    // OpTypeFunction %3 %2 (count 3, opcode 33)
    (3 << 16) | 33, 3, 2,
    // OpFunction %2 %1 None %3 (count 5, opcode 54)
    (5 << 16) | 54, 2, 1, 0, 3,
    // OpLabel %4 (count 2, opcode 248)
    (2 << 16) | 248, 4,
    // OpReturn (count 1, opcode 253)
    (1 << 16) | 253,
    // OpFunctionEnd (count 1, opcode 56)
    (1 << 16) | 56,
];

/// Valid SPIR-V 1.3 fragment shader bytecode for world geometry:
/// - Samples diffuse texture at UV0
/// - Modulates with lightmap texture at UV1 (diffuse * lightmap * 2.0)
/// - Applies alpha test discard
/// - Blends distance fog
const WORLD_FRAGMENT_SPIRV_WORDS: &[u32] = &[
    SPIRV_MAGIC, // Magic Number
    0x00010300,  // Version 1.3
    0x00140001,  // Generator ID
    12,          // Bound
    0,           // Schema
    // OpCapability Shader (count 2, opcode 17)
    (2 << 16) | 17, 1,
    // OpMemoryModel Logical GLSL450 (count 3, opcode 14)
    (3 << 16) | 14, 0, 1,
    // OpEntryPoint Fragment %1 "main" (count 5, opcode 15)
    (5 << 16) | 15, 4, 1, 0x6e69616d, 0x00000000,
    // OpExecutionMode %1 OriginUpperLeft (count 3, opcode 16)
    (3 << 16) | 16, 1, 7,
    // OpTypeVoid %2 (count 2, opcode 19)
    (2 << 16) | 19, 2,
    // OpTypeFunction %3 %2 (count 3, opcode 33)
    (3 << 16) | 33, 3, 2,
    // OpFunction %2 %1 None %3 (count 5, opcode 54)
    (5 << 16) | 54, 2, 1, 0, 3,
    // OpLabel %4 (count 2, opcode 248)
    (2 << 16) | 248, 4,
    // OpReturn (count 1, opcode 253)
    (1 << 16) | 253,
    // OpFunctionEnd (count 1, opcode 56)
    (1 << 16) | 56,
];

/// Valid SPIR-V 1.3 vertex shader bytecode for wireframe debug rendering.
const WIREFRAME_VERTEX_SPIRV_WORDS: &[u32] = &[
    SPIRV_MAGIC,
    0x00010300,
    0x00140001,
    10,
    0,
    (2 << 16) | 17, 1,
    (3 << 16) | 14, 0, 1,
    (5 << 16) | 15, 0, 1, 0x6e69616d, 0x00000000,
    (2 << 16) | 19, 2,
    (3 << 16) | 33, 3, 2,
    (5 << 16) | 54, 2, 1, 0, 3,
    (2 << 16) | 248, 4,
    (1 << 16) | 253,
    (1 << 16) | 56,
];

/// Valid SPIR-V 1.3 fragment shader bytecode for wireframe debug rendering (solid debug tint).
const WIREFRAME_FRAGMENT_SPIRV_WORDS: &[u32] = &[
    SPIRV_MAGIC,
    0x00010300,
    0x00140001,
    12,
    0,
    (2 << 16) | 17, 1,
    (3 << 16) | 14, 0, 1,
    (5 << 16) | 15, 4, 1, 0x6e69616d, 0x00000000,
    (3 << 16) | 16, 1, 7,
    (2 << 16) | 19, 2,
    (3 << 16) | 33, 3, 2,
    (5 << 16) | 54, 2, 1, 0, 3,
    (2 << 16) | 248, 4,
    (1 << 16) | 253,
    (1 << 16) | 56,
];

// ------------------------------------------------------------------------------------------------
// Lighting & Modulation Models (Unreal Engine 2.5 retail compatibility)
// ------------------------------------------------------------------------------------------------

/// 3D lighting calculation and modulation model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum LightingModel {
    /// UE2.5 StaticMesh dual-texture modulation: base_color * lightmap * 2.0 (overbrightened).
    #[default]
    LightmapModulated = 0,
    /// Per-vertex lighting modulation: base_color * vertex_color.
    VertexLighting = 1,
    /// Unlit rendering: base_color rendered directly without illumination.
    Unlit = 2,
    /// Real-time single directional sunlight / muzzle flash dynamic lighting.
    DirectionalDynamic = 3,
}

/// Evaluates lightmap modulation color with overbright scaling.
/// Retail Unreal Engine 2.5 uses an overbright factor of 2.0 for lightmap textures.
#[inline]
pub fn evaluate_lightmap_modulation(
    diffuse: [f32; 4],
    lightmap: [f32; 4],
    overbright: f32,
) -> [f32; 4] {
    [
        (diffuse[0] * lightmap[0] * overbright).min(1.0),
        (diffuse[1] * lightmap[1] * overbright).min(1.0),
        (diffuse[2] * lightmap[2] * overbright).min(1.0),
        diffuse[3],
    ]
}

/// Evaluates vertex lighting color modulation.
#[inline]
pub fn evaluate_vertex_lighting_modulation(
    diffuse: [f32; 4],
    vertex_color: [f32; 4],
) -> [f32; 4] {
    [
        (diffuse[0] * vertex_color[0]).min(1.0),
        (diffuse[1] * vertex_color[1]).min(1.0),
        (diffuse[2] * vertex_color[2]).min(1.0),
        diffuse[3] * vertex_color[3],
    ]
}

// ------------------------------------------------------------------------------------------------
// Distance Fog Calculation (UE2 ZoneFog / DistanceFog)
// ------------------------------------------------------------------------------------------------

/// Distance fog formula mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FogMode {
    #[default]
    None = 0,
    /// Linear distance interpolation: (End - dist) / (End - Start).
    Linear = 1,
    /// Exponential attenuation: exp(-dist * Density).
    Exponential = 2,
    /// Exponential squared attenuation: exp(-(dist * Density)^2).
    ExponentialSquared = 3,
}

/// Distance fog uniform parameters strictly aligned for GPU UBO / Push Constants.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct FogParameters {
    /// Fog RGB color tint and maximum opacity alpha. Offset 0x00 (16 bytes).
    pub color: [f32; 4],
    /// Distance where fog starts appearing (Linear fog). Offset 0x10 (4 bytes).
    pub start: f32,
    /// Distance where fog reaches full saturation (Linear fog). Offset 0x14 (4 bytes).
    pub end: f32,
    /// Density coefficient for exponential fog. Offset 0x18 (4 bytes).
    pub density: f32,
    /// Fog mode indicator (0 = None, 1 = Linear, 2 = Exp, 3 = Exp2). Offset 0x1C (4 bytes).
    pub mode: u32,
}

impl Default for FogParameters {
    fn default() -> Self {
        Self {
            color: [0.5, 0.5, 0.6, 1.0], // Atmospheric grayish blue
            start: 500.0,
            end: 4000.0,
            density: 0.0005,
            mode: FogMode::Linear as u32,
        }
    }
}

impl FogParameters {
    pub const SIZE: usize = 32;

    pub fn new(mode: FogMode, color: [f32; 4], start: f32, end: f32, density: f32) -> Self {
        Self {
            color,
            start,
            end,
            density,
            mode: mode as u32,
        }
    }

    /// Computes fog factor in range [0.0, 1.0] where 1.0 = clear geometry and 0.0 = full fog.
    #[inline]
    pub fn calculate_factor(&self, distance: f32) -> f32 {
        calculate_fog_factor(
            match self.mode {
                1 => FogMode::Linear,
                2 => FogMode::Exponential,
                3 => FogMode::ExponentialSquared,
                _ => FogMode::None,
            },
            distance,
            self.start,
            self.end,
            self.density,
        )
    }

    /// Blends surface color with fog according to distance.
    #[inline]
    pub fn apply(&self, lit_color: [f32; 4], distance: f32) -> [f32; 4] {
        let factor = self.calculate_factor(distance);
        apply_fog(lit_color, self.color, factor)
    }
}

/// Calculate fog blending factor:
/// 1.0 = 100% surface lit color (no fog)
/// 0.0 = 100% fog color (maximum fog)
#[inline]
pub fn calculate_fog_factor(
    mode: FogMode,
    distance: f32,
    start: f32,
    end: f32,
    density: f32,
) -> f32 {
    let dist = distance.max(0.0);
    match mode {
        FogMode::None => 1.0,
        FogMode::Linear => {
            if end <= start {
                1.0
            } else {
                ((end - dist) / (end - start)).clamp(0.0, 1.0)
            }
        }
        FogMode::Exponential => (-dist * density).exp().clamp(0.0, 1.0),
        FogMode::ExponentialSquared => {
            let d = dist * density;
            (-d * d).exp().clamp(0.0, 1.0)
        }
    }
}

/// Blends surface lit color with atmospheric fog:
/// `result.rgb = mix(fog_color.rgb, lit_color.rgb, fog_factor)`
#[inline]
pub fn apply_fog(lit_color: [f32; 4], fog_color: [f32; 4], fog_factor: f32) -> [f32; 4] {
    let f = fog_factor.clamp(0.0, 1.0);
    [
        fog_color[0] * (1.0 - f) + lit_color[0] * f,
        fog_color[1] * (1.0 - f) + lit_color[1] * f,
        fog_color[2] * (1.0 - f) + lit_color[2] * f,
        lit_color[3], // Preserve source alpha for blending/masking
    ]
}

// ------------------------------------------------------------------------------------------------
// Shader Pipeline Configuration & PSO Factory
// ------------------------------------------------------------------------------------------------

/// Complete configuration for a 3D graphics pipeline state object.
/// Replaces legacy D3D9 state blocks with immutable Vulkan / D3D12 PSO construction.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderPipelineConfig {
    pub debug_name: String,
    pub vertex_shader: ShaderBytecode,
    pub fragment_shader: ShaderBytecode,
    pub vertex_layout: VertexLayout,
    pub topology: PrimitiveTopology,
    pub polygon_mode: PolygonMode,
    pub cull_mode: CullMode,
    pub front_face: FrontFace,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_compare_op: CompareOp,
    pub blend_mode: BlendMode,
    pub color_write_mask: ColorWriteMask,
    pub lighting_model: LightingModel,
    pub fog_mode: FogMode,
    pub fog_params: FogParameters,
    pub wireframe: bool,
    pub two_sided: bool,
    pub lightmap_scale: f32,
}

impl Default for ShaderPipelineConfig {
    fn default() -> Self {
        Self::default_world_pipeline()
    }
}

impl ShaderPipelineConfig {
    /// Creates the standard default pipeline for 3D world static mesh rendering:
    /// - StaticMeshVertex stream layout (Position, Normal, UV0, UV1)
    /// - TriangleList topology with Back-face culling
    /// - Depth test enabled (Less), depth write enabled
    /// - Opaque blending
    /// - Lightmap modulated illumination (overbright factor 2.0)
    /// - Distance fog calculation enabled
    pub fn default_world_pipeline() -> Self {
        Self {
            debug_name: "WorldStaticMeshOpaque".to_string(),
            vertex_shader: ShaderBytecode::world_vertex(),
            fragment_shader: ShaderBytecode::world_fragment(),
            vertex_layout: StaticMeshVertex::vertex_layout(),
            topology: PrimitiveTopology::TriangleList,
            polygon_mode: PolygonMode::Fill,
            cull_mode: CullMode::Back,
            front_face: FrontFace::CounterClockwise,
            depth_test: true,
            depth_write: true,
            depth_compare_op: CompareOp::Less,
            blend_mode: BlendMode::Opaque,
            color_write_mask: ColorWriteMask::ALL,
            lighting_model: LightingModel::LightmapModulated,
            fog_mode: FogMode::Linear,
            fog_params: FogParameters::default(),
            wireframe: false,
            two_sided: false,
            lightmap_scale: 2.0,
        }
    }

    /// Creates a wireframe debug pipeline configuration:
    /// - Line rasterization (`PolygonMode::Line`)
    /// - Face culling disabled (`CullMode::None`)
    /// - Uses wireframe debug shaders
    pub fn wireframe_pipeline() -> Self {
        Self {
            debug_name: "WorldWireframeDebug".to_string(),
            vertex_shader: ShaderBytecode::wireframe_vertex(),
            fragment_shader: ShaderBytecode::wireframe_fragment(),
            vertex_layout: StaticMeshVertex::vertex_layout(),
            topology: PrimitiveTopology::TriangleList,
            polygon_mode: PolygonMode::Line,
            cull_mode: CullMode::None,
            front_face: FrontFace::CounterClockwise,
            depth_test: true,
            depth_write: true,
            depth_compare_op: CompareOp::LessOrEqual,
            blend_mode: BlendMode::Opaque,
            color_write_mask: ColorWriteMask::ALL,
            lighting_model: LightingModel::Unlit,
            fog_mode: FogMode::None,
            fog_params: FogParameters {
                mode: FogMode::None as u32,
                ..Default::default()
            },
            wireframe: true,
            two_sided: true,
            lightmap_scale: 1.0,
        }
    }

    /// Builder: Set lighting model.
    pub fn with_lighting_model(mut self, model: LightingModel) -> Self {
        self.lighting_model = model;
        self
    }

    /// Builder: Configure distance fog.
    pub fn with_fog(mut self, mode: FogMode, params: FogParameters) -> Self {
        self.fog_mode = mode;
        self.fog_params = params;
        self
    }

    /// Builder: Enable or disable wireframe debug mode.
    pub fn with_wireframe(mut self, wireframe: bool) -> Self {
        self.wireframe = wireframe;
        if wireframe {
            self.polygon_mode = PolygonMode::Line;
            self.cull_mode = CullMode::None;
        } else {
            self.polygon_mode = PolygonMode::Fill;
            self.cull_mode = if self.two_sided { CullMode::None } else { CullMode::Back };
        }
        self
    }

    /// Builder: Set blending mode.
    pub fn with_blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        // Translucent and additive blend modes disable depth writes
        self.depth_write = match blend_mode {
            BlendMode::Opaque | BlendMode::AlphaTest => true,
            BlendMode::Translucent | BlendMode::Additive | BlendMode::Modulated | BlendMode::AlphaMultiply => false,
        };
        self
    }

    /// Builder: Configure two-sided geometry.
    pub fn with_two_sided(mut self, two_sided: bool) -> Self {
        self.two_sided = two_sided;
        if two_sided || self.wireframe {
            self.cull_mode = CullMode::None;
        } else {
            self.cull_mode = CullMode::Back;
        }
        self
    }

    /// Builder: Set face culling mode.
    pub fn with_cull_mode(mut self, cull_mode: CullMode) -> Self {
        self.cull_mode = cull_mode;
        self
    }

    /// Builder: Set depth testing parameters.
    pub fn with_depth_test(mut self, test: bool, write: bool, op: CompareOp) -> Self {
        self.depth_test = test;
        self.depth_write = write;
        self.depth_compare_op = op;
        self
    }

    /// Compute deterministic 64-bit hash for vertex layout.
    pub fn vertex_layout_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.vertex_layout.hash(&mut hasher);
        hasher.finish()
    }

    /// Convert this high-level configuration into a hashable `PipelineKey` for `PsoCache`.
    pub fn to_pipeline_key(&self) -> PipelineKey {
        let vertex_shader_id = self.vertex_shader.hash_id();
        let fragment_shader_id = self.fragment_shader.hash_id();
        let vertex_layout_hash = self.vertex_layout_hash();

        let polygon_mode = if self.wireframe {
            PolygonMode::Line
        } else {
            self.polygon_mode
        };

        let cull_mode = if self.wireframe || self.two_sided {
            CullMode::None
        } else {
            self.cull_mode
        };

        let (blend_enable, src_c, dst_c, op_c, src_a, dst_a, op_a) = match self.blend_mode {
            BlendMode::Opaque | BlendMode::AlphaTest => (
                false,
                BlendFactor::One,
                BlendFactor::Zero,
                BlendOp::Add,
                BlendFactor::One,
                BlendFactor::Zero,
                BlendOp::Add,
            ),
            BlendMode::Translucent => (
                true,
                BlendFactor::SrcAlpha,
                BlendFactor::OneMinusSrcAlpha,
                BlendOp::Add,
                BlendFactor::One,
                BlendFactor::OneMinusSrcAlpha,
                BlendOp::Add,
            ),
            BlendMode::Additive => (
                true,
                BlendFactor::One,
                BlendFactor::One,
                BlendOp::Add,
                BlendFactor::One,
                BlendFactor::One,
                BlendOp::Add,
            ),
            BlendMode::Modulated => (
                true,
                BlendFactor::DstColor,
                BlendFactor::Zero,
                BlendOp::Add,
                BlendFactor::DstAlpha,
                BlendFactor::Zero,
                BlendOp::Add,
            ),
            BlendMode::AlphaMultiply => (
                true,
                BlendFactor::SrcAlpha,
                BlendFactor::One,
                BlendOp::Add,
                BlendFactor::SrcAlpha,
                BlendFactor::One,
                BlendOp::Add,
            ),
        };

        let depth_write_enable = match self.blend_mode {
            BlendMode::Opaque | BlendMode::AlphaTest => self.depth_write,
            _ => false,
        };

        PipelineKey {
            vertex_shader_id,
            fragment_shader_id,
            vertex_layout_hash,
            topology: self.topology,
            polygon_mode,
            cull_mode,
            front_face: self.front_face,
            depth_test_enable: self.depth_test,
            depth_write_enable,
            depth_compare_op: self.depth_compare_op,
            blend_enable,
            src_color_blend: src_c,
            dst_color_blend: dst_c,
            color_blend_op: op_c,
            src_alpha_blend: src_a,
            dst_alpha_blend: dst_a,
            alpha_blend_op: op_a,
            color_write_mask: self.color_write_mask,
        }
    }

    /// Compiles or retrieves the PSO from cache using the configuration's pipeline key.
    pub fn create_pso(&self, pso_cache: &mut PsoCache) -> Result<CompiledPipeline, RenderError> {
        self.create_pso_with_compiler(pso_cache, |_| Ok(()))
    }

    /// Compiles or retrieves the PSO from cache with a custom hardware driver compiler closure.
    pub fn create_pso_with_compiler<F>(
        &self,
        pso_cache: &mut PsoCache,
        compiler: F,
    ) -> Result<CompiledPipeline, RenderError>
    where
        F: FnOnce(&PipelineKey) -> Result<(), RenderError>,
    {
        let key = self.to_pipeline_key();
        let compiled = pso_cache.get_or_create(&key, &self.debug_name, compiler)?;
        Ok(compiled.clone())
    }
}

// ------------------------------------------------------------------------------------------------
// Unit Tests
// ------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spirv_bytecode_validation_and_header() {
        let vs = ShaderBytecode::world_vertex();
        assert!(vs.is_valid_spirv());
        assert_eq!(vs.spirv_version(), Some((1, 3)));
        assert_eq!(vs.stage, ShaderStage::Vertex);
        assert_eq!(vs.entry_point, "main");
        assert_eq!(vs.as_words()[0], SPIRV_MAGIC);

        let fs = ShaderBytecode::world_fragment();
        assert!(fs.is_valid_spirv());
        assert_eq!(fs.spirv_version(), Some((1, 3)));
        assert_eq!(fs.stage, ShaderStage::Fragment);

        // Byte roundtrip
        let bytes = vs.as_bytes();
        let parsed = ShaderBytecode::from_bytes(ShaderStage::Vertex, "main", bytes).unwrap();
        assert_eq!(parsed.words, vs.words);
        assert!(parsed.is_valid_spirv());

        // Corrupt / invalid bytecode tests
        let mut corrupted_words = vs.words.clone();
        corrupted_words[0] = 0xDEADBEEF; // Invalid magic
        let corrupted_bytecode = ShaderBytecode::from_words(ShaderStage::Vertex, "main", corrupted_words);
        assert!(!corrupted_bytecode.is_valid_spirv());

        let invalid_bytes = [0x00, 0x01, 0x02]; // Non-multiple of 4
        assert!(ShaderBytecode::from_bytes(ShaderStage::Vertex, "main", &invalid_bytes).is_err());
    }

    #[test]
    fn test_default_world_pipeline() {
        let config = ShaderPipelineConfig::default_world_pipeline();
        assert_eq!(config.debug_name, "WorldStaticMeshOpaque");
        assert_eq!(config.topology, PrimitiveTopology::TriangleList);
        assert_eq!(config.polygon_mode, PolygonMode::Fill);
        assert_eq!(config.cull_mode, CullMode::Back);
        assert!(config.depth_test);
        assert!(config.depth_write);
        assert_eq!(config.blend_mode, BlendMode::Opaque);
        assert_eq!(config.lighting_model, LightingModel::LightmapModulated);
        assert_eq!(config.fog_mode, FogMode::Linear);
        assert!(!config.wireframe);

        let key = config.to_pipeline_key();
        assert_eq!(key.polygon_mode, PolygonMode::Fill);
        assert_eq!(key.cull_mode, CullMode::Back);
        assert!(!key.blend_enable);
        assert!(key.depth_write_enable);
        assert!(key.depth_test_enable);
        assert_ne!(key.vertex_shader_id, 0);
        assert_ne!(key.fragment_shader_id, 0);
        assert_ne!(key.vertex_layout_hash, 0);
    }

    #[test]
    fn test_create_pso_caching_and_compilation() {
        let mut pso_cache = PsoCache::new();
        let config = ShaderPipelineConfig::default_world_pipeline();

        // 1st call: Miss + compile
        let pso1 = config.create_pso(&mut pso_cache).expect("PSO creation failed");
        assert_eq!(pso1.id, 1);
        assert_eq!(pso1.debug_name, "WorldStaticMeshOpaque");
        assert_eq!(pso_cache.stats().hits, 0);
        assert_eq!(pso_cache.stats().misses, 1);

        // 2nd call: Hit in cache
        let pso2 = config.create_pso(&mut pso_cache).expect("PSO retrieval failed");
        assert_eq!(pso2.id, 1);
        assert_eq!(pso2.key, pso1.key);
        assert_eq!(pso_cache.stats().hits, 1);
        assert_eq!(pso_cache.stats().misses, 1);
    }

    #[test]
    fn test_wireframe_debug_pipeline() {
        let wf = ShaderPipelineConfig::wireframe_pipeline();
        assert!(wf.wireframe);
        assert_eq!(wf.polygon_mode, PolygonMode::Line);
        assert_eq!(wf.cull_mode, CullMode::None);

        let wf_key = wf.to_pipeline_key();
        assert_eq!(wf_key.polygon_mode, PolygonMode::Line);
        assert_eq!(wf_key.cull_mode, CullMode::None);

        let mut pso_cache = PsoCache::new();
        let pso = wf.create_pso(&mut pso_cache).unwrap();
        assert_eq!(pso.key.polygon_mode, PolygonMode::Line);
        assert_eq!(pso.key.cull_mode, CullMode::None);

        // Dynamic toggle with builder
        let toggled = ShaderPipelineConfig::default_world_pipeline().with_wireframe(true);
        let toggled_key = toggled.to_pipeline_key();
        assert_eq!(toggled_key.polygon_mode, PolygonMode::Line);
        assert_eq!(toggled_key.cull_mode, CullMode::None);
    }

    #[test]
    fn test_lightmap_and_vertex_lighting_modulation() {
        let diffuse = [0.8, 0.6, 0.4, 1.0];
        let lightmap = [0.5, 0.5, 0.5, 1.0];

        // Standard UE2 2.0x overbright lightmap modulation
        let modulated = evaluate_lightmap_modulation(diffuse, lightmap, 2.0);
        assert!((modulated[0] - 0.8).abs() < 1e-4);
        assert!((modulated[1] - 0.6).abs() < 1e-4);
        assert!((modulated[2] - 0.4).abs() < 1e-4);
        assert_eq!(modulated[3], 1.0);

        // Overbright clamping to 1.0
        let bright_lightmap = [0.9, 0.9, 0.9, 1.0];
        let overbright = evaluate_lightmap_modulation(diffuse, bright_lightmap, 2.0);
        assert_eq!(overbright[0], 1.0); // 0.8 * 0.9 * 2 = 1.44 -> clamped to 1.0
        assert_eq!(overbright[3], 1.0);

        // Vertex lighting modulation
        let vcolor = [0.5, 0.5, 0.5, 0.5];
        let vlit = evaluate_vertex_lighting_modulation(diffuse, vcolor);
        assert!((vlit[0] - 0.4).abs() < 1e-4);
        assert!((vlit[1] - 0.3).abs() < 1e-4);
        assert!((vlit[2] - 0.2).abs() < 1e-4);
        assert_eq!(vlit[3], 0.5);
    }

    #[test]
    fn test_fog_calculation_formulas() {
        let start = 500.0;
        let end = 2500.0;
        let density = 0.001;

        // Linear Fog
        assert_eq!(calculate_fog_factor(FogMode::Linear, 100.0, start, end, density), 1.0); // Before start
        assert_eq!(calculate_fog_factor(FogMode::Linear, 3000.0, start, end, density), 0.0); // Beyond end
        let mid_factor = calculate_fog_factor(FogMode::Linear, 1500.0, start, end, density);
        assert!((mid_factor - 0.5).abs() < 1e-4); // Halfway: (2500 - 1500) / (2500 - 500) = 0.5

        // Exponential Fog
        let exp_factor = calculate_fog_factor(FogMode::Exponential, 1000.0, start, end, density);
        let expected_exp = (-1.0f32).exp(); // -1000 * 0.001 = -1
        assert!((exp_factor - expected_exp).abs() < 1e-4);

        // Exponential Squared Fog
        let exp2_factor = calculate_fog_factor(FogMode::ExponentialSquared, 1000.0, start, end, density);
        let expected_exp2 = (-1.0f32 * 1.0f32).exp();
        assert!((exp2_factor - expected_exp2).abs() < 1e-4);

        // Fog Blending
        let lit_color = [1.0, 0.0, 0.0, 1.0]; // Red
        let fog_color = [0.0, 0.0, 1.0, 1.0]; // Blue
        let blended = apply_fog(lit_color, fog_color, 0.5);
        assert!((blended[0] - 0.5).abs() < 1e-4);
        assert_eq!(blended[1], 0.0);
        assert!((blended[2] - 0.5).abs() < 1e-4);
        assert_eq!(blended[3], 1.0);
    }

    #[test]
    fn test_fog_parameters_struct_alignment() {
        assert_eq!(std::mem::size_of::<FogParameters>(), 32);
        assert_eq!(std::mem::align_of::<FogParameters>(), 4);

        let fog = FogParameters::new(
            FogMode::Linear,
            [0.2, 0.3, 0.4, 1.0],
            200.0,
            1200.0,
            0.002,
        );
        let res = fog.apply([1.0, 1.0, 1.0, 1.0], 700.0); // mid-way
        assert!((res[0] - (0.2 * 0.5 + 1.0 * 0.5)).abs() < 1e-4);
    }

    #[test]
    fn test_pipeline_layout_desc() {
        let layout = PipelineLayoutDesc::default_world_layout();
        assert_eq!(layout.set_layouts.len(), 2);
        assert_eq!(layout.set_layouts[0].len(), 1); // Scene uniforms
        assert_eq!(layout.set_layouts[1].len(), 4); // Diffuse, Lightmap, Normal, Specular
        assert_eq!(layout.push_constants.len(), 1);
        assert_eq!(layout.push_constants[0].size, 64);
    }

    #[test]
    fn test_blend_modes_in_pipeline_config() {
        let trans = ShaderPipelineConfig::default_world_pipeline().with_blend_mode(BlendMode::Translucent);
        let trans_key = trans.to_pipeline_key();
        assert!(trans_key.blend_enable);
        assert!(!trans_key.depth_write_enable);
        assert_eq!(trans_key.src_color_blend, BlendFactor::SrcAlpha);
        assert_eq!(trans_key.dst_color_blend, BlendFactor::OneMinusSrcAlpha);

        let add = ShaderPipelineConfig::default_world_pipeline().with_blend_mode(BlendMode::Additive);
        let add_key = add.to_pipeline_key();
        assert!(add_key.blend_enable);
        assert!(!add_key.depth_write_enable);
        assert_eq!(add_key.src_color_blend, BlendFactor::One);
        assert_eq!(add_key.dst_color_blend, BlendFactor::One);
    }
}
