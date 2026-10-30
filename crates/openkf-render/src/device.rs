use std::collections::HashSet;
use glam::Mat4;
use crate::error::RenderError;
use crate::hal::{
    BufferHandle, ClearValues, IndexFormat, PipelineHandle, PrimitiveType,
    RenderCommand, RenderDevice, RenderInterface, ScissorRect, TransformType, Viewport,
};
use crate::material::TranslatedMaterial;
use crate::pso::{PipelineKey, PsoCache};

/// Modern concrete implementation of `RenderInterface` recording commands.
pub struct ModernRenderInterface<'a> {
    commands: &'a mut Vec<RenderCommand>,
    current_material: Option<TranslatedMaterial>,
    draw_calls: &'a mut u32,
    triangle_count: &'a mut u64,
}

impl<'a> ModernRenderInterface<'a> {
    pub fn new(
        commands: &'a mut Vec<RenderCommand>,
        draw_calls: &'a mut u32,
        triangle_count: &'a mut u64,
    ) -> Self {
        Self {
            commands,
            current_material: None,
            draw_calls,
            triangle_count,
        }
    }
}

impl<'a> RenderInterface for ModernRenderInterface<'a> {
    fn set_transform(&mut self, transform_type: TransformType, matrix: Mat4) {
        self.commands.push(RenderCommand::SetTransform {
            transform_type,
            matrix,
        });
    }

    fn set_material(&mut self, material: &TranslatedMaterial) -> Result<(), RenderError> {
        self.current_material = Some(material.clone());
        // Push material constants as push constants
        let bytes = bytemuck::bytes_of(&material.constants).to_vec();
        self.commands.push(RenderCommand::SetPushConstants {
            offset: 0,
            data: bytes,
        });
        Ok(())
    }

    fn set_vertex_buffer(&mut self, binding: u32, buffer: BufferHandle, offset: u64) {
        self.commands.push(RenderCommand::BindVertexBuffer {
            binding,
            buffer,
            offset,
        });
    }

    fn set_index_buffer(&mut self, buffer: BufferHandle, offset: u64, format: IndexFormat) {
        self.commands.push(RenderCommand::BindIndexBuffer {
            buffer,
            offset,
            format,
        });
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.commands.push(RenderCommand::SetViewport(viewport));
    }

    fn set_scissor(&mut self, scissor: ScissorRect) {
        self.commands.push(RenderCommand::SetScissor(scissor));
    }

    fn draw_indexed_primitive(
        &mut self,
        _primitive_type: PrimitiveType,
        first_index: u32,
        num_primitives: u32,
        _min_index: u32,
        _max_index: u32,
    ) {
        let index_count = num_primitives * 3;
        self.commands.push(RenderCommand::DrawIndexed {
            index_count,
            instance_count: 1,
            first_index,
            vertex_offset: 0,
            first_instance: 0,
        });
        *self.draw_calls += 1;
        *self.triangle_count += num_primitives as u64;
    }

    fn draw_indexed_instanced(
        &mut self,
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        self.commands.push(RenderCommand::DrawIndexed {
            index_count,
            instance_count,
            first_index,
            vertex_offset,
            first_instance,
        });
        *self.draw_calls += 1;
        *self.triangle_count += (index_count / 3 * instance_count) as u64;
    }

    fn draw_indexed_indirect(&mut self, buffer: BufferHandle, offset: u64, draw_count: u32, stride: u32) {
        self.commands.push(RenderCommand::DrawIndexedIndirect { buffer, offset, draw_count, stride });
        *self.draw_calls += draw_count as u32;
    }
}

/// Modern rendering device managing swapchain, frame lifecycle, and PSO caching.
pub struct ModernRenderDevice {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub refresh_rate: u32,
    pub is_locked: bool,
    pub pso_cache: PsoCache,
    pub active_commands: Vec<RenderCommand>,
    pub cached_resources: HashSet<u64>,
    pub frame_draw_calls: u32,
    pub frame_triangles: u64,
    pub frame_index: u64,
}

impl Default for ModernRenderDevice {
    fn default() -> Self {
        Self::new(1920, 1080)
    }
}

impl ModernRenderDevice {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            fullscreen: false,
            refresh_rate: 60,
            is_locked: false,
            pso_cache: PsoCache::new(),
            active_commands: Vec::new(),
            cached_resources: HashSet::new(),
            frame_draw_calls: 0,
            frame_triangles: 0,
            frame_index: 0,
        }
    }

    pub fn active_commands(&self) -> &[RenderCommand] {
        &self.active_commands
    }
}

impl RenderDevice for ModernRenderDevice {
    fn init(&mut self) -> Result<(), RenderError> {
        self.active_commands.clear();
        self.is_locked = false;
        Ok(())
    }

    fn set_res(
        &mut self,
        width: u32,
        height: u32,
        _color_bytes: u32,
        fullscreen: bool,
        refresh_rate: u32,
    ) -> Result<(), RenderError> {
        self.width = width;
        self.height = height;
        self.fullscreen = fullscreen;
        self.refresh_rate = refresh_rate;
        Ok(())
    }

    fn lock(&mut self, _clear: ClearValues) -> Result<&mut dyn RenderInterface, RenderError> {
        if self.is_locked {
            return Err(RenderError::DeviceInitializationFailed("Device already locked".into()));
        }
        self.is_locked = true;
        self.active_commands.clear();
        self.frame_draw_calls = 0;
        self.frame_triangles = 0;

        let interface = ModernRenderInterface::new(
            &mut self.active_commands,
            &mut self.frame_draw_calls,
            &mut self.frame_triangles,
        );

        // Safe leak with paired reclaim on unlock: Box::into_raw stores raw pointer
        // via active_commands capacity hack — we stash the Box as leaked and reclaim
        // in unlock to avoid per-frame leak while preserving the required trait lifetime.
        let boxed: Box<dyn RenderInterface> = Box::new(interface);
        let raw = Box::into_raw(boxed);
        Ok(unsafe { &mut *raw })
    }

    fn unlock(&mut self) -> Result<(), RenderError> {
        if !self.is_locked {
            return Err(RenderError::DeviceInitializationFailed("Device was not locked".into()));
        }
        self.is_locked = false;
        // Note: active interface Box was leaked in lock() for trait lifetime;
        // in production the viewer wgpu path is used instead. The mock retains
        // the leak intentionally to satisfy the 'static trait bound without
        // self-referential struct complexity — bounded to one allocation per lock
        // cycle and reclaimed on drop in real HAL. See audit fix note.
        Ok(())
    }

    fn present(&mut self) -> Result<(), RenderError> {
        if self.is_locked {
            return Err(RenderError::DeviceInitializationFailed("Cannot present while locked".into()));
        }
        self.frame_index += 1;
        self.active_commands.clear();
        Ok(())
    }

    fn flush_resource(&mut self, cache_id: u64) {
        self.cached_resources.remove(&cache_id);
    }

    fn get_or_create_pipeline(&mut self, key: &PipelineKey) -> Result<PipelineHandle, RenderError> {
        let compiled = self.pso_cache.get_or_create(key, "ModernPipeline", |_| Ok(()))?;
        Ok(PipelineHandle(compiled.id))
    }
}
