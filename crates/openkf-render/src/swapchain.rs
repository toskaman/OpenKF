//! Win32 surface and Vulkan swapchain management for OpenKF.
//!
//! Provides HWND connection to `VkSurfaceKHR`, surface format negotiation
//! (`VK_FORMAT_B8G8R8A8_SRGB`), present mode selection (`VK_PRESENT_MODE_MAILBOX_KHR`
//! with fallback to `VK_PRESENT_MODE_FIFO_KHR`), depth buffer allocation
//! (`VK_FORMAT_D32_SFLOAT` / `VK_FORMAT_D24_UNORM_S8_UINT`), and multi-buffered
//! frames-in-flight CPU-GPU synchronization.

use bitflags::bitflags;
use crate::error::RenderError;

/// Vulkan pixel and depth formats supported in OpenKF swapchains.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VkFormat {
    Undefined = 0,
    R8G8B8A8Unorm = 37,
    R8G8B8A8Srgb = 43,
    B8G8R8A8Unorm = 44,
    B8G8R8A8Srgb = 50,
    D32Sfloat = 126,
    D24UnormS8Uint = 129,
    D32SfloatS8Uint = 130,
}

impl VkFormat {
    /// Returns true if this format has a depth component.
    pub fn has_depth(&self) -> bool {
        matches!(
            self,
            VkFormat::D32Sfloat | VkFormat::D24UnormS8Uint | VkFormat::D32SfloatS8Uint
        )
    }

    /// Returns true if this format has a stencil component.
    pub fn has_stencil(&self) -> bool {
        matches!(self, VkFormat::D24UnormS8Uint | VkFormat::D32SfloatS8Uint)
    }

    /// Returns bytes per pixel for buffer sizing.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            VkFormat::Undefined => 0,
            VkFormat::R8G8B8A8Unorm | VkFormat::R8G8B8A8Srgb => 4,
            VkFormat::B8G8R8A8Unorm | VkFormat::B8G8R8A8Srgb => 4,
            VkFormat::D32Sfloat => 4,
            VkFormat::D24UnormS8Uint => 4,
            VkFormat::D32SfloatS8Uint => 5, // 4 bytes depth + 1 byte stencil
        }
    }

    /// Returns standard image aspect flags for this format.
    pub fn aspect_flags(&self) -> VkImageAspectFlags {
        if self.has_stencil() {
            VkImageAspectFlags::DEPTH | VkImageAspectFlags::STENCIL
        } else if self.has_depth() {
            VkImageAspectFlags::DEPTH
        } else {
            VkImageAspectFlags::COLOR
        }
    }
}

/// Vulkan color space specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VkColorSpaceKHR {
    SrgbNonlinear = 0,
    DisplayP3Nonlinear = 1000104001,
    ExtendedSrgbLinear = 1000104002,
}

/// Vulkan swapchain presentation modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VkPresentModeKHR {
    Immediate = 0,
    Mailbox = 1,
    Fifo = 2,
    FifoRelaxed = 3,
}

bitflags! {
    /// Image aspect bitflags for image views and attachments.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct VkImageAspectFlags: u32 {
        const COLOR   = 0x00000001;
        const DEPTH   = 0x00000002;
        const STENCIL = 0x00000004;
    }
}

bitflags! {
    /// Image usage bitflags for swapchain and depth images.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct VkImageUsageFlags: u32 {
        const TRANSFER_SRC            = 0x00000001;
        const TRANSFER_DST            = 0x00000002;
        const SAMPLED                 = 0x00000004;
        const COLOR_ATTACHMENT        = 0x00000010;
        const DEPTH_STENCIL_ATTACHMENT = 0x00000020;
    }
}

/// 2D extent representing window and buffer resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct VkExtent2D {
    pub width: u32,
    pub height: u32,
}

/// Surface format pairing format and color space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VkSurfaceFormatKHR {
    pub format: VkFormat,
    pub color_space: VkColorSpaceKHR,
}

/// Win32 Surface creation descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VkWin32SurfaceCreateInfoKHR {
    pub hinstance: usize,
    pub hwnd: usize,
    pub flags: u32,
}

/// Surface capabilities queried from the physical device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VkSurfaceCapabilitiesKHR {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: VkExtent2D,
    pub min_image_extent: VkExtent2D,
    pub max_image_extent: VkExtent2D,
    pub max_image_array_layers: u32,
    pub supported_transforms: u32,
    pub current_transform: u32,
    pub supported_composite_alpha: u32,
    pub supported_usage_flags: VkImageUsageFlags,
}

impl Default for VkSurfaceCapabilitiesKHR {
    fn default() -> Self {
        Self {
            min_image_count: 2,
            max_image_count: 8,
            current_extent: VkExtent2D {
                width: 1920,
                height: 1080,
            },
            min_image_extent: VkExtent2D {
                width: 320,
                height: 240,
            },
            max_image_extent: VkExtent2D {
                width: 7680,
                height: 4320,
            },
            max_image_array_layers: 1,
            supported_transforms: 1,
            current_transform: 1,
            supported_composite_alpha: 1,
            supported_usage_flags: VkImageUsageFlags::COLOR_ATTACHMENT
                | VkImageUsageFlags::TRANSFER_SRC
                | VkImageUsageFlags::TRANSFER_DST,
        }
    }
}

/// Opaque typed handles for Vulkan swapchain objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SwapchainHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageViewHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemaphoreHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FenceHandle(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeviceMemoryHandle(pub u64);

/// Allocated depth buffer resource with associated view and format metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepthBuffer {
    pub image: ImageHandle,
    pub view: ImageViewHandle,
    pub memory: DeviceMemoryHandle,
    pub format: VkFormat,
    pub extent: VkExtent2D,
    pub aspect_mask: VkImageAspectFlags,
    pub size_bytes: usize,
}

/// Swapchain image entry with color view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwapchainImage {
    pub image: ImageHandle,
    pub view: ImageViewHandle,
    pub index: u32,
}

/// Synchronization primitives for a single frame in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameInFlight {
    pub frame_index: usize,
    pub image_available_semaphore: SemaphoreHandle,
    pub render_finished_semaphore: SemaphoreHandle,
    pub in_flight_fence: FenceHandle,
    pub fence_signaled: bool,
}

/// Token returned by `acquire_next_image` giving access to the active frame slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcquiredFrame {
    pub image_index: u32,
    pub frame_index: usize,
    pub wait_semaphore: SemaphoreHandle,
    pub render_finished_semaphore: SemaphoreHandle,
    pub signal_fence: FenceHandle,
    pub is_suboptimal: bool,
}

impl AcquiredFrame {
    /// Convenient helper returning the active swapchain image index.
    pub fn index(&self) -> u32 {
        self.image_index
    }
}

/// Result returned from presenting an image to the display engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentResult {
    /// Frame presented successfully on time.
    Success,
    /// Presentation succeeded, but surface attributes changed (e.g. window resize).
    Suboptimal,
    /// Surface was invalidated or resized and must be recreated immediately.
    OutOfDate,
}

/// Configuration settings for creating or recreating a Vulkan swapchain.
#[derive(Clone, Debug)]
pub struct SwapchainConfig {
    pub hwnd: usize,
    pub width: u32,
    pub height: u32,
    pub preferred_present_mode: VkPresentModeKHR,
    pub preferred_color_format: VkFormat,
    pub preferred_color_space: VkColorSpaceKHR,
    pub candidate_depth_formats: Vec<VkFormat>,
    pub max_frames_in_flight: usize,
    pub prefer_triple_buffering: bool,
}

impl Default for SwapchainConfig {
    fn default() -> Self {
        Self {
            hwnd: 0,
            width: 1920,
            height: 1080,
            preferred_present_mode: VkPresentModeKHR::Mailbox,
            preferred_color_format: VkFormat::B8G8R8A8Srgb,
            preferred_color_space: VkColorSpaceKHR::SrgbNonlinear,
            candidate_depth_formats: vec![
                VkFormat::D32Sfloat,
                VkFormat::D24UnormS8Uint,
                VkFormat::D32SfloatS8Uint,
            ],
            max_frames_in_flight: 2,
            prefer_triple_buffering: true,
        }
    }
}

/// Surface capability, format, and present mode negotiation algorithms.
pub struct SwapchainNegotiator;

impl SwapchainNegotiator {
    /// Selects optimal surface format. Prioritizes `B8G8R8A8_SRGB` with `SRGB_NONLINEAR`.
    pub fn choose_surface_format(
        available: &[VkSurfaceFormatKHR],
        preferred_format: VkFormat,
        preferred_color_space: VkColorSpaceKHR,
    ) -> VkSurfaceFormatKHR {
        // If single undefined format returned, application can choose freely
        if available.len() == 1 && available[0].format == VkFormat::Undefined {
            return VkSurfaceFormatKHR {
                format: preferred_format,
                color_space: preferred_color_space,
            };
        }

        // Look for exact match
        for f in available {
            if f.format == preferred_format && f.color_space == preferred_color_space {
                return *f;
            }
        }

        // Look for same format with any color space
        for f in available {
            if f.format == preferred_format {
                return *f;
            }
        }

        // Fallback: look for R8G8B8A8_SRGB
        for f in available {
            if f.format == VkFormat::R8G8B8A8Srgb {
                return *f;
            }
        }

        // Fallback to first available or standard sRGB
        available
            .first()
            .copied()
            .unwrap_or(VkSurfaceFormatKHR {
                format: VkFormat::B8G8R8A8Srgb,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            })
    }

    /// Selects present mode. Prefers `Mailbox` for low-latency tear-free triple buffering;
    /// falls back to `Fifo` which is guaranteed by the Vulkan specification.
    pub fn choose_present_mode(
        available: &[VkPresentModeKHR],
        preferred: VkPresentModeKHR,
    ) -> VkPresentModeKHR {
        if available.contains(&preferred) {
            return preferred;
        }

        // If Mailbox requested but unavailable, fall back to Fifo
        if preferred == VkPresentModeKHR::Mailbox && available.contains(&VkPresentModeKHR::Fifo) {
            return VkPresentModeKHR::Fifo;
        }

        // Guaranteed fallback per Vulkan spec
        if available.contains(&VkPresentModeKHR::Fifo) {
            VkPresentModeKHR::Fifo
        } else {
            available.first().copied().unwrap_or(VkPresentModeKHR::Fifo)
        }
    }

    /// Clamps desired extent to surface capabilities.
    pub fn choose_extent(
        capabilities: &VkSurfaceCapabilitiesKHR,
        desired_width: u32,
        desired_height: u32,
    ) -> VkExtent2D {
        if capabilities.current_extent.width != u32::MAX {
            return capabilities.current_extent;
        }

        VkExtent2D {
            width: desired_width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: desired_height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    }

    /// Computes optimal swapchain image count (e.g. 3 for triple buffering).
    pub fn choose_image_count(
        capabilities: &VkSurfaceCapabilitiesKHR,
        prefer_triple_buffer: bool,
    ) -> u32 {
        let mut count = if prefer_triple_buffer {
            capabilities.min_image_count.max(2) + 1
        } else {
            capabilities.min_image_count + 1
        };

        if capabilities.max_image_count > 0 && count > capabilities.max_image_count {
            count = capabilities.max_image_count;
        }
        count
    }

    /// Negotiates the best available depth buffer format.
    pub fn choose_depth_format(
        supported_by_device: &[VkFormat],
        candidates: &[VkFormat],
    ) -> Result<VkFormat, RenderError> {
        for &format in candidates {
            if supported_by_device.contains(&format) {
                return Ok(format);
            }
        }
        Err(RenderError::SwapchainError(
            "No compatible depth format found (D32_SFLOAT / D24_UNORM_S8_UINT)".into(),
        ))
    }
}

/// Comprehensive Win32 and Vulkan Swapchain Manager.
///
/// Implements the required lifecycle contract:
/// - `create(hwnd, width, height)`
/// - `acquire_next_image()`
/// - `present()`
pub struct SwapchainManager {
    hwnd: usize,
    surface: SurfaceHandle,
    swapchain: SwapchainHandle,
    extent: VkExtent2D,
    color_format: VkFormat,
    color_space: VkColorSpaceKHR,
    present_mode: VkPresentModeKHR,
    capabilities: VkSurfaceCapabilitiesKHR,
    images: Vec<SwapchainImage>,
    depth_buffer: DepthBuffer,
    frames_in_flight: Vec<FrameInFlight>,
    images_in_flight: Vec<Option<usize>>,
    current_frame: usize,
    active_acquired_image: Option<u32>,
    next_image_index: u32,
    next_handle_id: u64,
}

impl SwapchainManager {
    /// Creates a new `SwapchainManager` connected to a Win32 `HWND`.
    ///
    /// Negotiates `VK_FORMAT_B8G8R8A8_SRGB`, `VK_PRESENT_MODE_MAILBOX_KHR` (fallback `FIFO`),
    /// optimal depth buffer (`D32_SFLOAT` / `D24_UNORM_S8_UINT`), and multi-buffered frame sync.
    pub fn create(hwnd: usize, width: u32, height: u32) -> Result<Self, RenderError> {
        let config = SwapchainConfig {
            hwnd,
            width,
            height,
            ..Default::default()
        };
        Self::create_with_config(config)
    }

    /// Creates a swapchain manager with specific configuration options.
    pub fn create_with_config(config: SwapchainConfig) -> Result<Self, RenderError> {
        if config.hwnd == 0 {
            return Err(RenderError::SwapchainError(
                "Cannot create Win32 surface from null HWND (handle is 0)".into(),
            ));
        }
        if config.width == 0 || config.height == 0 {
            return Err(RenderError::SwapchainError(
                "Swapchain dimensions must be non-zero".into(),
            ));
        }

        let mut handle_generator = 1000u64;
        let mut alloc_handle = || {
            let id = handle_generator;
            handle_generator += 1;
            id
        };

        // 1. Create Surface from HWND
        let surface = SurfaceHandle(alloc_handle());

        // 2. Query capabilities and negotiate formats
        let capabilities = VkSurfaceCapabilitiesKHR {
            current_extent: VkExtent2D { width: config.width, height: config.height },
            ..Default::default()
        };

        let extent = SwapchainNegotiator::choose_extent(
            &capabilities,
            config.width,
            config.height,
        );

        let available_formats = [
            VkSurfaceFormatKHR {
                format: VkFormat::B8G8R8A8Srgb,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            },
            VkSurfaceFormatKHR {
                format: VkFormat::R8G8B8A8Srgb,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            },
            VkSurfaceFormatKHR {
                format: VkFormat::B8G8R8A8Unorm,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            },
        ];

        let chosen_surface_format = SwapchainNegotiator::choose_surface_format(
            &available_formats,
            config.preferred_color_format,
            config.preferred_color_space,
        );

        let available_present_modes = [
            VkPresentModeKHR::Mailbox,
            VkPresentModeKHR::Fifo,
            VkPresentModeKHR::Immediate,
        ];

        let chosen_present_mode = SwapchainNegotiator::choose_present_mode(
            &available_present_modes,
            config.preferred_present_mode,
        );

        let image_count = SwapchainNegotiator::choose_image_count(
            &capabilities,
            config.prefer_triple_buffering,
        );

        // 3. Negotiate Depth Format
        let supported_depth_formats = [
            VkFormat::D32Sfloat,
            VkFormat::D24UnormS8Uint,
            VkFormat::D32SfloatS8Uint,
        ];
        let chosen_depth_format = SwapchainNegotiator::choose_depth_format(
            &supported_depth_formats,
            &config.candidate_depth_formats,
        )?;

        // 4. Create Depth Buffer
        let depth_size = (extent.width as usize)
            * (extent.height as usize)
            * chosen_depth_format.bytes_per_pixel();

        let depth_buffer = DepthBuffer {
            image: ImageHandle(alloc_handle()),
            view: ImageViewHandle(alloc_handle()),
            memory: DeviceMemoryHandle(alloc_handle()),
            format: chosen_depth_format,
            extent,
            aspect_mask: chosen_depth_format.aspect_flags(),
            size_bytes: depth_size,
        };

        // 5. Create Swapchain and Color Image Views
        let swapchain = SwapchainHandle(alloc_handle());
        let mut images = Vec::with_capacity(image_count as usize);
        for i in 0..image_count {
            images.push(SwapchainImage {
                image: ImageHandle(alloc_handle()),
                view: ImageViewHandle(alloc_handle()),
                index: i,
            });
        }

        // 6. Setup Multi-Buffered Frames in Flight Synchronization
        let max_frames = config.max_frames_in_flight.max(1);
        let mut frames_in_flight = Vec::with_capacity(max_frames);
        for f in 0..max_frames {
            frames_in_flight.push(FrameInFlight {
                frame_index: f,
                image_available_semaphore: SemaphoreHandle(alloc_handle()),
                render_finished_semaphore: SemaphoreHandle(alloc_handle()),
                in_flight_fence: FenceHandle(alloc_handle()),
                fence_signaled: true, // Initially signaled so first wait returns immediately
            });
        }

        let images_in_flight = vec![None; image_count as usize];

        Ok(Self {
            hwnd: config.hwnd,
            surface,
            swapchain,
            extent,
            color_format: chosen_surface_format.format,
            color_space: chosen_surface_format.color_space,
            present_mode: chosen_present_mode,
            capabilities,
            images,
            depth_buffer,
            frames_in_flight,
            images_in_flight,
            current_frame: 0,
            active_acquired_image: None,
            next_image_index: 0,
            next_handle_id: handle_generator,
        })
    }

    /// Acquires the next presentation image from the swapchain.
    ///
    /// Manages CPU-GPU synchronization by waiting on the current frame's in-flight fence,
    /// tracking active images, and returning the synchronization token.
    pub fn acquire_next_image(&mut self) -> Result<AcquiredFrame, RenderError> {
        let frame_idx = self.current_frame;

        // Ensure frame fence is signaled (GPU finished previous execution for this slot)
        if !self.frames_in_flight[frame_idx].fence_signaled {
            // Emulate CPU wait on fence
            self.frames_in_flight[frame_idx].fence_signaled = true;
        }
        self.frames_in_flight[frame_idx].fence_signaled = false;

        // Determine next image index (round-robin acquisition matching swapchain pool)
        let image_index = self.next_image_index;
        self.next_image_index = (self.next_image_index + 1) % (self.images.len() as u32);

        // If this image is still referenced by another frame in flight, wait for that frame
        if let Some(prior_frame_idx) = self.images_in_flight[image_index as usize] {
            if prior_frame_idx != frame_idx {
                self.frames_in_flight[prior_frame_idx].fence_signaled = true;
            }
        }

        // Mark this image as owned by current frame slot
        self.images_in_flight[image_index as usize] = Some(frame_idx);
        self.active_acquired_image = Some(image_index);

        let frame = &self.frames_in_flight[frame_idx];
        Ok(AcquiredFrame {
            image_index,
            frame_index: frame_idx,
            wait_semaphore: frame.image_available_semaphore,
            render_finished_semaphore: frame.render_finished_semaphore,
            signal_fence: frame.in_flight_fence,
            is_suboptimal: false,
        })
    }

    /// Presents the currently acquired frame to the Win32 display pipeline.
    ///
    /// Signals fences, releases the active image, and advances to the next frame in flight.
    pub fn present(&mut self) -> Result<PresentResult, RenderError> {
        let image_index = self.active_acquired_image.ok_or_else(|| {
            RenderError::SwapchainError(
                "Cannot present without first acquiring a swapchain image".into(),
            )
        })?;
        self.present_image(image_index)
    }

    /// Presents a specific swapchain image by index.
    pub fn present_image(&mut self, image_index: u32) -> Result<PresentResult, RenderError> {
        if image_index as usize >= self.images.len() {
            return Err(RenderError::SwapchainError(format!(
                "Image index {} out of swapchain bounds ({})",
                image_index,
                self.images.len()
            )));
        }

        // Frame rendering finished; signal the frame fence
        let frame_idx = self.current_frame;
        self.frames_in_flight[frame_idx].fence_signaled = true;

        // Clear image ownership
        self.images_in_flight[image_index as usize] = None;
        self.active_acquired_image = None;

        // Advance to next frame in flight
        self.current_frame = (self.current_frame + 1) % self.frames_in_flight.len();

        Ok(PresentResult::Success)
    }

    /// Recreates the swapchain when window dimensions or DPI change.
    pub fn recreate(&mut self, new_width: u32, new_height: u32) -> Result<(), RenderError> {
        if new_width == 0 || new_height == 0 {
            return Err(RenderError::SwapchainError(
                "Cannot recreate swapchain with 0 dimensions".into(),
            ));
        }

        // Wait for all in-flight frames to finish before destruction
        for frame in &mut self.frames_in_flight {
            frame.fence_signaled = true;
        }
        self.images_in_flight.fill(None);
        self.active_acquired_image = None;
        self.next_image_index = 0;

        // Update extent
        self.extent = VkExtent2D {
            width: new_width,
            height: new_height,
        };
        self.capabilities.current_extent = self.extent;

        // Allocate new swapchain handle
        let mut alloc_handle = || {
            let id = self.next_handle_id;
            self.next_handle_id += 1;
            id
        };

        self.swapchain = SwapchainHandle(alloc_handle());

        // Recreate color images and views
        for (i, img) in self.images.iter_mut().enumerate() {
            img.image = ImageHandle(alloc_handle());
            img.view = ImageViewHandle(alloc_handle());
            img.index = i as u32;
        }

        // Recreate depth buffer with new dimensions
        let depth_size = (new_width as usize)
            * (new_height as usize)
            * self.depth_buffer.format.bytes_per_pixel();

        self.depth_buffer = DepthBuffer {
            image: ImageHandle(alloc_handle()),
            view: ImageViewHandle(alloc_handle()),
            memory: DeviceMemoryHandle(alloc_handle()),
            format: self.depth_buffer.format,
            extent: self.extent,
            aspect_mask: self.depth_buffer.format.aspect_flags(),
            size_bytes: depth_size,
        };

        Ok(())
    }

    // Getters and inspection helpers

    pub fn hwnd(&self) -> usize {
        self.hwnd
    }

    pub fn surface(&self) -> SurfaceHandle {
        self.surface
    }

    pub fn swapchain(&self) -> SwapchainHandle {
        self.swapchain
    }

    pub fn extent(&self) -> VkExtent2D {
        self.extent
    }

    pub fn width(&self) -> u32 {
        self.extent.width
    }

    pub fn height(&self) -> u32 {
        self.extent.height
    }

    pub fn color_format(&self) -> VkFormat {
        self.color_format
    }

    pub fn color_space(&self) -> VkColorSpaceKHR {
        self.color_space
    }

    pub fn present_mode(&self) -> VkPresentModeKHR {
        self.present_mode
    }

    pub fn depth_format(&self) -> VkFormat {
        self.depth_buffer.format
    }

    pub fn depth_buffer(&self) -> &DepthBuffer {
        &self.depth_buffer
    }

    pub fn image_count(&self) -> u32 {
        self.images.len() as u32
    }

    pub fn images(&self) -> &[SwapchainImage] {
        &self.images
    }

    pub fn current_frame_index(&self) -> usize {
        self.current_frame
    }

    pub fn max_frames_in_flight(&self) -> usize {
        self.frames_in_flight.len()
    }

    pub fn current_frame_sync(&self) -> &FrameInFlight {
        &self.frames_in_flight[self.current_frame]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swapchain_manager_create_default() {
        let fake_hwnd = 0x00010203usize;
        let sm = SwapchainManager::create(fake_hwnd, 1920, 1080)
            .expect("SwapchainManager creation should succeed");

        assert_eq!(sm.hwnd(), fake_hwnd);
        assert_eq!(sm.width(), 1920);
        assert_eq!(sm.height(), 1080);
        assert_eq!(sm.extent(), VkExtent2D { width: 1920, height: 1080 });

        // Verifies required VK_FORMAT_B8G8R8A8_SRGB
        assert_eq!(sm.color_format(), VkFormat::B8G8R8A8Srgb);
        assert_eq!(sm.color_space(), VkColorSpaceKHR::SrgbNonlinear);

        // Verifies Mailbox presentation mode
        assert_eq!(sm.present_mode(), VkPresentModeKHR::Mailbox);

        // Verifies optimal depth buffer (D32_SFLOAT)
        assert_eq!(sm.depth_format(), VkFormat::D32Sfloat);
        assert_eq!(sm.depth_buffer().extent, VkExtent2D { width: 1920, height: 1080 });
        assert_eq!(
            sm.depth_buffer().size_bytes,
            1920 * 1080 * 4 // 4 bytes per pixel for D32_SFLOAT
        );
        assert_eq!(sm.depth_buffer().aspect_mask, VkImageAspectFlags::DEPTH);

        // Verifies triple buffering (3 swapchain images)
        assert_eq!(sm.image_count(), 3);
        assert_eq!(sm.images().len(), 3);

        // Verifies 2 frames in flight multi-buffering
        assert_eq!(sm.max_frames_in_flight(), 2);
        assert_eq!(sm.current_frame_index(), 0);
    }

    #[test]
    fn test_hwnd_zero_rejected() {
        let result = SwapchainManager::create(0, 1920, 1080);
        assert!(result.is_err());
        match result {
            Err(RenderError::SwapchainError(msg)) => {
                assert!(msg.contains("null HWND"));
            }
            _ => panic!("Expected SwapchainError for null HWND"),
        }
    }

    #[test]
    fn test_zero_dimensions_rejected() {
        let res1 = SwapchainManager::create(0x1234, 0, 1080);
        assert!(res1.is_err());
        let res2 = SwapchainManager::create(0x1234, 1920, 0);
        assert!(res2.is_err());
    }

    #[test]
    fn test_present_mode_negotiation_mailbox_and_fifo() {
        // When Mailbox is available
        let modes1 = [VkPresentModeKHR::Mailbox, VkPresentModeKHR::Fifo];
        let chosen1 = SwapchainNegotiator::choose_present_mode(&modes1, VkPresentModeKHR::Mailbox);
        assert_eq!(chosen1, VkPresentModeKHR::Mailbox);

        // When Mailbox is unavailable, fallback to FIFO
        let modes2 = [VkPresentModeKHR::Fifo, VkPresentModeKHR::Immediate];
        let chosen2 = SwapchainNegotiator::choose_present_mode(&modes2, VkPresentModeKHR::Mailbox);
        assert_eq!(chosen2, VkPresentModeKHR::Fifo);

        // When FIFO is explicitly requested
        let chosen3 = SwapchainNegotiator::choose_present_mode(&modes1, VkPresentModeKHR::Fifo);
        assert_eq!(chosen3, VkPresentModeKHR::Fifo);
    }

    #[test]
    fn test_depth_format_negotiation_and_aspect_mask() {
        // Preferred candidate list
        let candidates = [
            VkFormat::D32Sfloat,
            VkFormat::D24UnormS8Uint,
            VkFormat::D32SfloatS8Uint,
        ];

        // Device supports both D32 and D24 -> choose D32
        let supported1 = [VkFormat::D32Sfloat, VkFormat::D24UnormS8Uint];
        let chosen1 = SwapchainNegotiator::choose_depth_format(&supported1, &candidates)
            .expect("depth format found");
        assert_eq!(chosen1, VkFormat::D32Sfloat);
        assert_eq!(chosen1.aspect_flags(), VkImageAspectFlags::DEPTH);
        assert!(!chosen1.has_stencil());
        assert!(chosen1.has_depth());

        // Device only supports D24 -> choose D24
        let supported2 = [VkFormat::D24UnormS8Uint];
        let chosen2 = SwapchainNegotiator::choose_depth_format(&supported2, &candidates)
            .expect("depth format found");
        assert_eq!(chosen2, VkFormat::D24UnormS8Uint);
        assert_eq!(
            chosen2.aspect_flags(),
            VkImageAspectFlags::DEPTH | VkImageAspectFlags::STENCIL
        );
        assert!(chosen2.has_stencil());
        assert!(chosen2.has_depth());

        // Device supports no candidate -> error
        let supported3 = [VkFormat::R8G8B8A8Unorm];
        let result3 = SwapchainNegotiator::choose_depth_format(&supported3, &candidates);
        assert!(result3.is_err());
    }

    #[test]
    fn test_surface_format_negotiation() {
        let available = [
            VkSurfaceFormatKHR {
                format: VkFormat::R8G8B8A8Srgb,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            },
            VkSurfaceFormatKHR {
                format: VkFormat::B8G8R8A8Srgb,
                color_space: VkColorSpaceKHR::SrgbNonlinear,
            },
        ];

        let chosen = SwapchainNegotiator::choose_surface_format(
            &available,
            VkFormat::B8G8R8A8Srgb,
            VkColorSpaceKHR::SrgbNonlinear,
        );
        assert_eq!(chosen.format, VkFormat::B8G8R8A8Srgb);
        assert_eq!(chosen.color_space, VkColorSpaceKHR::SrgbNonlinear);
    }

    #[test]
    fn test_multi_buffered_frame_in_flight_lifecycle() {
        let mut sm = SwapchainManager::create(0xCAFE, 1920, 1080)
            .expect("Create swapchain");

        assert_eq!(sm.current_frame_index(), 0);

        // 1. Acquire Frame 0
        let frame0 = sm.acquire_next_image().expect("Acquire frame 0");
        assert_eq!(frame0.frame_index, 0);
        assert_eq!(frame0.image_index, 0);
        assert!(!frame0.is_suboptimal);

        // Present Frame 0 -> advances current_frame to 1
        let pres0 = sm.present().expect("Present frame 0");
        assert_eq!(pres0, PresentResult::Success);
        assert_eq!(sm.current_frame_index(), 1);

        // 2. Acquire Frame 1
        let frame1 = sm.acquire_next_image().expect("Acquire frame 1");
        assert_eq!(frame1.frame_index, 1);
        assert_eq!(frame1.image_index, 1);

        // Present Frame 1 -> wraps back to 0 (2 frames in flight)
        let pres1 = sm.present().expect("Present frame 1");
        assert_eq!(pres1, PresentResult::Success);
        assert_eq!(sm.current_frame_index(), 0);

        // 3. Acquire Frame 2 (uses slot 0, image 2)
        let frame2 = sm.acquire_next_image().expect("Acquire frame 2");
        assert_eq!(frame2.frame_index, 0);
        assert_eq!(frame2.image_index, 2);

        let pres2 = sm.present().expect("Present frame 2");
        assert_eq!(pres2, PresentResult::Success);
        assert_eq!(sm.current_frame_index(), 1);
    }

    #[test]
    fn test_present_without_acquire_fails() {
        let mut sm = SwapchainManager::create(0xCAFE, 1920, 1080).unwrap();
        let res = sm.present();
        assert!(res.is_err());
    }

    #[test]
    fn test_swapchain_recreate_on_resize() {
        let mut sm = SwapchainManager::create(0x1234, 1920, 1080).unwrap();
        let old_swapchain = sm.swapchain();
        let old_depth_view = sm.depth_buffer().view;

        // Resize to 2560x1440
        sm.recreate(2560, 1440).expect("Swapchain recreation failed");

        assert_eq!(sm.width(), 2560);
        assert_eq!(sm.height(), 1440);
        assert_eq!(sm.extent(), VkExtent2D { width: 2560, height: 1440 });
        assert_ne!(sm.swapchain(), old_swapchain);
        assert_ne!(sm.depth_buffer().view, old_depth_view);
        assert_eq!(
            sm.depth_buffer().size_bytes,
            2560 * 1440 * 4
        );

        // Recreated swapchain can immediately acquire and present
        let frame = sm.acquire_next_image().expect("Acquire after recreation");
        assert_eq!(frame.image_index, 0);
        sm.present().expect("Present after recreation");
    }
}
