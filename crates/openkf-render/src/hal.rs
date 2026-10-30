use glam::Mat4;
use crate::error::RenderError;
pub use crate::pso::PipelineKey;
use crate::material::TranslatedMaterial;

/// Transform matrix types in legacy Unreal Engine 2.5 `FRenderInterface::SetTransform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransformType {
    /// World-to-Camera View Matrix (Type 0).
    View = 0,
    /// Camera-to-Clip Projection Matrix (Type 1).
    Projection = 1,
    /// Model-to-World Object Matrix (Type 2).
    World = 2,
}

/// Primitive type for draw calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PrimitiveType {
    #[default]
    TriangleList,
    TriangleStrip,
    LineList,
    PointList,
}

/// Index buffer data type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

/// Opaque resource handles for GPU allocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipelineHandle(pub u32);

/// Viewport configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            min_depth: 0.0,
            max_depth: 1.0,
        }
    }
}

/// Scissor rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScissorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Framebuffer clear values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearValues {
    pub color: [f32; 4],
    pub depth: f32,
    pub stencil: u32,
    pub clear_color: bool,
    pub clear_depth: bool,
    pub clear_stencil: bool,
}

impl Default for ClearValues {
    fn default() -> Self {
        Self {
            color: [0.05, 0.05, 0.05, 1.0],
            depth: 1.0,
            stencil: 0,
            clear_color: true,
            clear_depth: true,
            clear_stencil: false,
        }
    }
}

/// Draw command recorded in command buffers.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderCommand {
    SetTransform {
        transform_type: TransformType,
        matrix: Mat4,
    },
    BindPipeline(PipelineHandle),
    BindVertexBuffer {
        binding: u32,
        buffer: BufferHandle,
        offset: u64,
    },
    BindIndexBuffer {
        buffer: BufferHandle,
        offset: u64,
        format: IndexFormat,
    },
    SetViewport(Viewport),
    SetScissor(ScissorRect),
    SetPushConstants {
        offset: u32,
        data: Vec<u8>,
    },
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    },
    DrawIndexedIndirect {
        buffer: BufferHandle,
        offset: u64,
        draw_count: u32,
        stride: u32,
    },
}

/// Low-level rendering interface abstracting UE2.5 `FRenderInterface`.
pub trait RenderInterface {
    /// Sets active transform matrix (View, Projection, or World).
    fn set_transform(&mut self, transform_type: TransformType, matrix: Mat4);

    /// Binds active material state and descriptors.
    fn set_material(&mut self, material: &TranslatedMaterial) -> Result<(), RenderError>;

    /// Binds GPU vertex stream buffer.
    fn set_vertex_buffer(&mut self, binding: u32, buffer: BufferHandle, offset: u64);

    /// Binds GPU index buffer.
    fn set_index_buffer(&mut self, buffer: BufferHandle, offset: u64, format: IndexFormat);

    /// Sets viewport dimensions.
    fn set_viewport(&mut self, viewport: Viewport);

    /// Sets scissor clipping rectangle.
    fn set_scissor(&mut self, scissor: ScissorRect);

    /// Issues indexed draw primitive call.
    fn draw_indexed_primitive(
        &mut self,
        primitive_type: PrimitiveType,
        first_index: u32,
        num_primitives: u32,
        min_index: u32,
        max_index: u32,
    );

    /// Issues instanced indexed draw call (for modern GPU indirect rendering).
    fn draw_indexed_instanced(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    );

    /// Issues indirect indexed draw (vkCmdDrawIndexedIndirect) from GPU buffer.
    fn draw_indexed_indirect(&mut self, buffer: BufferHandle, offset: u64, draw_count: u32, stride: u32);
}

/// High-level device lifecycle and swapchain manager abstracting UE2.5 `URenderDevice`.
pub trait RenderDevice {
    /// Initializes device context, caps negotiation, and shader models.
    fn init(&mut self) -> Result<(), RenderError>;

    /// Configures swapchain resolution and display format.
    fn set_res(
        &mut self,
        width: u32,
        height: u32,
        color_bytes: u32,
        fullscreen: bool,
        refresh_rate: u32,
    ) -> Result<(), RenderError>;

    /// Begins a frame render pass; returns an active `RenderInterface` command recorder.
    fn lock(&mut self, clear: ClearValues) -> Result<&mut dyn RenderInterface, RenderError>;

    /// Ends frame render pass.
    fn unlock(&mut self) -> Result<(), RenderError>;

    /// Swaps backbuffer to screen (`Present`).
    fn present(&mut self) -> Result<(), RenderError>;

    /// Evicts GPU resources (textures, vertex buffers) from cache.
    fn flush_resource(&mut self, cache_id: u64);

    /// Compiles or retrieves a cached Pipeline State Object (PSO).
    fn get_or_create_pipeline(&mut self, key: &PipelineKey) -> Result<PipelineHandle, RenderError>;
}
