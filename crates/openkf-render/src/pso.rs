use std::collections::HashMap;
use bitflags::bitflags;
use crate::error::RenderError;

/// Geometric primitive topology for rasterizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PrimitiveTopology {
    #[default]
    TriangleList,
    TriangleStrip,
    LineList,
    LineStrip,
    PointList,
}

/// Polygon rasterization mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PolygonMode {
    #[default]
    Fill,
    Line,
    Point,
}

/// Triangle culling mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CullMode {
    None,
    #[default]
    Back,
    Front,
}

/// Vertex winding order for front-facing triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum FrontFace {
    #[default]
    CounterClockwise,
    Clockwise,
}

/// Depth comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CompareOp {
    Never,
    #[default]
    Less,
    Equal,
    LessOrEqual,
    Greater,
    NotEqual,
    GreaterOrEqual,
    Always,
}

/// Blend factors for color and alpha frame-buffer arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BlendFactor {
    #[default]
    Zero,
    One,
    SrcColor,
    OneMinusSrcColor,
    DstColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcAlpha,
    DstAlpha,
    OneMinusDstAlpha,
}

/// Blend operations for combining source and destination pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BlendOp {
    #[default]
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

bitflags! {
    /// Mask controlling channel writes to render targets.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ColorWriteMask: u8 {
        const RED   = 0x1;
        const GREEN = 0x2;
        const BLUE  = 0x4;
        const ALPHA = 0x8;
        const ALL   = 0xF;
    }
}

impl Default for ColorWriteMask {
    fn default() -> Self {
        ColorWriteMask::ALL
    }
}

/// Fully hashable, immutable Pipeline State Object (PSO) descriptor key.
/// Replaces legacy Direct3D 9 immediate state machine with modern Vulkan / D3D12 PSO architecture.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    /// Hash or ID of vertex shader stage.
    pub vertex_shader_id: u64,
    /// Hash or ID of fragment shader stage.
    pub fragment_shader_id: u64,
    /// Hash of input vertex streams & attributes.
    pub vertex_layout_hash: u64,
    /// Primitive assembly topology.
    pub topology: PrimitiveTopology,
    /// Rasterizer polygon mode.
    pub polygon_mode: PolygonMode,
    /// Face culling mode.
    pub cull_mode: CullMode,
    /// Front-face winding order.
    pub front_face: FrontFace,
    /// Whether depth testing is enabled.
    pub depth_test_enable: bool,
    /// Whether depth buffer writes are enabled.
    pub depth_write_enable: bool,
    /// Depth test comparison operator.
    pub depth_compare_op: CompareOp,
    /// Whether color blending is enabled on attachment 0.
    pub blend_enable: bool,
    /// Source color blend factor.
    pub src_color_blend: BlendFactor,
    /// Destination color blend factor.
    pub dst_color_blend: BlendFactor,
    /// Color blend operator.
    pub color_blend_op: BlendOp,
    /// Source alpha blend factor.
    pub src_alpha_blend: BlendFactor,
    /// Destination alpha blend factor.
    pub dst_alpha_blend: BlendFactor,
    /// Alpha blend operator.
    pub alpha_blend_op: BlendOp,
    /// Render target write mask.
    pub color_write_mask: ColorWriteMask,
}

impl Default for PipelineKey {
    fn default() -> Self {
        Self {
            vertex_shader_id: 0,
            fragment_shader_id: 0,
            vertex_layout_hash: 0,
            topology: PrimitiveTopology::TriangleList,
            polygon_mode: PolygonMode::Fill,
            cull_mode: CullMode::Back,
            front_face: FrontFace::CounterClockwise,
            depth_test_enable: true,
            depth_write_enable: true,
            depth_compare_op: CompareOp::Less,
            blend_enable: false,
            src_color_blend: BlendFactor::One,
            dst_color_blend: BlendFactor::Zero,
            color_blend_op: BlendOp::Add,
            src_alpha_blend: BlendFactor::One,
            dst_alpha_blend: BlendFactor::Zero,
            alpha_blend_op: BlendOp::Add,
            color_write_mask: ColorWriteMask::ALL,
        }
    }
}

/// Compiled hardware pipeline state representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledPipeline {
    pub id: u32,
    pub key: PipelineKey,
    pub debug_name: String,
}

/// Statistics for the PSO cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PsoCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_created: usize,
}

/// High-performance PSO cache replacing D3D9 immediate state changes.
pub struct PsoCache {
    pipelines: HashMap<PipelineKey, CompiledPipeline>,
    next_id: u32,
    hits: u64,
    misses: u64,
}

impl Default for PsoCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PsoCache {
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            next_id: 1,
            hits: 0,
            misses: 0,
        }
    }

    /// Retrieve an existing compiled pipeline or compile a new one.
    pub fn get_or_create<F>(&mut self, key: &PipelineKey, debug_name: &str, compiler: F) -> Result<&CompiledPipeline, RenderError>
    where
        F: FnOnce(&PipelineKey) -> Result<(), RenderError>,
    {
        if self.pipelines.contains_key(key) {
            self.hits += 1;
            return Ok(self.pipelines.get(key).unwrap());
        }

        self.misses += 1;
        compiler(key)?;

        let id = self.next_id;
        self.next_id += 1;

        let compiled = CompiledPipeline {
            id,
            key: key.clone(),
            debug_name: debug_name.to_string(),
        };

        self.pipelines.insert(key.clone(), compiled);
        Ok(self.pipelines.get(key).unwrap())
    }

    /// Query if a pipeline key exists in cache.
    pub fn get(&mut self, key: &PipelineKey) -> Option<&CompiledPipeline> {
        if let Some(p) = self.pipelines.get(key) {
            self.hits += 1;
            Some(p)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Number of active cached pipelines.
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    /// Cache performance metrics.
    pub fn stats(&self) -> PsoCacheStats {
        PsoCacheStats {
            hits: self.hits,
            misses: self.misses,
            total_created: self.pipelines.len(),
        }
    }

    /// Flush all compiled pipelines.
    pub fn clear(&mut self) {
        self.pipelines.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Serializes cache keys into a binary blob for persistent disk caching.
    pub fn serialize_cache(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        // Magic header: b"KFPS" (Killing Floor Pipeline State)
        buffer.extend_from_slice(b"KFPS");
        let count = self.pipelines.len() as u32;
        buffer.extend_from_slice(&count.to_le_bytes());

        for (key, pipeline) in &self.pipelines {
            buffer.extend_from_slice(&pipeline.id.to_le_bytes());
            buffer.extend_from_slice(&key.vertex_shader_id.to_le_bytes());
            buffer.extend_from_slice(&key.fragment_shader_id.to_le_bytes());
            buffer.extend_from_slice(&key.vertex_layout_hash.to_le_bytes());
            buffer.push(key.topology as u8);
            buffer.push(key.polygon_mode as u8);
            buffer.push(key.cull_mode as u8);
            buffer.push(key.front_face as u8);
            buffer.push(key.depth_test_enable as u8);
            buffer.push(key.depth_write_enable as u8);
            buffer.push(key.depth_compare_op as u8);
            buffer.push(key.blend_enable as u8);
            buffer.push(key.src_color_blend as u8);
            buffer.push(key.dst_color_blend as u8);
            buffer.push(key.color_blend_op as u8);
            buffer.push(key.src_alpha_blend as u8);
            buffer.push(key.dst_alpha_blend as u8);
            buffer.push(key.alpha_blend_op as u8);
            buffer.push(key.color_write_mask.bits());

            let name_bytes = pipeline.debug_name.as_bytes();
            let name_len = name_bytes.len() as u16;
            buffer.extend_from_slice(&name_len.to_le_bytes());
            buffer.extend_from_slice(name_bytes);
        }

        buffer
    }

    /// Deserializes a binary cache blob from disk into memory.
    pub fn deserialize_cache(&mut self, data: &[u8]) -> Result<usize, RenderError> {
        if data.len() < 8 {
            return Err(RenderError::SerializationError("Cache buffer too short".into()));
        }
        if &data[0..4] != b"KFPS" {
            return Err(RenderError::SerializationError("Invalid magic header for PSO cache".into()));
        }

        let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let mut offset = 8;
        let mut loaded = 0;

        for _ in 0..count {
            if offset + 4 + 8 + 8 + 8 + 16 > data.len() {
                return Err(RenderError::SerializationError("Truncated PSO cache record".into()));
            }

            let id = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
            offset += 4;
            let vs_id = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let fs_id = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let v_layout = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            offset += 8;

            let topology = match data[offset] {
                0 => PrimitiveTopology::TriangleList,
                1 => PrimitiveTopology::TriangleStrip,
                2 => PrimitiveTopology::LineList,
                3 => PrimitiveTopology::LineStrip,
                _ => PrimitiveTopology::PointList,
            };
            offset += 1;

            let polygon_mode = match data[offset] {
                0 => PolygonMode::Fill,
                1 => PolygonMode::Line,
                _ => PolygonMode::Point,
            };
            offset += 1;

            let cull_mode = match data[offset] {
                0 => CullMode::None,
                1 => CullMode::Back,
                _ => CullMode::Front,
            };
            offset += 1;

            let front_face = match data[offset] {
                0 => FrontFace::CounterClockwise,
                _ => FrontFace::Clockwise,
            };
            offset += 1;

            let depth_test_enable = data[offset] != 0;
            offset += 1;
            let depth_write_enable = data[offset] != 0;
            offset += 1;

            let depth_compare_op = match data[offset] {
                1 => CompareOp::Less,
                2 => CompareOp::Equal,
                3 => CompareOp::LessOrEqual,
                4 => CompareOp::Greater,
                5 => CompareOp::NotEqual,
                6 => CompareOp::GreaterOrEqual,
                7 => CompareOp::Always,
                _ => CompareOp::Never,
            };
            offset += 1;

            let blend_enable = data[offset] != 0;
            offset += 1;

            let decode_blend_factor = |b: u8| match b {
                1 => BlendFactor::One,
                2 => BlendFactor::SrcColor,
                3 => BlendFactor::OneMinusSrcColor,
                4 => BlendFactor::DstColor,
                5 => BlendFactor::OneMinusDstColor,
                6 => BlendFactor::SrcAlpha,
                7 => BlendFactor::OneMinusSrcAlpha,
                8 => BlendFactor::DstAlpha,
                9 => BlendFactor::OneMinusDstAlpha,
                _ => BlendFactor::Zero,
            };

            let src_color_blend = decode_blend_factor(data[offset]);
            offset += 1;
            let dst_color_blend = decode_blend_factor(data[offset]);
            offset += 1;

            let decode_blend_op = |b: u8| match b {
                1 => BlendOp::Subtract,
                2 => BlendOp::ReverseSubtract,
                3 => BlendOp::Min,
                4 => BlendOp::Max,
                _ => BlendOp::Add,
            };

            let color_blend_op = decode_blend_op(data[offset]);
            offset += 1;
            let src_alpha_blend = decode_blend_factor(data[offset]);
            offset += 1;
            let dst_alpha_blend = decode_blend_factor(data[offset]);
            offset += 1;
            let alpha_blend_op = decode_blend_op(data[offset]);
            offset += 1;

            let color_write_mask = ColorWriteMask::from_bits_truncate(data[offset]);
            offset += 1;

            if offset + 2 > data.len() {
                return Err(RenderError::SerializationError("Truncated name length".into()));
            }
            let name_len = u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as usize;
            offset += 2;

            if offset + name_len > data.len() {
                return Err(RenderError::SerializationError("Truncated name string".into()));
            }
            let debug_name = String::from_utf8_lossy(&data[offset..offset + name_len]).into_owned();
            offset += name_len;

            let key = PipelineKey {
                vertex_shader_id: vs_id,
                fragment_shader_id: fs_id,
                vertex_layout_hash: v_layout,
                topology,
                polygon_mode,
                cull_mode,
                front_face,
                depth_test_enable,
                depth_write_enable,
                depth_compare_op,
                blend_enable,
                src_color_blend,
                dst_color_blend,
                color_blend_op,
                src_alpha_blend,
                dst_alpha_blend,
                alpha_blend_op,
                color_write_mask,
            };

            self.pipelines.insert(key.clone(), CompiledPipeline {
                id,
                key,
                debug_name,
            });

            if id >= self.next_id {
                self.next_id = id + 1;
            }
            loaded += 1;
        }

        Ok(loaded)
    }
}
