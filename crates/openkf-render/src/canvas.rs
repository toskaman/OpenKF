//! Unreal Engine 2 Canvas 2D rendering primitives and dynamic batching.
//!
//! Provides the [`Canvas2D`] immediate-mode rendering canvas supporting
//! textured quads ([`Canvas2D::draw_tile`]), line primitives ([`Canvas2D::draw_line`]),
//! scissor clipping ([`Canvas2D::set_clip`]), orthographic 2D projection matrices,
//! and dynamic GPU batching compatible with the OpenKF Hardware Abstraction Layer.

use glam::Mat4;
use crate::error::RenderError;
use crate::hal::{
    BufferHandle, IndexFormat, PrimitiveType, RenderCommand, RenderInterface,
    ScissorRect, TextureHandle, TransformType,
};
use crate::material::BlendMode;
use crate::vertex::UIVertex;

/// Contiguous draw call batch sharing texture, blending state, and scissor clipping.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasDrawBatch {
    /// Primitive topology (`TriangleList` for quads, `LineList` for lines).
    pub primitive_type: PrimitiveType,
    /// Optional texture handle bound for this batch (`None` for solid colored primitives).
    pub texture: Option<TextureHandle>,
    /// Material blending mode.
    pub blend_mode: BlendMode,
    /// Active scissor clipping rectangle if clipping is enabled.
    pub scissor: Option<ScissorRect>,
    /// Offset of the first index in the index buffer.
    pub first_index: u32,
    /// Total number of indices in this batch.
    pub index_count: u32,
    /// Vertex buffer base offset.
    pub vertex_offset: i32,
    /// Total number of primitives (triangles or lines).
    pub num_primitives: u32,
    /// Lowest vertex index referenced in this batch.
    pub min_vertex_index: u32,
    /// Highest vertex index referenced in this batch.
    pub max_vertex_index: u32,
}

/// Statistics and GPU draw command payload generated when flushing the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasFlushResult {
    /// Ordered list of batched draw calls.
    pub batches: Vec<CanvasDrawBatch>,
    /// Accumulated vertex buffer payload for this frame.
    pub vertices: Vec<UIVertex>,
    /// Accumulated 16-bit index buffer payload for this frame.
    pub indices: Vec<u16>,
    /// Sequence of recorded low-level [`RenderCommand`]s ready for GPU execution.
    pub commands: Vec<RenderCommand>,
    /// Total number of distinct GPU draw batches.
    pub draw_call_count: u32,
    /// Total number of 2D quads drawn.
    pub quad_count: u32,
    /// Total number of 2D line segments drawn.
    pub line_count: u32,
    /// Total number of rasterized triangles.
    pub triangle_count: u64,
}

impl CanvasFlushResult {
    /// Returns true if no draw commands or geometry were recorded.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }
}

/// Unreal Engine 2 style 2D Canvas primitive renderer.
///
/// Implements immediate-mode drawing commands (`draw_tile`, `draw_line`, `set_clip`)
/// with automatic CPU-side draw call batching that groups sequential primitives sharing
/// the same texture, blend mode, and scissor rectangle.
#[derive(Clone, Debug)]
pub struct Canvas2D {
    /// Canvas virtual width in screen pixels.
    pub size_x: f32,
    /// Canvas virtual height in screen pixels.
    pub size_y: f32,
    /// Current cursor X coordinate (UnrealScript `Canvas.CurX`).
    pub cur_x: f32,
    /// Current cursor Y coordinate (UnrealScript `Canvas.CurY`).
    pub cur_y: f32,
    /// Clipping origin X coordinate (UnrealScript `Canvas.OrgX`).
    pub org_x: f32,
    /// Clipping origin Y coordinate (UnrealScript `Canvas.OrgY`).
    pub org_y: f32,
    /// Clipping width in screen pixels (UnrealScript `Canvas.ClipX`).
    pub clip_x: f32,
    /// Clipping height in screen pixels (UnrealScript `Canvas.ClipY`).
    pub clip_y: f32,
    /// Active scissor clipping rectangle (if set).
    pub current_clip: Option<ScissorRect>,
    /// Active tint color RGBA.
    pub draw_color: [f32; 4],
    /// Active blending mode.
    pub style: BlendMode,

    /// Accumulated dynamic vertex buffer stream (`UIVertex`, 32 bytes each).
    vertices: Vec<UIVertex>,
    /// Accumulated 16-bit index buffer stream.
    indices: Vec<u16>,
    /// Active draw call batches.
    batches: Vec<CanvasDrawBatch>,

    /// GPU vertex buffer handle assigned for the UI stream.
    pub vertex_buffer_handle: BufferHandle,
    /// GPU index buffer handle assigned for the UI stream.
    pub index_buffer_handle: BufferHandle,

    /// Statistics: total quads drawn since last flush.
    total_quads: u32,
    /// Statistics: total lines drawn since last flush.
    total_lines: u32,
}

impl Canvas2D {
    /// Creates a new Canvas2D instance with the given virtual screen dimensions.
    pub fn new(size_x: f32, size_y: f32) -> Self {
        Self {
            size_x,
            size_y,
            cur_x: 0.0,
            cur_y: 0.0,
            org_x: 0.0,
            org_y: 0.0,
            clip_x: size_x,
            clip_y: size_y,
            current_clip: None,
            draw_color: [1.0, 1.0, 1.0, 1.0],
            style: BlendMode::Translucent,
            vertices: Vec::with_capacity(1024),
            indices: Vec::with_capacity(1536),
            batches: Vec::with_capacity(64),
            vertex_buffer_handle: BufferHandle(100),
            index_buffer_handle: BufferHandle(101),
            total_quads: 0,
            total_lines: 0,
        }
    }

    /// Sets the cursor position (UnrealScript `Canvas.SetPos(X, Y)`).
    #[inline]
    pub fn set_pos(&mut self, x: f32, y: f32) {
        self.cur_x = x;
        self.cur_y = y;
    }

    /// Sets the active drawing color (RGBA in 0.0..1.0 range).
    #[inline]
    pub fn set_draw_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.draw_color = [r, g, b, a];
    }

    /// Sets the active blend style (Opaque, Translucent, Additive, Modulated).
    #[inline]
    pub fn set_style(&mut self, style: BlendMode) {
        self.style = style;
    }

    /// Computes the 2D orthographic projection matrix mapping screen pixel space
    /// `(0, 0) .. (width, height)` to Normalized Device Coordinates (NDC):
    /// - `(0.0, 0.0)` -> Top-Left `(-1.0, 1.0)`
    /// - `(width, height)` -> Bottom-Right `(1.0, -1.0)`
    /// - `(width/2, height/2)` -> Center `(0.0, 0.0)`
    pub fn ortho_projection_matrix(width: f32, height: f32) -> Mat4 {
        Mat4::orthographic_rh(0.0, width, height, 0.0, 0.0, 1.0)
    }

    /// Returns the active 2D orthographic projection matrix for this canvas.
    #[inline]
    pub fn ortho_projection(&self) -> Mat4 {
        Self::ortho_projection_matrix(self.size_x, self.size_y)
    }

    /// Alias for [`Canvas2D::ortho_projection`].
    #[inline]
    pub fn ortho_2d(&self) -> Mat4 {
        self.ortho_projection()
    }

    /// Sets the scissor clipping rectangle (UnrealScript `Canvas.SetClip(X, Y, XL, YL)`).
    ///
    /// Any subsequent primitives will be bound to this scissor region.
    /// If an active batch exists with a different scissor region, a new batch is started.
    pub fn set_clip(&mut self, x: f32, y: f32, xl: f32, yl: f32) {
        self.org_x = x;
        self.org_y = y;
        self.clip_x = xl;
        self.clip_y = yl;
        self.current_clip = Some(ScissorRect {
            x: x.max(0.0) as i32,
            y: y.max(0.0) as i32,
            width: xl.max(0.0) as u32,
            height: yl.max(0.0) as u32,
        });
    }

    /// Resets the clipping rectangle to the full canvas viewport (disables scissor clipping).
    pub fn reset_clip(&mut self) {
        self.org_x = 0.0;
        self.org_y = 0.0;
        self.clip_x = self.size_x;
        self.clip_y = self.size_y;
        self.current_clip = None;
    }

    /// Draws a textured 2D quad (UnrealScript `Canvas.DrawTile`).
    ///
    /// Parameters:
    /// - `x, y`: Screen coordinates of top-left corner.
    /// - `xl, yl`: Width and height of tile in screen pixels.
    /// - `u, v`: Texture coordinates of top-left corner.
    /// - `ul, vl`: Width and height in texture coordinate space.
    /// - `texture`: GPU texture handle.
    /// - `color`: RGBA tint color multiplier.
    pub fn draw_tile(
        &mut self,
        x: f32,
        y: f32,
        xl: f32,
        yl: f32,
        u: f32,
        v: f32,
        ul: f32,
        vl: f32,
        texture: TextureHandle,
        color: [f32; 4],
    ) {
        self.draw_tile_internal(x, y, xl, yl, u, v, ul, vl, Some(texture), color);
    }

    /// Draws an untextured solid-color 2D rectangle.
    pub fn draw_rect(&mut self, x: f32, y: f32, xl: f32, yl: f32, color: [f32; 4]) {
        self.draw_tile_internal(x, y, xl, yl, 0.0, 0.0, 1.0, 1.0, None, color);
    }

    /// Internal helper for textured or untextured quads.
    fn draw_tile_internal(
        &mut self,
        x: f32,
        y: f32,
        xl: f32,
        yl: f32,
        u: f32,
        v: f32,
        ul: f32,
        vl: f32,
        texture: Option<TextureHandle>,
        color: [f32; 4],
    ) {
        let base_vertex = self.vertices.len() as u32;
        let base_index = self.indices.len() as u32;

        // 4 Quad Vertices (Counter-Clockwise winding):
        // 0: Top-Left
        let v0 = UIVertex::new([x, y], [u, v], color);
        // 1: Top-Right
        let v1 = UIVertex::new([x + xl, y], [u + ul, v], color);
        // 2: Bottom-Right
        let v2 = UIVertex::new([x + xl, y + yl], [u + ul, v + vl], color);
        // 3: Bottom-Left
        let v3 = UIVertex::new([x, y + yl], [u, v + vl], color);

        self.vertices.push(v0);
        self.vertices.push(v1);
        self.vertices.push(v2);
        self.vertices.push(v3);

        // 6 Indices (2 Triangles):
        // Triangle 1: (0, 1, 2)
        // Triangle 2: (0, 2, 3)
        let b = base_vertex as u16;
        self.indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);

        self.total_quads += 1;

        // Dynamic Batching: attempt to merge with the previous batch
        let can_merge = if let Some(last) = self.batches.last() {
            last.primitive_type == PrimitiveType::TriangleList
                && last.texture == texture
                && last.blend_mode == self.style
                && last.scissor == self.current_clip
        } else {
            false
        };

        if can_merge {
            let last = self.batches.last_mut().unwrap();
            last.index_count += 6;
            last.num_primitives += 2;
            last.max_vertex_index = base_vertex + 3;
        } else {
            self.batches.push(CanvasDrawBatch {
                primitive_type: PrimitiveType::TriangleList,
                texture,
                blend_mode: self.style,
                scissor: self.current_clip,
                first_index: base_index,
                index_count: 6,
                vertex_offset: 0,
                num_primitives: 2,
                min_vertex_index: base_vertex,
                max_vertex_index: base_vertex + 3,
            });
        }
    }

    /// Draws a 2D line segment between two points in screen space (UnrealScript `Canvas.DrawLine`).
    ///
    /// Parameters:
    /// - `x1, y1`: Starting point coordinates in screen pixels.
    /// - `x2, y2`: Ending point coordinates in screen pixels.
    /// - `color`: RGBA line color.
    pub fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: [f32; 4]) {
        let base_vertex = self.vertices.len() as u32;
        let base_index = self.indices.len() as u32;

        let v0 = UIVertex::new([x1, y1], [0.0, 0.0], color);
        let v1 = UIVertex::new([x2, y2], [1.0, 1.0], color);

        self.vertices.push(v0);
        self.vertices.push(v1);

        let b = base_vertex as u16;
        self.indices.push(b);
        self.indices.push(b + 1);

        self.total_lines += 1;

        // Dynamic Batching: attempt to merge with the previous LineList batch
        let can_merge = if let Some(last) = self.batches.last() {
            last.primitive_type == PrimitiveType::LineList
                && last.texture.is_none()
                && last.blend_mode == self.style
                && last.scissor == self.current_clip
        } else {
            false
        };

        if can_merge {
            let last = self.batches.last_mut().unwrap();
            last.index_count += 2;
            last.num_primitives += 1;
            last.max_vertex_index = base_vertex + 1;
        } else {
            self.batches.push(CanvasDrawBatch {
                primitive_type: PrimitiveType::LineList,
                texture: None,
                blend_mode: self.style,
                scissor: self.current_clip,
                first_index: base_index,
                index_count: 2,
                vertex_offset: 0,
                num_primitives: 1,
                min_vertex_index: base_vertex,
                max_vertex_index: base_vertex + 1,
            });
        }
    }

    /// Draws a wireframe 2D rectangle (box) outline using four line segments.
    pub fn draw_box(&mut self, x: f32, y: f32, xl: f32, yl: f32, color: [f32; 4]) {
        self.draw_line(x, y, x + xl, y, color);
        self.draw_line(x + xl, y, x + xl, y + yl, color);
        self.draw_line(x + xl, y + yl, x, y + yl, color);
        self.draw_line(x, y + yl, x, y, color);
    }

    /// Returns the number of pending batches prior to flushing.
    #[inline]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// Returns the number of accumulated vertices.
    #[inline]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Returns the number of accumulated indices.
    #[inline]
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Flushes all batched 2D primitives and generates a [`CanvasFlushResult`].
    ///
    /// Constructs the complete sequence of low-level [`RenderCommand`]s
    /// (transforms, buffer bindings, scissor clipping, indexed draw calls)
    /// and clears internal scratch buffers for the next frame.
    pub fn flush_to_gpu(&mut self) -> CanvasFlushResult {
        let draw_call_count = self.batches.len() as u32;
        let mut triangle_count = 0u64;
        let mut commands = Vec::new();

        if !self.batches.is_empty() {
            // 1. Set 2D orthographic projection matrix
            commands.push(RenderCommand::SetTransform {
                transform_type: TransformType::Projection,
                matrix: self.ortho_projection(),
            });

            // 2. Set View and World identity matrices
            commands.push(RenderCommand::SetTransform {
                transform_type: TransformType::View,
                matrix: Mat4::IDENTITY,
            });
            commands.push(RenderCommand::SetTransform {
                transform_type: TransformType::World,
                matrix: Mat4::IDENTITY,
            });

            // 3. Bind UI vertex and index buffers
            commands.push(RenderCommand::BindVertexBuffer {
                binding: 0,
                buffer: self.vertex_buffer_handle,
                offset: 0,
            });
            commands.push(RenderCommand::BindIndexBuffer {
                buffer: self.index_buffer_handle,
                offset: 0,
                format: IndexFormat::Uint16,
            });

            // 4. Issue commands per batch
            let mut current_scissor: Option<ScissorRect> = None;
            for batch in &self.batches {
                if batch.scissor != current_scissor {
                    current_scissor = batch.scissor;
                    if let Some(scissor) = current_scissor {
                        commands.push(RenderCommand::SetScissor(scissor));
                    }
                }

                if batch.primitive_type == PrimitiveType::TriangleList {
                    triangle_count += batch.num_primitives as u64;
                }

                commands.push(RenderCommand::DrawIndexed {
                    index_count: batch.index_count,
                    instance_count: 1,
                    first_index: batch.first_index,
                    vertex_offset: batch.vertex_offset,
                    first_instance: 0,
                });
            }
        }

        let batches = std::mem::take(&mut self.batches);
        let vertices = std::mem::take(&mut self.vertices);
        let indices = std::mem::take(&mut self.indices);
        let quad_count = self.total_quads;
        let line_count = self.total_lines;

        self.total_quads = 0;
        self.total_lines = 0;

        CanvasFlushResult {
            batches,
            vertices,
            indices,
            commands,
            draw_call_count,
            quad_count,
            line_count,
            triangle_count,
        }
    }

    /// Submits all batched 2D draw commands directly into an active [`RenderInterface`].
    pub fn flush_to_render_interface(
        &mut self,
        render: &mut dyn RenderInterface,
    ) -> Result<CanvasFlushResult, RenderError> {
        let result = self.flush_to_gpu();

        if !result.batches.is_empty() {
            render.set_transform(TransformType::Projection, self.ortho_projection());
            render.set_transform(TransformType::View, Mat4::IDENTITY);
            render.set_transform(TransformType::World, Mat4::IDENTITY);

            render.set_vertex_buffer(0, self.vertex_buffer_handle, 0);
            render.set_index_buffer(self.index_buffer_handle, 0, IndexFormat::Uint16);

            let mut active_scissor: Option<ScissorRect> = None;
            for batch in &result.batches {
                if batch.scissor != active_scissor {
                    active_scissor = batch.scissor;
                    if let Some(scissor) = active_scissor {
                        render.set_scissor(scissor);
                    }
                }

                render.draw_indexed_primitive(
                    batch.primitive_type,
                    batch.first_index,
                    batch.num_primitives,
                    batch.min_vertex_index,
                    batch.max_vertex_index,
                );
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;
    use crate::device::ModernRenderDevice;
    use crate::hal::{ClearValues, RenderDevice};

    #[test]
    fn test_canvas_creation_and_defaults() {
        let canvas = Canvas2D::new(1920.0, 1080.0);
        assert_eq!(canvas.size_x, 1920.0);
        assert_eq!(canvas.size_y, 1080.0);
        assert_eq!(canvas.cur_x, 0.0);
        assert_eq!(canvas.cur_y, 0.0);
        assert_eq!(canvas.clip_x, 1920.0);
        assert_eq!(canvas.clip_y, 1080.0);
        assert_eq!(canvas.org_x, 0.0);
        assert_eq!(canvas.org_y, 0.0);
        assert!(canvas.current_clip.is_none());
        assert_eq!(canvas.draw_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(canvas.style, BlendMode::Translucent);
        assert_eq!(canvas.batch_count(), 0);
        assert_eq!(canvas.vertex_count(), 0);
        assert_eq!(canvas.index_count(), 0);
    }

    #[test]
    fn test_ortho_projection_matrix() {
        let width = 800.0;
        let height = 600.0;
        let proj = Canvas2D::ortho_projection_matrix(width, height);

        // Top-Left (0.0, 0.0) -> NDC (-1.0, 1.0, 0.0, 1.0)
        let tl = proj * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((tl.x - (-1.0)).abs() < 1e-4);
        assert!((tl.y - 1.0).abs() < 1e-4);
        assert!((tl.z - 0.0).abs() < 1e-4);

        // Bottom-Right (800.0, 600.0) -> NDC (1.0, -1.0, 0.0, 1.0)
        let br = proj * Vec4::new(800.0, 600.0, 0.0, 1.0);
        assert!((br.x - 1.0).abs() < 1e-4);
        assert!((br.y - (-1.0)).abs() < 1e-4);

        // Screen Center (400.0, 300.0) -> NDC (0.0, 0.0, 0.0, 1.0)
        let center = proj * Vec4::new(400.0, 300.0, 0.0, 1.0);
        assert!(center.x.abs() < 1e-4);
        assert!(center.y.abs() < 1e-4);
    }

    #[test]
    fn test_draw_tile_quad_geometry_and_indices() {
        let mut canvas = Canvas2D::new(1024.0, 768.0);
        let color = [1.0, 0.5, 0.25, 1.0];
        let tex = TextureHandle(42);

        canvas.draw_tile(10.0, 20.0, 100.0, 50.0, 0.0, 0.0, 1.0, 1.0, tex, color);

        assert_eq!(canvas.vertex_count(), 4);
        assert_eq!(canvas.index_count(), 6);
        assert_eq!(canvas.batch_count(), 1);

        // Check vertices
        let v0 = canvas.vertices[0];
        assert_eq!(v0.position, [10.0, 20.0]);
        assert_eq!(v0.uv, [0.0, 0.0]);
        assert_eq!(v0.color, color);

        let v1 = canvas.vertices[1];
        assert_eq!(v1.position, [110.0, 20.0]);
        assert_eq!(v1.uv, [1.0, 0.0]);

        let v2 = canvas.vertices[2];
        assert_eq!(v2.position, [110.0, 70.0]);
        assert_eq!(v2.uv, [1.0, 1.0]);

        let v3 = canvas.vertices[3];
        assert_eq!(v3.position, [10.0, 70.0]);
        assert_eq!(v3.uv, [0.0, 1.0]);

        // Check indices: [0, 1, 2, 0, 2, 3]
        assert_eq!(&canvas.indices[0..6], &[0, 1, 2, 0, 2, 3]);

        let batch = &canvas.batches[0];
        assert_eq!(batch.primitive_type, PrimitiveType::TriangleList);
        assert_eq!(batch.texture, Some(tex));
        assert_eq!(batch.blend_mode, BlendMode::Translucent);
        assert_eq!(batch.index_count, 6);
        assert_eq!(batch.num_primitives, 2);
    }

    #[test]
    fn test_draw_tile_dynamic_batching_same_texture() {
        let mut canvas = Canvas2D::new(1920.0, 1080.0);
        let tex = TextureHandle(1);
        let color = [1.0, 1.0, 1.0, 1.0];

        // Draw 3 tiles sharing same texture and state
        canvas.draw_tile(0.0, 0.0, 64.0, 64.0, 0.0, 0.0, 1.0, 1.0, tex, color);
        canvas.draw_tile(64.0, 0.0, 64.0, 64.0, 0.0, 0.0, 1.0, 1.0, tex, color);
        canvas.draw_tile(128.0, 0.0, 64.0, 64.0, 0.0, 0.0, 1.0, 1.0, tex, color);

        // All 3 quads batched into 1 GPU draw call
        assert_eq!(canvas.batch_count(), 1);
        assert_eq!(canvas.vertex_count(), 12);
        assert_eq!(canvas.index_count(), 18);

        let batch = &canvas.batches[0];
        assert_eq!(batch.index_count, 18);
        assert_eq!(batch.num_primitives, 6); // 6 triangles
        assert_eq!(batch.min_vertex_index, 0);
        assert_eq!(batch.max_vertex_index, 11);
    }

    #[test]
    fn test_draw_tile_batch_break_on_texture_change() {
        let mut canvas = Canvas2D::new(1920.0, 1080.0);
        let tex_a = TextureHandle(1);
        let tex_b = TextureHandle(2);
        let color = [1.0, 1.0, 1.0, 1.0];

        canvas.draw_tile(0.0, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0, 1.0, tex_a, color);
        canvas.draw_tile(32.0, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0, 1.0, tex_b, color);
        canvas.draw_tile(64.0, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0, 1.0, tex_b, color);

        // Texture change breaks batch into 2 batches (1 tile in first, 2 tiles in second)
        assert_eq!(canvas.batch_count(), 2);
        assert_eq!(canvas.batches[0].texture, Some(tex_a));
        assert_eq!(canvas.batches[0].index_count, 6);
        assert_eq!(canvas.batches[1].texture, Some(tex_b));
        assert_eq!(canvas.batches[1].index_count, 12);
    }

    #[test]
    fn test_draw_line_primitive_batching() {
        let mut canvas = Canvas2D::new(800.0, 600.0);
        let color = [0.0, 1.0, 0.0, 1.0];

        // Draw 2 connected lines
        canvas.draw_line(0.0, 0.0, 100.0, 100.0, color);
        canvas.draw_line(100.0, 100.0, 200.0, 50.0, color);

        // Merged into 1 LineList batch
        assert_eq!(canvas.batch_count(), 1);
        assert_eq!(canvas.vertex_count(), 4);
        assert_eq!(canvas.index_count(), 4);

        let batch = &canvas.batches[0];
        assert_eq!(batch.primitive_type, PrimitiveType::LineList);
        assert_eq!(batch.num_primitives, 2);
        assert_eq!(batch.index_count, 4);
    }

    #[test]
    fn test_set_clip_and_scissor_batching() {
        let mut canvas = Canvas2D::new(1920.0, 1080.0);
        let tex = TextureHandle(1);
        let color = [1.0, 1.0, 1.0, 1.0];

        // 1. Unclipped tile
        canvas.draw_tile(0.0, 0.0, 50.0, 50.0, 0.0, 0.0, 1.0, 1.0, tex, color);

        // 2. Set clipping box
        canvas.set_clip(100.0, 100.0, 400.0, 300.0);
        assert_eq!(
            canvas.current_clip,
            Some(ScissorRect {
                x: 100,
                y: 100,
                width: 400,
                height: 300
            })
        );

        // 3. Clipped tile
        canvas.draw_tile(120.0, 120.0, 50.0, 50.0, 0.0, 0.0, 1.0, 1.0, tex, color);

        // 4. Reset clipping
        canvas.reset_clip();
        assert!(canvas.current_clip.is_none());

        // 5. Unclipped tile
        canvas.draw_tile(200.0, 200.0, 50.0, 50.0, 0.0, 0.0, 1.0, 1.0, tex, color);

        // Scissor changes force 3 distinct batches
        assert_eq!(canvas.batch_count(), 3);
        assert!(canvas.batches[0].scissor.is_none());
        assert_eq!(
            canvas.batches[1].scissor,
            Some(ScissorRect {
                x: 100,
                y: 100,
                width: 400,
                height: 300
            })
        );
        assert!(canvas.batches[2].scissor.is_none());
    }

    #[test]
    fn test_flush_to_gpu_commands_and_clearing() {
        let mut canvas = Canvas2D::new(1920.0, 1080.0);
        let tex = TextureHandle(1);
        let color = [1.0, 0.0, 0.0, 1.0];

        canvas.set_clip(50.0, 50.0, 200.0, 200.0);
        canvas.draw_tile(60.0, 60.0, 80.0, 80.0, 0.0, 0.0, 1.0, 1.0, tex, color);
        canvas.draw_line(10.0, 10.0, 40.0, 40.0, color);

        let result = canvas.flush_to_gpu();

        assert_eq!(result.draw_call_count, 2); // 1 Quad batch + 1 Line batch
        assert_eq!(result.quad_count, 1);
        assert_eq!(result.line_count, 1);
        assert_eq!(result.triangle_count, 2);
        assert_eq!(result.vertices.len(), 6); // 4 for quad + 2 for line
        assert_eq!(result.indices.len(), 8); // 6 for quad + 2 for line

        // Commands sequence:
        // SetTransform(Projection), SetTransform(View), SetTransform(World),
        // BindVertexBuffer, BindIndexBuffer, SetScissor, DrawIndexed (tile), DrawIndexed (line)
        assert_eq!(result.commands.len(), 8);

        // Canvas internal state should be completely cleared after flush
        assert_eq!(canvas.batch_count(), 0);
        assert_eq!(canvas.vertex_count(), 0);
        assert_eq!(canvas.index_count(), 0);
    }

    #[test]
    fn test_flush_to_render_interface_integration() {
        let mut device = ModernRenderDevice::new(1920, 1080);
        device.init().unwrap();

        let iface = device.lock(ClearValues::default()).unwrap();

        let mut canvas = Canvas2D::new(1920.0, 1080.0);
        let tex = TextureHandle(5);
        canvas.draw_tile(10.0, 10.0, 100.0, 100.0, 0.0, 0.0, 1.0, 1.0, tex, [1.0, 1.0, 1.0, 1.0]);

        let result = canvas.flush_to_render_interface(iface).unwrap();
        assert_eq!(result.draw_call_count, 1);
        assert_eq!(result.quad_count, 1);

        device.unlock().unwrap();

        // 1 draw call was issued to the device, 2 triangles
        assert_eq!(device.frame_draw_calls, 1);
        assert_eq!(device.frame_triangles, 2);
    }
}
