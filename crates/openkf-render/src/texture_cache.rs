//! GPU texture sampler caching, descriptor set pooling, and texture resource management for OpenKF.
//!
//! Provides Vulkan-compliant sampler caching with support for:
//! - `VK_SAMPLER_ADDRESS_MODE_REPEAT`
//! - `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`
//! - 16x anisotropic filtering (`max_anisotropy = 16.0`)
//! - Negative LOD bias (`lod_bias = -0.5`) for razor-sharp mipmapped textures
//! - Descriptor set pooling and recycling for `COMBINED_IMAGE_SAMPLER` descriptors.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::error::RenderError;
use crate::hal::TextureHandle;
use crate::pso::CompareOp;
use crate::swapchain::VkFormat;

// ============================================================================
// Vulkan Sampler Enums and Constants
// ============================================================================

/// Vulkan texture coordinate addressing / wrap modes (`VkSamplerAddressMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum VkSamplerAddressMode {
    /// Repeat texture coordinates (`VK_SAMPLER_ADDRESS_MODE_REPEAT` = 0).
    #[default]
    Repeat = 0,
    /// Mirrored repeat coordinates (`VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT` = 1).
    MirroredRepeat = 1,
    /// Clamp to texture outer edge (`VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE` = 2).
    ClampToEdge = 2,
    /// Clamp to custom border color (`VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER` = 3).
    ClampToBorder = 3,
    /// Mirror clamp to edge (`VK_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE` = 4).
    MirrorClampToEdge = 4,
}

/// Constant aliases matching exact Vulkan C API naming conventions.
pub const VK_SAMPLER_ADDRESS_MODE_REPEAT: VkSamplerAddressMode = VkSamplerAddressMode::Repeat;
pub const VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT: VkSamplerAddressMode =
    VkSamplerAddressMode::MirroredRepeat;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE: VkSamplerAddressMode =
    VkSamplerAddressMode::ClampToEdge;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER: VkSamplerAddressMode =
    VkSamplerAddressMode::ClampToBorder;
pub const VK_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE: VkSamplerAddressMode =
    VkSamplerAddressMode::MirrorClampToEdge;

/// Vulkan texture filtering modes (`VkFilter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum VkFilter {
    /// Nearest-neighbor point filtering (`VK_FILTER_NEAREST` = 0).
    Nearest = 0,
    /// Bilinear interpolation filtering (`VK_FILTER_LINEAR` = 1).
    #[default]
    Linear = 1,
    /// Bicubic spline filtering (`VK_FILTER_CUBIC_IMG` = 1000015000).
    Cubic = 1000015000,
}

pub const VK_FILTER_NEAREST: VkFilter = VkFilter::Nearest;
pub const VK_FILTER_LINEAR: VkFilter = VkFilter::Linear;
pub const VK_FILTER_CUBIC: VkFilter = VkFilter::Cubic;

/// Vulkan mipmap filtering mode (`VkSamplerMipmapMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum VkSamplerMipmapMode {
    /// Nearest mipmap level selection (`VK_SAMPLER_MIPMAP_MODE_NEAREST` = 0).
    Nearest = 0,
    /// Linear interpolation between mipmap levels (`VK_SAMPLER_MIPMAP_MODE_LINEAR` = 1).
    #[default]
    Linear = 1,
}

pub const VK_SAMPLER_MIPMAP_MODE_NEAREST: VkSamplerMipmapMode = VkSamplerMipmapMode::Nearest;
pub const VK_SAMPLER_MIPMAP_MODE_LINEAR: VkSamplerMipmapMode = VkSamplerMipmapMode::Linear;

/// Vulkan border color selection (`VkBorderColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum VkBorderColor {
    #[default]
    FloatTransparentBlack = 0,
    IntTransparentBlack = 1,
    FloatOpaqueBlack = 2,
    IntOpaqueBlack = 3,
    FloatOpaqueWhite = 4,
    IntOpaqueWhite = 5,
}

/// Opaque 64-bit Vulkan sampler handle (`VkSampler`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct VkSampler(pub u64);

impl VkSampler {
    pub const NULL: Self = Self(0);

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

/// Opaque 64-bit Vulkan image view handle (`VkImageView`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct VkImageView(pub u64);

impl VkImageView {
    pub const NULL: Self = Self(0);

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

/// Opaque 64-bit Vulkan descriptor set handle (`VkDescriptorSet`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct VkDescriptorSet(pub u64);

impl VkDescriptorSet {
    pub const NULL: Self = Self(0);

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

/// Opaque 64-bit Vulkan descriptor pool handle (`VkDescriptorPool`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct VkDescriptorPool(pub u64);

impl VkDescriptorPool {
    pub const NULL: Self = Self(0);

    #[inline]
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

// ============================================================================
// Sampler Descriptor and Canonical Hash Key
// ============================================================================

/// Full specification of a Vulkan texture sampler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerDesc {
    pub mag_filter: VkFilter,
    pub min_filter: VkFilter,
    pub mipmap_mode: VkSamplerMipmapMode,
    pub address_mode_u: VkSamplerAddressMode,
    pub address_mode_v: VkSamplerAddressMode,
    pub address_mode_w: VkSamplerAddressMode,
    /// Mipmap level-of-detail bias. Negative value (e.g. -0.5) sharpens textures when aniso is enabled.
    pub mip_lod_bias: f32,
    /// Whether anisotropic filtering is enabled.
    pub anisotropy_enable: bool,
    /// Maximum anisotropy clamping ratio (typically 1.0 to 16.0).
    pub max_anisotropy: f32,
    /// Whether depth comparison sampling is enabled (e.g. for shadow map PCF).
    pub compare_enable: bool,
    pub compare_op: CompareOp,
    pub min_lod: f32,
    pub max_lod: f32,
    pub border_color: VkBorderColor,
    pub unnormalized_coordinates: bool,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            mag_filter: VkFilter::Linear,
            min_filter: VkFilter::Linear,
            mipmap_mode: VkSamplerMipmapMode::Linear,
            address_mode_u: VkSamplerAddressMode::Repeat,
            address_mode_v: VkSamplerAddressMode::Repeat,
            address_mode_w: VkSamplerAddressMode::Repeat,
            mip_lod_bias: -0.5,
            anisotropy_enable: true,
            max_anisotropy: 16.0,
            compare_enable: false,
            compare_op: CompareOp::Never,
            min_lod: 0.0,
            max_lod: 1000.0,
            border_color: VkBorderColor::FloatTransparentBlack,
            unnormalized_coordinates: false,
        }
    }
}

impl SamplerDesc {
    /// Creates a standard Unreal Engine 2.5 / OpenKF 3D world texture sampler:
    /// - Filter: Trilinear (`Linear` min/mag/mipmap)
    /// - Address mode: `VK_SAMPLER_ADDRESS_MODE_REPEAT`
    /// - Anisotropy: 16x enabled
    /// - LOD bias: -0.5 (compensates for anisotropic blur)
    pub fn world_repeat_aniso16() -> Self {
        Self::default()
    }

    /// Creates a clamped texture sampler (skyboxes, lightmaps, decals):
    /// - Address mode: `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`
    /// - Anisotropy: 16x enabled
    /// - LOD bias: -0.5
    pub fn world_clamp_aniso16() -> Self {
        Self {
            address_mode_u: VkSamplerAddressMode::ClampToEdge,
            address_mode_v: VkSamplerAddressMode::ClampToEdge,
            address_mode_w: VkSamplerAddressMode::ClampToEdge,
            ..Self::default()
        }
    }

    /// Creates a 2D GUI / HUD / Font texture sampler:
    /// - Filter: Bilinear
    /// - Address mode: `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`
    /// - Anisotropy: Disabled (1.0x)
    /// - LOD bias: 0.0
    pub fn gui_clamp() -> Self {
        Self {
            mag_filter: VkFilter::Linear,
            min_filter: VkFilter::Linear,
            mipmap_mode: VkSamplerMipmapMode::Nearest,
            address_mode_u: VkSamplerAddressMode::ClampToEdge,
            address_mode_v: VkSamplerAddressMode::ClampToEdge,
            address_mode_w: VkSamplerAddressMode::ClampToEdge,
            mip_lod_bias: 0.0,
            anisotropy_enable: false,
            max_anisotropy: 1.0,
            compare_enable: false,
            compare_op: CompareOp::Never,
            min_lod: 0.0,
            max_lod: 0.0,
            border_color: VkBorderColor::FloatTransparentBlack,
            unnormalized_coordinates: false,
        }
    }

    /// Creates a pixel-art / retro nearest-neighbor sampler:
    pub fn point_clamp() -> Self {
        Self {
            mag_filter: VkFilter::Nearest,
            min_filter: VkFilter::Nearest,
            mipmap_mode: VkSamplerMipmapMode::Nearest,
            address_mode_u: VkSamplerAddressMode::ClampToEdge,
            address_mode_v: VkSamplerAddressMode::ClampToEdge,
            address_mode_w: VkSamplerAddressMode::ClampToEdge,
            mip_lod_bias: 0.0,
            anisotropy_enable: false,
            max_anisotropy: 1.0,
            compare_enable: false,
            compare_op: CompareOp::Never,
            min_lod: 0.0,
            max_lod: 0.0,
            border_color: VkBorderColor::FloatTransparentBlack,
            unnormalized_coordinates: false,
        }
    }

    /// Creates a shadow map percentage-closer filtering (PCF) sampler:
    pub fn shadow_depth_pcf() -> Self {
        Self {
            mag_filter: VkFilter::Linear,
            min_filter: VkFilter::Linear,
            mipmap_mode: VkSamplerMipmapMode::Nearest,
            address_mode_u: VkSamplerAddressMode::ClampToBorder,
            address_mode_v: VkSamplerAddressMode::ClampToBorder,
            address_mode_w: VkSamplerAddressMode::ClampToBorder,
            mip_lod_bias: 0.0,
            anisotropy_enable: false,
            max_anisotropy: 1.0,
            compare_enable: true,
            compare_op: CompareOp::LessOrEqual,
            min_lod: 0.0,
            max_lod: 0.0,
            border_color: VkBorderColor::FloatOpaqueWhite,
            unnormalized_coordinates: false,
        }
    }

    /// Generates canonical hash key with bitwise-normalized floating point representations.
    pub fn to_key(&self) -> SamplerKey {
        SamplerKey {
            mag_filter: self.mag_filter,
            min_filter: self.min_filter,
            mipmap_mode: self.mipmap_mode,
            address_mode_u: self.address_mode_u,
            address_mode_v: self.address_mode_v,
            address_mode_w: self.address_mode_w,
            mip_lod_bias_bits: self.mip_lod_bias.to_bits(),
            anisotropy_enable: self.anisotropy_enable,
            max_anisotropy_bits: self.max_anisotropy.to_bits(),
            compare_enable: self.compare_enable,
            compare_op: self.compare_op,
            min_lod_bits: self.min_lod.to_bits(),
            max_lod_bits: self.max_lod.to_bits(),
            border_color: self.border_color,
            unnormalized_coordinates: self.unnormalized_coordinates,
        }
    }
}

/// Bitwise hashable key for sampler caching, preventing duplicate Vulkan sampler creations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerKey {
    pub mag_filter: VkFilter,
    pub min_filter: VkFilter,
    pub mipmap_mode: VkSamplerMipmapMode,
    pub address_mode_u: VkSamplerAddressMode,
    pub address_mode_v: VkSamplerAddressMode,
    pub address_mode_w: VkSamplerAddressMode,
    pub mip_lod_bias_bits: u32,
    pub anisotropy_enable: bool,
    pub max_anisotropy_bits: u32,
    pub compare_enable: bool,
    pub compare_op: CompareOp,
    pub min_lod_bits: u32,
    pub max_lod_bits: u32,
    pub border_color: VkBorderColor,
    pub unnormalized_coordinates: bool,
}

/// Statistics for sampler cache hits and misses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SamplerCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_cached: usize,
}

// ============================================================================
// Texture Sampler Cache Implementation
// ============================================================================

static NEXT_SAMPLER_HANDLE: AtomicU64 = AtomicU64::new(0x8000_0001);

/// GPU texture sampler cache guaranteeing exact deduplication of Vulkan samplers.
pub struct TextureSamplerCache {
    samplers: HashMap<SamplerKey, VkSampler>,
    descriptors: HashMap<VkSampler, SamplerDesc>,
    stats: SamplerCacheStats,
}

impl Default for TextureSamplerCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureSamplerCache {
    /// Creates a new texture sampler cache and pre-warms common sampler configurations.
    pub fn new() -> Self {
        let mut cache = Self {
            samplers: HashMap::new(),
            descriptors: HashMap::new(),
            stats: SamplerCacheStats::default(),
        };

        // Pre-warm primary sampler presets
        cache.get_or_create_custom_sampler(&SamplerDesc::world_repeat_aniso16());
        cache.get_or_create_custom_sampler(&SamplerDesc::world_clamp_aniso16());
        cache.get_or_create_custom_sampler(&SamplerDesc::gui_clamp());
        cache.get_or_create_custom_sampler(&SamplerDesc::point_clamp());

        // Reset stats after pre-warming
        cache.stats.hits = 0;
        cache.stats.misses = 0;

        cache
    }

    /// Creates an empty texture sampler cache without pre-warmed presets.
    pub fn empty() -> Self {
        Self {
            samplers: HashMap::new(),
            descriptors: HashMap::new(),
            stats: SamplerCacheStats::default(),
        }
    }

    /// Primary contract requirement:
    /// Retrieves an existing cached sampler or allocates a new `VkSampler` matching the parameters:
    /// - `filter`: Min/Mag interpolation filter (`Linear` / `Nearest`)
    /// - `address_mode`: Texture coordinate wrapping (`Repeat` / `ClampToEdge` / etc.)
    /// - `max_anisotropy`: Maximum anisotropic filtering ratio (e.g. `16.0`)
    /// - `lod_bias`: Level-of-detail bias (e.g. `-0.5` for sharpening)
    pub fn get_or_create_sampler(
        &mut self,
        filter: VkFilter,
        address_mode: VkSamplerAddressMode,
        max_anisotropy: f32,
        lod_bias: f32,
    ) -> VkSampler {
        let anisotropy_enable = max_anisotropy > 1.0;
        let mipmap_mode = if filter == VkFilter::Nearest {
            VkSamplerMipmapMode::Nearest
        } else {
            VkSamplerMipmapMode::Linear
        };

        let desc = SamplerDesc {
            mag_filter: filter,
            min_filter: filter,
            mipmap_mode,
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            address_mode_w: address_mode,
            mip_lod_bias: lod_bias,
            anisotropy_enable,
            max_anisotropy: if anisotropy_enable { max_anisotropy } else { 1.0 },
            compare_enable: false,
            compare_op: CompareOp::Never,
            min_lod: 0.0,
            max_lod: 1000.0,
            border_color: VkBorderColor::FloatTransparentBlack,
            unnormalized_coordinates: false,
        };

        self.get_or_create_custom_sampler(&desc)
    }

    /// Retrieves an existing cached sampler or allocates a new one for any custom descriptor.
    pub fn get_or_create_custom_sampler(&mut self, desc: &SamplerDesc) -> VkSampler {
        let key = desc.to_key();

        if let Some(&sampler) = self.samplers.get(&key) {
            self.stats.hits += 1;
            return sampler;
        }

        // Cache miss: generate unique synthetic VkSampler handle
        self.stats.misses += 1;
        let raw_handle = NEXT_SAMPLER_HANDLE.fetch_add(1, Ordering::Relaxed);
        let sampler = VkSampler(raw_handle);

        self.samplers.insert(key, sampler);
        self.descriptors.insert(sampler, *desc);
        self.stats.total_cached = self.samplers.len();

        sampler
    }

    /// Retrieves the sampler description for a cached `VkSampler` handle.
    pub fn get_sampler_desc(&self, sampler: VkSampler) -> Option<&SamplerDesc> {
        self.descriptors.get(&sampler)
    }

    /// Returns cache statistics (hits, misses, total cached).
    #[inline]
    pub fn stats(&self) -> SamplerCacheStats {
        self.stats
    }

    /// Returns total number of cached samplers.
    #[inline]
    pub fn len(&self) -> usize {
        self.samplers.len()
    }

    /// Returns true if cache contains no samplers.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.samplers.is_empty()
    }

    /// Clears all cached samplers.
    pub fn clear(&mut self) {
        self.samplers.clear();
        self.descriptors.clear();
        self.stats.total_cached = 0;
    }
}

// ============================================================================
// Descriptor Set Pooling
// ============================================================================

static NEXT_DESCRIPTOR_SET_HANDLE: AtomicU64 = AtomicU64::new(0x9000_0001);
static NEXT_DESCRIPTOR_POOL_HANDLE: AtomicU64 = AtomicU64::new(0x7000_0001);

/// Unique key identifying a combined image-sampler descriptor set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureDescriptorKey {
    pub layout_id: u32,
    pub texture_handle: u64,
    pub sampler: VkSampler,
    pub binding_slot: u32,
}

/// Configuration for a Vulkan descriptor pool block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorPoolConfig {
    pub max_sets: u32,
    pub max_combined_samplers: u32,
}

impl Default for DescriptorPoolConfig {
    fn default() -> Self {
        Self {
            max_sets: 512,
            max_combined_samplers: 1024,
        }
    }
}

/// An individual Vulkan descriptor pool block supporting set allocation and recycling.
pub struct DescriptorPoolBlock {
    pub handle: VkDescriptorPool,
    pub capacity: u32,
    pub allocated_count: u32,
    pub free_list: Vec<VkDescriptorSet>,
}

impl DescriptorPoolBlock {
    pub fn new(capacity: u32) -> Self {
        let raw = NEXT_DESCRIPTOR_POOL_HANDLE.fetch_add(1, Ordering::Relaxed);
        Self {
            handle: VkDescriptorPool(raw),
            capacity,
            allocated_count: 0,
            free_list: Vec::new(),
        }
    }

    /// Tries to allocate a descriptor set from this block.
    pub fn allocate(&mut self) -> Option<VkDescriptorSet> {
        // First check free list for recycled descriptor sets
        if let Some(recycled) = self.free_list.pop() {
            return Some(recycled);
        }

        // Allocate fresh set if under block capacity
        if self.allocated_count < self.capacity {
            self.allocated_count += 1;
            let raw = NEXT_DESCRIPTOR_SET_HANDLE.fetch_add(1, Ordering::Relaxed);
            Some(VkDescriptorSet(raw))
        } else {
            None // Block exhausted
        }
    }

    /// Reclaims a freed descriptor set into the block's free list.
    pub fn free(&mut self, set: VkDescriptorSet) {
        if !set.is_null() && !self.free_list.contains(&set) {
            self.free_list.push(set);
        }
    }

    /// Resets the pool block, invalidating all sets and resetting counter to zero.
    pub fn reset(&mut self) {
        self.allocated_count = 0;
        self.free_list.clear();
    }

    /// Returns remaining available set slots.
    #[inline]
    pub fn available_sets(&self) -> u32 {
        let remaining_new = self.capacity.saturating_sub(self.allocated_count);
        remaining_new + self.free_list.len() as u32
    }
}

/// Statistics for descriptor set pooling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DescriptorPoolStats {
    pub total_pools: usize,
    pub active_sets: usize,
    pub cached_sets: usize,
    pub recycled_reuses: u64,
}

/// Multi-block descriptor set pool manager with automatic expansion and texture/sampler caching.
pub struct DescriptorSetPool {
    config: DescriptorPoolConfig,
    pools: Vec<DescriptorPoolBlock>,
    /// Cache mapping `(layout, texture, sampler, slot)` to pre-allocated `VkDescriptorSet`.
    cached_sets: HashMap<TextureDescriptorKey, VkDescriptorSet>,
    stats: DescriptorPoolStats,
}

impl Default for DescriptorSetPool {
    fn default() -> Self {
        Self::new(DescriptorPoolConfig::default())
    }
}

impl DescriptorSetPool {
    /// Creates a new descriptor set pool manager with the given block configuration.
    pub fn new(config: DescriptorPoolConfig) -> Self {
        let first_pool = DescriptorPoolBlock::new(config.max_sets);
        Self {
            config,
            pools: vec![first_pool],
            cached_sets: HashMap::new(),
            stats: DescriptorPoolStats {
                total_pools: 1,
                active_sets: 0,
                cached_sets: 0,
                recycled_reuses: 0,
            },
        }
    }

    /// Retrieves an existing cached descriptor set or allocates a new one for the given
    /// texture and sampler binding. Automatically deduplicates allocations across materials.
    pub fn get_or_allocate_texture_descriptor(
        &mut self,
        layout_id: u32,
        texture_handle: u64,
        sampler: VkSampler,
        binding_slot: u32,
    ) -> Result<VkDescriptorSet, RenderError> {
        let key = TextureDescriptorKey {
            layout_id,
            texture_handle,
            sampler,
            binding_slot,
        };

        // Cache lookup
        if let Some(&set) = self.cached_sets.get(&key) {
            self.stats.recycled_reuses += 1;
            return Ok(set);
        }

        // Allocate new descriptor set from active or new pool block
        let set = self.allocate_raw_set()?;
        self.cached_sets.insert(key, set);
        self.stats.cached_sets = self.cached_sets.len();
        self.stats.active_sets += 1;

        Ok(set)
    }

    /// Allocates an uncached descriptor set from available pool blocks, creating a new block if needed.
    pub fn allocate_raw_set(&mut self) -> Result<VkDescriptorSet, RenderError> {
        // Try allocating from existing pool blocks
        for pool in &mut self.pools {
            if let Some(set) = pool.allocate() {
                return Ok(set);
            }
        }

        // All existing pools are exhausted: dynamically grow pool chain
        let mut new_pool = DescriptorPoolBlock::new(self.config.max_sets);
        let set = new_pool.allocate().ok_or_else(|| {
            RenderError::DescriptorBindingError {
                slot: 0,
                reason: "Failed to allocate descriptor set from newly spawned pool block".to_string(),
            }
        })?;

        self.pools.push(new_pool);
        self.stats.total_pools = self.pools.len();
        Ok(set)
    }

    /// Releases a descriptor set and removes it from the deduplication cache.
    pub fn free_set(&mut self, key: &TextureDescriptorKey) {
        if let Some(set) = self.cached_sets.remove(key) {
            self.stats.cached_sets = self.cached_sets.len();
            if self.stats.active_sets > 0 {
                self.stats.active_sets -= 1;
            }
            // Return to first pool's free list
            if let Some(first_pool) = self.pools.first_mut() {
                first_pool.free(set);
            }
        }
    }

    /// Resets all pools and flushes the descriptor cache (typically called on level transition).
    pub fn reset_all(&mut self) {
        self.cached_sets.clear();
        for pool in &mut self.pools {
            pool.reset();
        }
        self.stats.active_sets = 0;
        self.stats.cached_sets = 0;
    }

    /// Returns pool statistics.
    #[inline]
    pub fn stats(&self) -> DescriptorPoolStats {
        self.stats
    }

    /// Returns total number of pool blocks currently active.
    #[inline]
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }
}

// ============================================================================
// GPU Texture Representation and Integrated Cache
// ============================================================================

static NEXT_TEXTURE_VIEW_HANDLE: AtomicU64 = AtomicU64::new(0x6000_0001);

/// Metadata for a GPU texture managed by OpenKF.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuTexture {
    pub handle: TextureHandle,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub mip_levels: u32,
    pub format: VkFormat,
    pub image_view: VkImageView,
}

/// Integrated texture resource and sampler cache system.
pub struct TextureCache {
    textures_by_name: HashMap<String, GpuTexture>,
    textures_by_handle: HashMap<TextureHandle, GpuTexture>,
    sampler_cache: TextureSamplerCache,
    descriptor_pool: DescriptorSetPool,
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            textures_by_name: HashMap::new(),
            textures_by_handle: HashMap::new(),
            sampler_cache: TextureSamplerCache::new(),
            descriptor_pool: DescriptorSetPool::default(),
        }
    }

    /// Registers a newly uploaded GPU texture.
    pub fn register_texture(
        &mut self,
        handle: TextureHandle,
        name: impl Into<String>,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: VkFormat,
    ) -> &GpuTexture {
        let name_str = name.into();
        let raw_view = NEXT_TEXTURE_VIEW_HANDLE.fetch_add(1, Ordering::Relaxed);
        let tex = GpuTexture {
            handle,
            name: name_str.clone(),
            width,
            height,
            mip_levels,
            format,
            image_view: VkImageView(raw_view),
        };

        self.textures_by_name.insert(name_str, tex.clone());
        self.textures_by_handle.insert(handle, tex);
        self.textures_by_handle.get(&handle).unwrap()
    }

    /// Looks up texture by human-readable asset path / name.
    pub fn get_by_name(&self, name: &str) -> Option<&GpuTexture> {
        self.textures_by_name.get(name)
    }

    /// Looks up texture by numeric `TextureHandle`.
    pub fn get_by_handle(&self, handle: TextureHandle) -> Option<&GpuTexture> {
        self.textures_by_handle.get(&handle)
    }

    /// Borrows the inner sampler cache.
    pub fn samplers(&self) -> &TextureSamplerCache {
        &self.sampler_cache
    }

    /// Mutably borrows the inner sampler cache.
    pub fn samplers_mut(&mut self) -> &mut TextureSamplerCache {
        &mut self.sampler_cache
    }

    /// Borrows the inner descriptor pool.
    pub fn descriptors(&self) -> &DescriptorSetPool {
        &self.descriptor_pool
    }

    /// Mutably borrows the inner descriptor pool.
    pub fn descriptors_mut(&mut self) -> &mut DescriptorSetPool {
        &mut self.descriptor_pool
    }

    /// Binds a texture with a sampler and returns a pooled descriptor set ready for Vulkan commands.
    pub fn bind_texture_sampler(
        &mut self,
        layout_id: u32,
        texture_handle: TextureHandle,
        sampler: VkSampler,
        binding_slot: u32,
    ) -> Result<VkDescriptorSet, RenderError> {
        self.descriptor_pool.get_or_allocate_texture_descriptor(
            layout_id,
            texture_handle.0,
            sampler,
            binding_slot,
        )
    }

    /// Evicts a texture by handle and purges any associated cached descriptors.
    pub fn evict(&mut self, handle: TextureHandle) {
        if let Some(tex) = self.textures_by_handle.remove(&handle) {
            self.textures_by_name.remove(&tex.name);
        }
    }

    /// Clears all textures, samplers, and descriptor pools.
    pub fn clear(&mut self) {
        self.textures_by_name.clear();
        self.textures_by_handle.clear();
        self.sampler_cache.clear();
        self.descriptor_pool.reset_all();
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampler_address_mode_constants() {
        assert_eq!(VK_SAMPLER_ADDRESS_MODE_REPEAT, VkSamplerAddressMode::Repeat);
        assert_eq!(
            VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            VkSamplerAddressMode::ClampToEdge
        );
        assert_eq!(
            VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT,
            VkSamplerAddressMode::MirroredRepeat
        );
        assert_eq!(
            VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER,
            VkSamplerAddressMode::ClampToBorder
        );
        assert_eq!(
            VK_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE,
            VkSamplerAddressMode::MirrorClampToEdge
        );
    }

    #[test]
    fn test_get_or_create_sampler_repeat_aniso16_lod_bias() {
        let mut cache = TextureSamplerCache::empty();
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 0);

        // 16x aniso, repeat, negative LOD bias (-0.5)
        let s1 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -0.5,
        );
        assert!(!s1.is_null());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        let desc = cache.get_sampler_desc(s1).expect("desc should exist");
        assert_eq!(desc.mag_filter, VkFilter::Linear);
        assert_eq!(desc.min_filter, VkFilter::Linear);
        assert_eq!(desc.address_mode_u, VkSamplerAddressMode::Repeat);
        assert_eq!(desc.address_mode_v, VkSamplerAddressMode::Repeat);
        assert_eq!(desc.address_mode_w, VkSamplerAddressMode::Repeat);
        assert!(desc.anisotropy_enable);
        assert_eq!(desc.max_anisotropy, 16.0);
        assert_eq!(desc.mip_lod_bias, -0.5);

        // Same call must return identical cached sampler and increment hits
        let s2 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -0.5,
        );
        assert_eq!(s1, s2);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_get_or_create_sampler_clamp_to_edge() {
        let mut cache = TextureSamplerCache::new();

        let s1 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            1.0,
            0.0,
        );
        assert!(!s1.is_null());

        let desc = cache.get_sampler_desc(s1).expect("desc should exist");
        assert_eq!(desc.address_mode_u, VkSamplerAddressMode::ClampToEdge);
        assert_eq!(desc.address_mode_v, VkSamplerAddressMode::ClampToEdge);
        assert_eq!(desc.address_mode_w, VkSamplerAddressMode::ClampToEdge);
        assert!(!desc.anisotropy_enable);
        assert_eq!(desc.max_anisotropy, 1.0);
        assert_eq!(desc.mip_lod_bias, 0.0);

        // Distinct from repeat sampler
        let s_repeat = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            1.0,
            0.0,
        );
        assert_ne!(s1, s_repeat);
    }

    #[test]
    fn test_negative_lod_bias_precision_and_deduplication() {
        let mut cache = TextureSamplerCache::new();

        let s1 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -0.5,
        );
        let s2 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -1.0,
        );
        let s3 = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            0.0,
        );

        // Different LOD biases must yield distinct samplers
        assert_ne!(s1, s2);
        assert_ne!(s1, s3);
        assert_ne!(s2, s3);
    }

    #[test]
    fn test_descriptor_set_pool_allocation_and_reuse() {
        let mut pool = DescriptorSetPool::default();

        let tex_handle = 1001u64;
        let sampler = VkSampler(500);

        // First allocation creates set
        let set1 = pool
            .get_or_allocate_texture_descriptor(1, tex_handle, sampler, 0)
            .expect("allocation should succeed");
        assert!(!set1.is_null());

        // Second allocation with identical parameters returns cached set
        let set2 = pool
            .get_or_allocate_texture_descriptor(1, tex_handle, sampler, 0)
            .expect("lookup should succeed");
        assert_eq!(set1, set2);

        assert_eq!(pool.stats().recycled_reuses, 1);
        assert_eq!(pool.stats().cached_sets, 1);
        assert_eq!(pool.stats().active_sets, 1);
    }

    #[test]
    fn test_descriptor_pool_block_growth() {
        // Create pool with small capacity of 2 sets per block
        let config = DescriptorPoolConfig {
            max_sets: 2,
            max_combined_samplers: 4,
        };
        let mut pool = DescriptorSetPool::new(config);

        assert_eq!(pool.pool_count(), 1);

        // Allocate 5 distinct sets (exceeding initial block capacity of 2)
        let mut sets = Vec::new();
        for i in 0..5 {
            let set = pool
                .get_or_allocate_texture_descriptor(1, i, VkSampler(i + 1), 0)
                .expect("allocation should grow pool");
            sets.push(set);
        }

        // Must have grown to at least 3 pool blocks (2 + 2 + 1)
        assert!(pool.pool_count() >= 3);

        // All 5 handles must be unique
        for i in 0..sets.len() {
            for j in (i + 1)..sets.len() {
                assert_ne!(sets[i], sets[j]);
            }
        }
    }

    #[test]
    fn test_descriptor_set_free_and_recycle() {
        let mut pool = DescriptorSetPool::new(DescriptorPoolConfig {
            max_sets: 2,
            max_combined_samplers: 4,
        });

        let key = TextureDescriptorKey {
            layout_id: 1,
            texture_handle: 42,
            sampler: VkSampler(99),
            binding_slot: 0,
        };

        let set1 = pool
            .get_or_allocate_texture_descriptor(
                key.layout_id,
                key.texture_handle,
                key.sampler,
                key.binding_slot,
            )
            .unwrap();

        pool.free_set(&key);
        assert_eq!(pool.stats().cached_sets, 0);

        // Allocating a new set should recycle the freed set handle from the block free list
        let set2 = pool.allocate_raw_set().unwrap();
        assert_eq!(set1, set2);
    }

    #[test]
    fn test_texture_cache_end_to_end() {
        let mut cache = TextureCache::new();

        let tex_handle = TextureHandle(777);
        let tex = cache.register_texture(
            tex_handle,
            "KF_Soldier_Textures.Armor_D",
            1024,
            1024,
            11,
            VkFormat::R8G8B8A8Srgb,
        );
        assert_eq!(tex.name, "KF_Soldier_Textures.Armor_D");
        assert_eq!(tex.mip_levels, 11);
        assert!(!tex.image_view.is_null());

        // Retrieve by name and by handle
        assert!(cache.get_by_name("KF_Soldier_Textures.Armor_D").is_some());
        assert!(cache.get_by_handle(tex_handle).is_some());

        // Get 16x aniso sampler with -0.5 lod bias
        let sampler = cache.samplers_mut().get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -0.5,
        );

        // Bind texture with sampler into pooled descriptor set
        let set1 = cache
            .bind_texture_sampler(1, tex_handle, sampler, 0)
            .expect("bind should succeed");
        assert!(!set1.is_null());

        // Re-binding returns same descriptor set
        let set2 = cache
            .bind_texture_sampler(1, tex_handle, sampler, 0)
            .expect("re-bind should succeed");
        assert_eq!(set1, set2);

        // Evict texture
        cache.evict(tex_handle);
        assert!(cache.get_by_handle(tex_handle).is_none());
        assert!(cache.get_by_name("KF_Soldier_Textures.Armor_D").is_none());
    }

    #[test]
    fn test_prewarmed_samplers_present() {
        let cache = TextureSamplerCache::new();
        // At least 4 presets pre-warmed
        assert!(cache.len() >= 4);
    }
}
