use std::time::Instant;
use glam::Mat4;

use crate::error::RenderError;
use crate::hal::{
    BufferHandle, ClearValues, IndexFormat, PipelineHandle,
    PrimitiveType, RenderDevice, RenderInterface, ScissorRect,
    TransformType, Viewport,
};
use crate::pso::PipelineKey;
use crate::material::TranslatedMaterial;
use crate::scene::{SceneUniforms, StaticMeshSectionDraw};
use crate::device::ModernRenderDevice;
use crate::swapchain::{PresentResult, SwapchainManager};

/// 3D Camera re-exported from camera module
pub use crate::camera::Camera;

/// Mesh section render item contained in a 3D scene.
#[derive(Clone, Debug)]
pub struct SceneMeshItem {
    pub world_transform: Mat4,
    pub vertex_buffer: BufferHandle,
    pub index_buffer: BufferHandle,
    pub section: StaticMeshSectionDraw,
    pub material: Option<TranslatedMaterial>,
    pub index_format: IndexFormat,
}

impl SceneMeshItem {
    pub fn new(
        world_transform: Mat4,
        vertex_buffer: BufferHandle,
        index_buffer: BufferHandle,
        section: StaticMeshSectionDraw,
        material: Option<TranslatedMaterial>,
        index_format: IndexFormat,
    ) -> Self {
        Self {
            world_transform,
            vertex_buffer,
            index_buffer,
            section,
            material,
            index_format,
        }
    }
}

/// Scene holding renderable meshes, lights, and uniform data.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub meshes: Vec<SceneMeshItem>,
    pub ambient_light: [f32; 4],
    pub time_seconds: f32,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            ambient_light: [0.05, 0.05, 0.05, 1.0],
            time_seconds: 0.0,
        }
    }

    pub fn add_mesh(&mut self, item: SceneMeshItem) {
        self.meshes.push(item);
    }

    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    /// Builds standard uniform buffer structure for shaders.
    pub fn uniforms(&self, camera: &Camera, screen_size: [f32; 2]) -> SceneUniforms {
        SceneUniforms::new(
            camera.view_matrix(),
            camera.projection_matrix(),
            camera.position.to_array(),
            self.time_seconds,
            screen_size,
        )
    }
}

/// Swapchain / Window render surface target.
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub fullscreen: bool,
    pub refresh_rate: u32,
    pub device: ModernRenderDevice,
    pub swapchain: Option<SwapchainManager>,
}

impl Surface {
    /// Creates a surface with an underlying modern render device.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            vsync: true,
            fullscreen: false,
            refresh_rate: 60,
            device: ModernRenderDevice::new(width, height),
            swapchain: None,
        }
    }

    /// Creates a surface connected to a native Win32 `HWND` with full Vulkan swapchain management.
    pub fn with_swapchain(hwnd: usize, width: u32, height: u32) -> Result<Self, RenderError> {
        let swapchain = SwapchainManager::create(hwnd, width, height)?;
        Ok(Self {
            width,
            height,
            vsync: true,
            fullscreen: false,
            refresh_rate: 60,
            device: ModernRenderDevice::new(width, height),
            swapchain: Some(swapchain),
        })
    }

    /// Resizes the surface and its internal render device and swapchain.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.width = width;
        self.height = height;
        self.device.set_res(width, height, 4, self.fullscreen, self.refresh_rate)?;
        if let Some(ref mut swp) = self.swapchain {
            swp.recreate(width, height)?;
        }
        Ok(())
    }

    #[inline]
    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0 {
            self.width as f32 / self.height as f32
        } else {
            1.0
        }
    }
}

impl RenderDevice for Surface {
    fn init(&mut self) -> Result<(), RenderError> {
        self.device.init()
    }

    fn set_res(
        &mut self,
        width: u32,
        height: u32,
        color_bytes: u32,
        fullscreen: bool,
        refresh_rate: u32,
    ) -> Result<(), RenderError> {
        self.width = width;
        self.height = height;
        self.fullscreen = fullscreen;
        self.refresh_rate = refresh_rate;
        self.device.set_res(width, height, color_bytes, fullscreen, refresh_rate)
    }

    fn lock(&mut self, clear: ClearValues) -> Result<&mut dyn RenderInterface, RenderError> {
        self.device.lock(clear)
    }

    fn unlock(&mut self) -> Result<(), RenderError> {
        self.device.unlock()
    }

    fn present(&mut self) -> Result<(), RenderError> {
        self.device.present()
    }

    fn flush_resource(&mut self, cache_id: u64) {
        self.device.flush_resource(cache_id);
    }

    fn get_or_create_pipeline(&mut self, key: &PipelineKey) -> Result<PipelineHandle, RenderError> {
        self.device.get_or_create_pipeline(key)
    }
}

/// Statistics collected during a single rendered frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameStats {
    pub frame_index: u64,
    pub delta_time: f32,
    pub fps: f32,
    pub draw_calls: u32,
    pub triangle_count: u64,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub rendered: bool,
}

/// Real-time 3D viewport loop manager.
///
/// Connects camera, swapchain surface, and scene rendering into a continuous frame loop.
/// Manages frame delta time, clear color, render pass execution, and viewport resizing on WM_SIZE.
pub struct ViewportLoop {
    pub width: u32,
    pub height: u32,
    pub viewport: Viewport,
    pub scissor: ScissorRect,
    pub clear_color: [f32; 4],
    pub clear_depth: f32,
    pub delta_time: f32,
    pub total_time: f32,
    pub fps: f32,
    pub frame_count: u64,
    pub is_minimized: bool,
    pub resized_pending: bool,
    last_frame_instant: Option<Instant>,
    fixed_time_step: Option<f32>,
}

impl Default for ViewportLoop {
    fn default() -> Self {
        Self::new(1920, 1080)
    }
}

impl ViewportLoop {
    /// Creates a new viewport loop with the given dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let viewport = Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = ScissorRect {
            x: 0,
            y: 0,
            width,
            height,
        };

        Self {
            width,
            height,
            viewport,
            scissor,
            clear_color: [0.05, 0.05, 0.08, 1.0], // Dark charcoal/navy gaming background
            clear_depth: 1.0,
            delta_time: 0.0,
            total_time: 0.0,
            fps: 0.0,
            frame_count: 0,
            is_minimized: width == 0 || height == 0,
            resized_pending: false,
            last_frame_instant: None,
            fixed_time_step: None,
        }
    }

    /// Sets a fixed time step for deterministic testing / benchmarking.
    pub fn set_fixed_time_step(&mut self, step: Option<f32>) {
        self.fixed_time_step = step;
    }

    /// Configures the framebuffer clear color.
    pub fn set_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }

    /// Returns current framebuffer clear color.
    pub fn clear_color(&self) -> [f32; 4] {
        self.clear_color
    }

    /// Returns current frame delta time in seconds.
    pub fn delta_time(&self) -> f32 {
        self.delta_time
    }

    /// Returns total accumulated time in seconds.
    pub fn total_time(&self) -> f32 {
        self.total_time
    }

    /// Returns smoothed instantaneous frames per second.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Returns total completed frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Indicates if viewport is currently minimized (width or height == 0).
    pub fn is_minimized(&self) -> bool {
        self.is_minimized
    }

    /// Resizes viewport and scissor bounds, flagging pending swapchain update.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            self.is_minimized = true;
            return;
        }

        self.is_minimized = false;
        self.width = width;
        self.height = height;

        self.viewport = Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        self.scissor = ScissorRect {
            x: 0,
            y: 0,
            width,
            height,
        };

        self.resized_pending = true;
    }

    /// Handles Win32 `WM_SIZE` window message (`lparam` holds width/height, `wparam` indicates state).
    /// Returns true if a non-minimized resize was processed.
    pub fn handle_wm_size(&mut self, wparam: usize, lparam: isize) -> bool {
        const SIZE_MINIMIZED: usize = 1;

        if wparam == SIZE_MINIMIZED {
            self.is_minimized = true;
            return false;
        }

        let width = (lparam as usize & 0xFFFF) as u32;
        let height = ((lparam as usize >> 16) & 0xFFFF) as u32;

        self.resize(width, height);
        !self.is_minimized
    }

    /// Advances frame timing and delta time computation.
    fn advance_time(&mut self) -> f32 {
        let dt = if let Some(fixed) = self.fixed_time_step {
            fixed
        } else {
            let now = Instant::now();
            let elapsed = match self.last_frame_instant {
                Some(last) => now.duration_since(last).as_secs_f32(),
                None => 1.0 / 60.0,
            };
            self.last_frame_instant = Some(now);
            elapsed
        };

        self.delta_time = dt;
        self.total_time += dt;
        self.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        self.frame_count += 1;

        dt
    }

    /// Executes a single complete render frame:
    /// 1. Synchronizes swapchain surface dimensions on resize.
    /// 2. Binds View and Projection matrices from camera.
    /// 3. Executes render pass clear on surface.
    /// 4. Submits scene meshes, materials, and indexed draw primitives.
    /// 5. Ends render pass and presents surface swapchain.
    pub fn render_frame(
        &mut self,
        camera: &Camera,
        scene: &Scene,
        surface: &mut Surface,
    ) -> Result<FrameStats, RenderError> {
        let dt = self.advance_time();

        // 1. Guard against minimized window rendering (swapchain recreation deadlock avoidance)
        if self.is_minimized || surface.width == 0 || surface.height == 0 {
            return Ok(FrameStats {
                frame_index: self.frame_count,
                delta_time: dt,
                fps: self.fps,
                draw_calls: 0,
                triangle_count: 0,
                viewport_width: self.width,
                viewport_height: self.height,
                rendered: false,
            });
        }

        // 2. Synchronize swapchain resolution if a resize is pending
        if self.resized_pending || surface.width != self.width || surface.height != self.height {
            surface.resize(self.width, self.height)?;
            self.resized_pending = false;
        }

        // 3. Acquire next image from swapchain if connected
        if let Some(ref mut swp) = surface.swapchain {
            let acquired = swp.acquire_next_image()?;
            if acquired.is_suboptimal {
                self.resized_pending = true;
            }
        }

        // 4. Begin render pass with clear values
        let clear = ClearValues {
            color: self.clear_color,
            depth: self.clear_depth,
            stencil: 0,
            clear_color: true,
            clear_depth: true,
            clear_stencil: false,
        };

        let interface = surface.lock(clear)?;

        // 5. Set dynamic viewport and scissor states
        interface.set_viewport(self.viewport);
        interface.set_scissor(self.scissor);

        // 6. Set camera view and projection matrices
        interface.set_transform(TransformType::View, camera.view_matrix());
        interface.set_transform(TransformType::Projection, camera.projection_matrix());

        // 7. Record scene draw commands
        for item in &scene.meshes {
            // Apply Model-to-World transform
            interface.set_transform(TransformType::World, item.world_transform);

            // Bind material constants if present
            if let Some(ref mat) = item.material {
                interface.set_material(mat)?;
            }

            // Bind vertex and index streams
            interface.set_vertex_buffer(0, item.vertex_buffer, 0);
            interface.set_index_buffer(item.index_buffer, 0, item.index_format);

            // Issue indexed draw primitive call
            interface.draw_indexed_primitive(
                PrimitiveType::TriangleList,
                item.section.first_index,
                item.section.num_triangles(),
                item.section.min_vertex_index,
                item.section.max_vertex_index,
            );
        }

        let draw_calls = surface.device.frame_draw_calls;
        let triangle_count = surface.device.frame_triangles;

        // 8. End render pass and present swapchain backbuffer
        surface.unlock()?;
        surface.present()?;

        // Present swapchain if connected
        if let Some(ref mut swp) = surface.swapchain {
            let pres = swp.present()?;
            if pres == PresentResult::Suboptimal || pres == PresentResult::OutOfDate {
                self.resized_pending = true;
            }
        }

        Ok(FrameStats {
            frame_index: self.frame_count,
            delta_time: dt,
            fps: self.fps,
            draw_calls,
            triangle_count,
            viewport_width: self.width,
            viewport_height: self.height,
            rendered: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use crate::material::{MaterialDefinition, MaterialTranslator};

    #[test]
    fn test_viewport_loop_initialization() {
        let loop_mgr = ViewportLoop::new(1280, 720);
        assert_eq!(loop_mgr.width, 1280);
        assert_eq!(loop_mgr.height, 720);
        assert_eq!(loop_mgr.viewport.width, 1280.0);
        assert_eq!(loop_mgr.viewport.height, 720.0);
        assert_eq!(loop_mgr.scissor.width, 1280);
        assert_eq!(loop_mgr.scissor.height, 720);
        assert_eq!(loop_mgr.frame_count(), 0);
        assert_eq!(loop_mgr.delta_time(), 0.0);
        assert!(!loop_mgr.is_minimized());
    }

    #[test]
    fn test_viewport_loop_resize_and_wm_size() {
        let mut loop_mgr = ViewportLoop::new(1920, 1080);

        // Test direct resize
        loop_mgr.resize(2560, 1440);
        assert_eq!(loop_mgr.width, 2560);
        assert_eq!(loop_mgr.height, 1440);
        assert_eq!(loop_mgr.viewport.width, 2560.0);
        assert_eq!(loop_mgr.viewport.height, 1440.0);
        assert!(loop_mgr.resized_pending);

        // Test Win32 WM_SIZE packed message: LOWORD=800, HIWORD=600
        let lparam = (600 << 16) | 800;
        let handled = loop_mgr.handle_wm_size(0, lparam);
        assert!(handled);
        assert_eq!(loop_mgr.width, 800);
        assert_eq!(loop_mgr.height, 600);
        assert_eq!(loop_mgr.viewport.width, 800.0);
        assert_eq!(loop_mgr.viewport.height, 600.0);

        // Test Win32 WM_SIZE SIZE_MINIMIZED (wparam = 1)
        let handled_min = loop_mgr.handle_wm_size(1, lparam);
        assert!(!handled_min);
        assert!(loop_mgr.is_minimized());
    }

    #[test]
    fn test_camera_matrices_and_aspect_ratio() {
        let mut camera = Camera::new(Vec3::new(0.0, 100.0, 500.0), 90.0, 16.0 / 9.0);
        let view = camera.view_matrix();
        let proj = camera.projection_matrix();
        let view_proj = camera.view_projection_matrix();

        assert_ne!(view, Mat4::ZERO);
        assert_ne!(proj, Mat4::ZERO);
        assert_eq!(view_proj, proj * view);

        // Update aspect ratio on 1920x1080 -> 16/9
        camera.update_aspect_ratio(1920, 1080);
        let ratio = camera.aspect_ratio;
        assert!((ratio - (16.0 / 9.0)).abs() < 0.001);

        // Test UE2.5 rotator conversion (pitch=0, yaw=16384 -> 90 degrees)
        let ue_camera = Camera::from_ue_rotator(
            Vec3::new(10.0, 20.0, 30.0),
            0,
            16384,
            0,
            90.0,
            1.777,
        );
        assert!((ue_camera.yaw - (std::f32::consts::PI / 2.0)).abs() < 0.001);
    }

    #[test]
    fn test_render_frame_pass_execution() {
        let mut loop_mgr = ViewportLoop::new(1920, 1080);
        loop_mgr.set_fixed_time_step(Some(1.0 / 60.0));
        loop_mgr.set_clear_color(0.1, 0.2, 0.3, 1.0);

        let camera = Camera::new(Vec3::new(0.0, 50.0, -200.0), 90.0, 16.0 / 9.0);
        let mut scene = Scene::new();

        // Create sample material
        let def = MaterialDefinition::default();
        let mat = MaterialTranslator::translate(&def, 0.0);

        // Create sample static mesh section
        let section = StaticMeshSectionDraw {
            material_index: 0,
            first_index: 0,
            index_count: 36, // 12 triangles
            min_vertex_index: 0,
            max_vertex_index: 23,
        };

        scene.add_mesh(SceneMeshItem::new(
            Mat4::IDENTITY,
            BufferHandle(101),
            BufferHandle(102),
            section,
            Some(mat),
            IndexFormat::Uint16,
        ));

        let mut surface = Surface::new(1920, 1080);

        // Render frame 1
        let stats = loop_mgr.render_frame(&camera, &scene, &mut surface).unwrap();

        assert!(stats.rendered);
        assert_eq!(stats.frame_index, 1);
        assert_eq!(stats.draw_calls, 1);
        assert_eq!(stats.triangle_count, 12);
        assert_eq!(stats.viewport_width, 1920);
        assert_eq!(stats.viewport_height, 1080);
        assert!((stats.delta_time - (1.0 / 60.0)).abs() < 0.001);
        assert!((stats.fps - 60.0).abs() < 0.1);

        // Verify device state after present
        assert_eq!(surface.device.frame_index, 1);
        assert_eq!(loop_mgr.frame_count(), 1);
    }

    #[test]
    fn test_render_frame_with_swapchain() {
        let mut loop_mgr = ViewportLoop::new(1920, 1080);
        loop_mgr.set_fixed_time_step(Some(1.0 / 60.0));

        let camera = Camera::default();
        let scene = Scene::new();

        // Surface with mock Win32 HWND 0x1001
        let mut surface = Surface::with_swapchain(0x1001, 1920, 1080).unwrap();
        assert!(surface.swapchain.is_some());

        let stats = loop_mgr.render_frame(&camera, &scene, &mut surface).unwrap();
        assert!(stats.rendered);
        assert_eq!(stats.frame_index, 1);
        assert_eq!(surface.device.frame_index, 1);
    }

    #[test]
    fn test_render_frame_when_minimized() {
        let mut loop_mgr = ViewportLoop::new(1920, 1080);
        loop_mgr.resize(0, 0); // Window minimized
        assert!(loop_mgr.is_minimized());

        let camera = Camera::default();
        let scene = Scene::new();
        let mut surface = Surface::new(1920, 1080);

        let stats = loop_mgr.render_frame(&camera, &scene, &mut surface).unwrap();
        assert!(!stats.rendered);
        assert_eq!(stats.draw_calls, 0);
    }

    #[test]
    fn test_render_frame_automatic_surface_resize() {
        let mut loop_mgr = ViewportLoop::new(800, 600);
        loop_mgr.resize(1280, 720); // Resize event occurred

        let camera = Camera::default();
        let scene = Scene::new();
        let mut surface = Surface::new(800, 600);

        assert_eq!(surface.width, 800);
        assert_eq!(surface.height, 600);

        // render_frame must synchronize surface to loop_mgr dimensions
        let stats = loop_mgr.render_frame(&camera, &scene, &mut surface).unwrap();
        assert!(stats.rendered);
        assert_eq!(surface.width, 1280);
        assert_eq!(surface.height, 720);
        assert_eq!(stats.viewport_width, 1280);
        assert_eq!(stats.viewport_height, 720);
    }
}
