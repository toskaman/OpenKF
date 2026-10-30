use bytemuck::{Pod, Zeroable};
use crate::pso::{BlendFactor, BlendOp, ColorWriteMask, PipelineKey, PrimitiveTopology, PolygonMode, CullMode, FrontFace, CompareOp};

/// Retail Unreal Engine 2.5 Frame-buffer blending modes serialized in `UFinalBlend`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// Fully opaque geometry. Writes to depth buffer; no alpha blending.
    #[default]
    Opaque,
    /// 1-bit cutout transparency (grates, foliage, fences).
    /// Discards fragment if alpha < alpha_ref. Writes to depth buffer.
    AlphaTest,
    /// Standard smooth alpha transparency (glass, windows, water).
    /// Blends: SrcAlpha + (1 - SrcAlpha) * Dst. Depth buffer write disabled.
    Translucent,
    /// Additive luminous blending (muzzle flash, laser beams, fire particles, sparks).
    /// Blends: One * Src + One * Dst. Depth buffer write disabled.
    Additive,
    /// Multiplicative light/shadow modulation (decals, blood splatters, shadows).
    /// Blends: DstColor * Src + Zero * Dst. Depth buffer write disabled.
    Modulated,
    /// Alpha-multiplied blend mode. Blends: SrcAlpha * Src + One * Dst.
    AlphaMultiply,
}

/// UV transform animation modifiers (TexPanner, TexRotator, TexOscillator).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UvModifier {
    Panner { speed_u: f32, speed_v: f32 },
    Rotator { rate: f32, pivot_u: f32, pivot_v: f32 },
    Oscillator { rate_u: f32, rate_v: f32, amplitude_u: f32, amplitude_v: f32 },
}

/// High-level representation of an Unreal Engine 2.5 material definition.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDefinition {
    pub name: String,
    pub blend_mode: BlendMode,
    pub alpha_ref: f32,
    pub two_sided: bool,
    pub wireframe: bool,
    pub diffuse_map: Option<String>,
    pub normal_map: Option<String>,
    pub specular_map: Option<String>,
    pub opacity_map: Option<String>,
    pub detail_map: Option<String>,
    pub diffuse_color: [f32; 4],
    pub specular_color: [f32; 4],
    pub specular_power: f32,
    pub uv_modifier: Option<UvModifier>,
}

impl Default for MaterialDefinition {
    fn default() -> Self {
        Self {
            name: "DefaultMaterial".to_string(),
            blend_mode: BlendMode::Opaque,
            alpha_ref: 0.5,
            two_sided: false,
            wireframe: false,
            diffuse_map: None,
            normal_map: None,
            specular_map: None,
            opacity_map: None,
            detail_map: None,
            diffuse_color: [1.0, 1.0, 1.0, 1.0],
            specular_color: [1.0, 1.0, 1.0, 1.0],
            specular_power: 16.0,
            uv_modifier: None,
        }
    }
}

/// Material flag bits for GPU uniform buffer.
pub mod material_flags {
    pub const HAS_DIFFUSE: u32 = 1 << 0;
    pub const HAS_NORMAL: u32 = 1 << 1;
    pub const HAS_SPECULAR: u32 = 1 << 2;
    pub const HAS_OPACITY: u32 = 1 << 3;
    pub const HAS_DETAIL: u32 = 1 << 4;
    pub const ALPHA_TEST: u32 = 1 << 5;
    pub const TWO_SIDED: u32 = 1 << 6;
}

/// Modern GPU uniform buffer layout for material parameters.
/// Strictly 64-byte aligned and packed for std140 / Vulkan push constants.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MaterialConstants {
    /// Base albedo tint color (RGBA). Offset 0x00 (16 bytes).
    pub diffuse_tint: [f32; 4],
    /// Specular reflection color (RGB) + roughness/gloss in W. Offset 0x10 (16 bytes).
    pub specular_tint: [f32; 4],
    /// UV translation (X, Y) and UV tiling scale (Z, W). Offset 0x20 (16 bytes).
    pub uv_pan_scale: [f32; 4],
    /// Specular exponent power. Offset 0x30 (4 bytes).
    pub specular_power: f32,
    /// Alpha cutout discard threshold for AlphaTest. Offset 0x34 (4 bytes).
    pub alpha_cutoff: f32,
    /// Bitflags indicating bound textures and flags. Offset 0x38 (4 bytes).
    pub flags: u32,
    /// Padding for 64-byte alignment. Offset 0x3C (4 bytes).
    pub _pad: u32,
}

impl Default for MaterialConstants {
    fn default() -> Self {
        Self {
            diffuse_tint: [1.0, 1.0, 1.0, 1.0],
            specular_tint: [1.0, 1.0, 1.0, 1.0],
            uv_pan_scale: [0.0, 0.0, 1.0, 1.0],
            specular_power: 16.0,
            alpha_cutoff: 0.0,
            flags: 0,
            _pad: 0,
        }
    }
}

/// Descriptor slot mapping for modern shader bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorSlot {
    Diffuse = 0,
    Normal = 1,
    Specular = 2,
    Opacity = 3,
    Detail = 4,
    MaterialUniforms = 5,
}

/// Translated descriptor bindings ready for modern Vulkan `vkUpdateDescriptorSets`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorBinding {
    pub slot: u32,
    pub resource_name: String,
    pub is_sampler_texture: bool,
}

/// Complete translated material ready for modern pipeline binding.
#[derive(Clone, Debug, PartialEq)]
pub struct TranslatedMaterial {
    pub name: String,
    pub blend_mode: BlendMode,
    pub constants: MaterialConstants,
    pub descriptor_bindings: Vec<DescriptorBinding>,
    pub depth_write: bool,
    pub depth_test: bool,
    pub cull_mode: CullMode,
}

/// Translator converting Unreal Engine 2.5 material graphs into modern HAL state.
pub struct MaterialTranslator;

impl MaterialTranslator {
    /// Translates a material definition and time sample into GPU constants, descriptors, and PSO key.
    pub fn translate(mat: &MaterialDefinition, time_seconds: f32) -> TranslatedMaterial {
        let mut flags = 0u32;
        let mut bindings = Vec::new();

        if let Some(ref diff) = mat.diffuse_map {
            flags |= material_flags::HAS_DIFFUSE;
            bindings.push(DescriptorBinding {
                slot: DescriptorSlot::Diffuse as u32,
                resource_name: diff.clone(),
                is_sampler_texture: true,
            });
        }

        if let Some(ref norm) = mat.normal_map {
            flags |= material_flags::HAS_NORMAL;
            bindings.push(DescriptorBinding {
                slot: DescriptorSlot::Normal as u32,
                resource_name: norm.clone(),
                is_sampler_texture: true,
            });
        }

        if let Some(ref spec) = mat.specular_map {
            flags |= material_flags::HAS_SPECULAR;
            bindings.push(DescriptorBinding {
                slot: DescriptorSlot::Specular as u32,
                resource_name: spec.clone(),
                is_sampler_texture: true,
            });
        }

        if let Some(ref opac) = mat.opacity_map {
            flags |= material_flags::HAS_OPACITY;
            bindings.push(DescriptorBinding {
                slot: DescriptorSlot::Opacity as u32,
                resource_name: opac.clone(),
                is_sampler_texture: true,
            });
        }

        if let Some(ref detail) = mat.detail_map {
            flags |= material_flags::HAS_DETAIL;
            bindings.push(DescriptorBinding {
                slot: DescriptorSlot::Detail as u32,
                resource_name: detail.clone(),
                is_sampler_texture: true,
            });
        }

        if mat.two_sided {
            flags |= material_flags::TWO_SIDED;
        }

        let (alpha_cutoff, depth_write) = match mat.blend_mode {
            BlendMode::Opaque => (0.0, true),
            BlendMode::AlphaTest => {
                flags |= material_flags::ALPHA_TEST;
                (mat.alpha_ref, true)
            }
            BlendMode::Translucent | BlendMode::Additive | BlendMode::Modulated | BlendMode::AlphaMultiply => {
                (0.0, false)
            }
        };

        // Compute UV animation offsets
        let mut uv_pan_scale = [0.0, 0.0, 1.0, 1.0];
        if let Some(modifier) = mat.uv_modifier {
            match modifier {
                UvModifier::Panner { speed_u, speed_v } => {
                    uv_pan_scale[0] = (speed_u * time_seconds).fract();
                    uv_pan_scale[1] = (speed_v * time_seconds).fract();
                }
                UvModifier::Rotator { rate, pivot_u, pivot_v } => {
                    let angle = rate * time_seconds;
                    uv_pan_scale[0] = pivot_u;
                    uv_pan_scale[1] = pivot_v;
                    uv_pan_scale[2] = angle.cos();
                    uv_pan_scale[3] = angle.sin();
                }
                UvModifier::Oscillator { rate_u, rate_v, amplitude_u, amplitude_v } => {
                    uv_pan_scale[0] = (rate_u * time_seconds).sin() * amplitude_u;
                    uv_pan_scale[1] = (rate_v * time_seconds).sin() * amplitude_v;
                }
            }
        }

        let constants = MaterialConstants {
            diffuse_tint: mat.diffuse_color,
            specular_tint: mat.specular_color,
            uv_pan_scale,
            specular_power: mat.specular_power,
            alpha_cutoff,
            flags,
            _pad: 0,
        };

        let cull_mode = if mat.two_sided {
            CullMode::None
        } else {
            CullMode::Back
        };

        TranslatedMaterial {
            name: mat.name.clone(),
            blend_mode: mat.blend_mode,
            constants,
            descriptor_bindings: bindings,
            depth_write,
            depth_test: true,
            cull_mode,
        }
    }

    /// Generates a pipeline key corresponding to the translated material and vertex layout.
    pub fn build_pipeline_key(
        translated: &TranslatedMaterial,
        vertex_layout_hash: u64,
        vertex_shader_id: u64,
        fragment_shader_id: u64,
    ) -> PipelineKey {
        let (blend_enable, src_c, dst_c, op_c, src_a, dst_a, op_a) = match translated.blend_mode {
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
                BlendFactor::One,
                BlendFactor::One,
                BlendOp::Add,
            ),
        };

        PipelineKey {
            vertex_shader_id,
            fragment_shader_id,
            vertex_layout_hash,
            topology: PrimitiveTopology::TriangleList,
            polygon_mode: PolygonMode::Fill,
            cull_mode: translated.cull_mode,
            front_face: FrontFace::CounterClockwise,
            depth_test_enable: translated.depth_test,
            depth_write_enable: translated.depth_write,
            depth_compare_op: CompareOp::LessOrEqual,
            blend_enable,
            src_color_blend: src_c,
            dst_color_blend: dst_c,
            color_blend_op: op_c,
            src_alpha_blend: src_a,
            dst_alpha_blend: dst_a,
            alpha_blend_op: op_a,
            color_write_mask: ColorWriteMask::ALL,
        }
    }
}
