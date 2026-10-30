pub mod error;
pub mod vertex;
pub mod pso;
pub mod material;
pub mod hal;
pub mod scene;
pub mod device;
pub mod camera;
pub mod gui;
pub mod swapchain;
pub mod hud;
pub mod viewport;
pub mod fonts;
pub mod shaders;
pub mod canvas;
pub mod server_browser;
pub mod texture_cache;
pub mod news_dialog;
pub mod main_menu;
pub mod culling;
pub mod trader_ui;
pub mod perk_select_ui;

// Re-exports for convenient top-level usage
pub use error::RenderError;
pub use swapchain::{
    AcquiredFrame, DepthBuffer, FrameInFlight, PresentResult, SwapchainConfig,
    SwapchainManager, SwapchainNegotiator, VkColorSpaceKHR, VkExtent2D, VkFormat,
    VkImageAspectFlags, VkImageUsageFlags, VkPresentModeKHR, VkSurfaceCapabilitiesKHR,
    VkSurfaceFormatKHR,
};
pub use vertex::{
    MeshInstanceData, SkeletalMeshVertex, SkinnedOutputVertex, StaticMeshVertex,
    UIVertex, VertexAttribute, VertexFormat, VertexInputRate, VertexLayout,
};
pub use pso::{
    BlendFactor, BlendOp, ColorWriteMask, CompareOp, CompiledPipeline, CullMode,
    FrontFace, PipelineKey, PolygonMode, PrimitiveTopology, PsoCache, PsoCacheStats,
};
pub use material::{
    material_flags, BlendMode, DescriptorBinding, DescriptorSlot, MaterialConstants,
    MaterialDefinition, MaterialTranslator, TranslatedMaterial, UvModifier,
};
pub use hal::{
    BufferHandle, ClearValues, IndexFormat, PipelineHandle, PrimitiveType,
    RenderCommand, RenderDevice, RenderInterface, ScissorRect, TextureHandle,
    TransformType, Viewport,
};
pub use scene::{SceneUniforms, StaticMeshSectionDraw};
pub use device::{ModernRenderDevice, ModernRenderInterface};
pub use gui::{
    AnchorPoint, ButtonStyle, Canvas, ComponentState, DrawCommand, GUIButton,
    GUIComponent, GUIController, GUILabel, GUIListBox, GUIPage, GuiButton,
    GuiComponent, GuiController, GuiLabel, GuiListBox, GuiPage, LayoutBox,
    LayoutMode, MouseButton, MouseEvent, Rect, TextAlignment,
};
pub use hud::{
    FireMode, HitmarkerState, HudCanvas, HudConfig, HudDrawBatch, InGameHud,
    PerkType, PlayerState, WaveState, WeaponState,
};
pub use fonts::{
    CanvasTextSettings, FontAtlas, FontEngine, GlyphMetrics, KerningTable, TextBatch,
    TextQuad, UE2FontCharacter,
};
pub use viewport::{
    Camera, FrameStats, Scene, SceneMeshItem, Surface, ViewportLoop,
};
pub use canvas::{Canvas2D, CanvasDrawBatch, CanvasFlushResult};
pub use shaders::{
    apply_fog, calculate_fog_factor, evaluate_lightmap_modulation,
    evaluate_vertex_lighting_modulation, DescriptorSetLayoutBindingDesc, DescriptorType,
    FogMode, FogParameters, LightingModel, PipelineLayoutDesc, PushConstantRangeDesc,
    ShaderBytecode, ShaderPipelineConfig, ShaderStage, ShaderStageFlags, SPIRV_MAGIC,
};
pub use server_browser::{
    BrowserColumn, BrowserFilters, BrowserTab, ConnectError, ConnectTarget,
    DirectConnectDialog, ServerBrowserPage, ServerDifficulty, ServerEntry, SortDirection,
};
pub use texture_cache::{
    DescriptorPoolBlock, DescriptorPoolConfig, DescriptorPoolStats, DescriptorSetPool,
    GpuTexture, SamplerCacheStats, SamplerDesc, SamplerKey, TextureCache,
    TextureDescriptorKey, TextureSamplerCache, VkBorderColor, VkDescriptorPool,
    VkDescriptorSet, VkFilter, VkImageView, VkSampler, VkSamplerAddressMode,
    VkSamplerMipmapMode, VK_FILTER_CUBIC, VK_FILTER_LINEAR, VK_FILTER_NEAREST,
    VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER, VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
    VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT, VK_SAMPLER_ADDRESS_MODE_MIRROR_CLAMP_TO_EDGE,
    VK_SAMPLER_ADDRESS_MODE_REPEAT, VK_SAMPLER_MIPMAP_MODE_LINEAR,
    VK_SAMPLER_MIPMAP_MODE_NEAREST,
};
pub use news_dialog::{
    ComparisonRow, FeatureCategory, NewsDialogPage, NewsDocument, NewsHeader, NewsSection,
};
pub use main_menu::{
    MainMenu3DBackground, MainMenuAction, MainMenuButton, MainMenuPage, MainMenuState,
    MenuButtonId, MenuSettings, MenuSoundCue, PerkProgressInfo, SoloMatchConfig,
};
pub use trader_ui::{
    InventoryWeapon, TraderError, TraderShopPage, TraderTab, TraderWeaponItem,
};
pub use perk_select_ui::{
    format_number, get_perk_bonuses, get_perk_weapons, perk_requirements,
    perk_role_summary, perk_tag, perk_theme_color, perk_tint_color, perk_title,
    PerkDetailTab, PerkProgression, PerkSelectionPage, PerkStatBonus, PerkUiButton,
    PerkWeapon, ALL_PERKS, MAX_PERK_LEVEL,
};

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    #[test]
    fn test_static_mesh_vertex_layout_36_bytes() {
        // Must be exactly 36 bytes for UE2.5 retail static mesh stream
        assert_eq!(std::mem::size_of::<StaticMeshVertex>(), 36);
        assert_eq!(std::mem::align_of::<StaticMeshVertex>(), 4);

        let v = StaticMeshVertex::new(
            [10.0, 20.0, 30.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.5],
            [16384, 49152],
        );

        // Verify field offsets in bytes
        let base_ptr = &v as *const _ as usize;
        let pos_ptr = &v.position as *const _ as usize;
        let norm_ptr = &v.normal as *const _ as usize;
        let uv0_ptr = &v.uv0 as *const _ as usize;
        let uv1_ptr = &v.uv1 as *const _ as usize;

        assert_eq!(pos_ptr - base_ptr, 0);
        assert_eq!(norm_ptr - base_ptr, 12);
        assert_eq!(uv0_ptr - base_ptr, 24);
        assert_eq!(uv1_ptr - base_ptr, 32);

        // Test Pod byte roundtrip
        let bytes = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 36);
        let roundtrip: &StaticMeshVertex = bytemuck::from_bytes(bytes);
        assert_eq!(*roundtrip, v);

        // Verify vertex layout attribute count and total stride
        let layout = StaticMeshVertex::vertex_layout();
        assert_eq!(layout.stride, 36);
        assert_eq!(layout.attributes.len(), 4);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[1].offset, 12);
        assert_eq!(layout.attributes[2].offset, 24);
        assert_eq!(layout.attributes[3].offset, 32);
        assert_eq!(layout.attributes[3].format, VertexFormat::Half2);

        // Test from_f32 conversion helper
        let v_f32 = StaticMeshVertex::from_f32(
            [10.0, 20.0, 30.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.5],
            [0.25, 0.75],
        );
        assert_eq!(v_f32.position, [10.0, 20.0, 30.0]);
        let lm = v_f32.lightmap_uv_f32();
        assert!((lm[0] - 0.25).abs() < 0.001);
        assert!((lm[1] - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_skeletal_mesh_vertex_layout() {
        assert_eq!(std::mem::size_of::<SkeletalMeshVertex>(), 56);
        assert_eq!(std::mem::align_of::<SkeletalMeshVertex>(), 4);

        let v = SkeletalMeshVertex::new(
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            [0.1, 0.9],
            [5, 12, 18, 0],
            [0.6, 0.3, 0.1, 0.0],
        );

        let base_ptr = &v as *const _ as usize;
        let pos_ptr = &v.position as *const _ as usize;
        let norm_ptr = &v.normal as *const _ as usize;
        let uv_ptr = &v.uv as *const _ as usize;
        let bones_ptr = &v.bone_indices as *const _ as usize;
        let weights_ptr = &v.bone_weights as *const _ as usize;

        assert_eq!(pos_ptr - base_ptr, 0);
        assert_eq!(norm_ptr - base_ptr, 12);
        assert_eq!(uv_ptr - base_ptr, 24);
        assert_eq!(bones_ptr - base_ptr, 32);
        assert_eq!(weights_ptr - base_ptr, 40);

        let bytes = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 56);
        let roundtrip: &SkeletalMeshVertex = bytemuck::from_bytes(bytes);
        assert_eq!(*roundtrip, v);

        // Skinned output vertex layout
        assert_eq!(std::mem::size_of::<SkinnedOutputVertex>(), 32);
    }

    #[test]
    fn test_ui_canvas_quad_vertex_layout() {
        assert_eq!(std::mem::size_of::<UIVertex>(), 32);
        assert_eq!(std::mem::align_of::<UIVertex>(), 4);

        let v = UIVertex::new([100.0, 200.0], [0.0, 1.0], [1.0, 0.0, 0.0, 1.0]);

        let base_ptr = &v as *const _ as usize;
        let pos_ptr = &v.position as *const _ as usize;
        let uv_ptr = &v.uv as *const _ as usize;
        let col_ptr = &v.color as *const _ as usize;

        assert_eq!(pos_ptr - base_ptr, 0);
        assert_eq!(uv_ptr - base_ptr, 8);
        assert_eq!(col_ptr - base_ptr, 16);

        let bytes = bytemuck::bytes_of(&v);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_mesh_instance_data_layout() {
        assert_eq!(std::mem::size_of::<MeshInstanceData>(), 80);
        assert_eq!(std::mem::align_of::<MeshInstanceData>(), 4);

        let identity = Mat4::IDENTITY.to_cols_array();
        let inst = MeshInstanceData::new(identity, [0.1, 0.2, 0.5, 0.5]);

        let base_ptr = &inst as *const _ as usize;
        let mat_ptr = &inst.world_matrix as *const _ as usize;
        let lm_ptr = &inst.lightmap_offset_scale as *const _ as usize;

        assert_eq!(mat_ptr - base_ptr, 0);
        assert_eq!(lm_ptr - base_ptr, 64);
    }

    #[test]
    fn test_material_constants_uniform_alignment() {
        // MaterialConstants must be packed and 64 bytes for GPU push constants / uniform buffers
        assert_eq!(std::mem::size_of::<MaterialConstants>(), 64);
        assert_eq!(std::mem::align_of::<MaterialConstants>(), 4);
    }

    #[test]
    fn test_material_translator_opaque() {
        let mat = MaterialDefinition {
            name: "KF_WallConcrete".into(),
            blend_mode: BlendMode::Opaque,
            diffuse_map: Some("KF_Textures.WallDiff".into()),
            normal_map: Some("KF_Textures.WallNorm".into()),
            specular_map: Some("KF_Textures.WallSpec".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        assert_eq!(translated.blend_mode, BlendMode::Opaque);
        assert!(translated.depth_write);
        assert!(translated.depth_test);
        assert_eq!(translated.constants.alpha_cutoff, 0.0);
        assert_eq!(
            translated.constants.flags & material_flags::HAS_DIFFUSE,
            material_flags::HAS_DIFFUSE
        );
        assert_eq!(
            translated.constants.flags & material_flags::HAS_NORMAL,
            material_flags::HAS_NORMAL
        );
        assert_eq!(
            translated.constants.flags & material_flags::HAS_SPECULAR,
            material_flags::HAS_SPECULAR
        );
        assert_eq!(translated.descriptor_bindings.len(), 3);

        let key = MaterialTranslator::build_pipeline_key(&translated, 100, 1, 2);
        assert!(!key.blend_enable);
        assert!(key.depth_write_enable);
    }

    #[test]
    fn test_material_translator_alpha_test() {
        let mat = MaterialDefinition {
            name: "KF_ChainFence".into(),
            blend_mode: BlendMode::AlphaTest,
            alpha_ref: 0.6,
            two_sided: true,
            diffuse_map: Some("KF_Textures.FenceDiff".into()),
            opacity_map: Some("KF_Textures.FenceAlpha".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        assert_eq!(translated.blend_mode, BlendMode::AlphaTest);
        assert!(translated.depth_write);
        assert_eq!(translated.constants.alpha_cutoff, 0.6);
        assert_eq!(
            translated.constants.flags & material_flags::ALPHA_TEST,
            material_flags::ALPHA_TEST
        );
        assert_eq!(
            translated.constants.flags & material_flags::TWO_SIDED,
            material_flags::TWO_SIDED
        );
        assert_eq!(translated.cull_mode, CullMode::None);

        let key = MaterialTranslator::build_pipeline_key(&translated, 100, 1, 2);
        assert_eq!(key.cull_mode, CullMode::None);
        assert!(!key.blend_enable);
        assert!(key.depth_write_enable);
    }

    #[test]
    fn test_material_translator_translucent() {
        let mat = MaterialDefinition {
            name: "KF_BloodWindow".into(),
            blend_mode: BlendMode::Translucent,
            diffuse_map: Some("KF_Textures.Glass".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        assert_eq!(translated.blend_mode, BlendMode::Translucent);
        assert!(!translated.depth_write);

        let key = MaterialTranslator::build_pipeline_key(&translated, 100, 1, 2);
        assert!(key.blend_enable);
        assert!(!key.depth_write_enable);
        assert_eq!(key.src_color_blend, BlendFactor::SrcAlpha);
        assert_eq!(key.dst_color_blend, BlendFactor::OneMinusSrcAlpha);
    }

    #[test]
    fn test_material_translator_additive() {
        let mat = MaterialDefinition {
            name: "KF_MuzzleFire".into(),
            blend_mode: BlendMode::Additive,
            diffuse_map: Some("KF_Textures.Fire".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        let key = MaterialTranslator::build_pipeline_key(&translated, 100, 1, 2);
        assert!(key.blend_enable);
        assert!(!key.depth_write_enable);
        assert_eq!(key.src_color_blend, BlendFactor::One);
        assert_eq!(key.dst_color_blend, BlendFactor::One);
    }

    #[test]
    fn test_material_translator_modulated() {
        let mat = MaterialDefinition {
            name: "KF_DecalShadow".into(),
            blend_mode: BlendMode::Modulated,
            diffuse_map: Some("KF_Textures.Shadow".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        let key = MaterialTranslator::build_pipeline_key(&translated, 100, 1, 2);
        assert!(key.blend_enable);
        assert!(!key.depth_write_enable);
        assert_eq!(key.src_color_blend, BlendFactor::DstColor);
        assert_eq!(key.dst_color_blend, BlendFactor::Zero);
    }

    #[test]
    fn test_shader_descriptor_bindings_mapping() {
        let mat = MaterialDefinition {
            name: "FullSpecMaterial".into(),
            blend_mode: BlendMode::Opaque,
            diffuse_map: Some("diffuse.dds".into()),
            normal_map: Some("normal.dds".into()),
            specular_map: Some("specular.dds".into()),
            opacity_map: Some("opacity.dds".into()),
            detail_map: Some("detail.dds".into()),
            ..Default::default()
        };

        let translated = MaterialTranslator::translate(&mat, 0.0);
        assert_eq!(translated.descriptor_bindings.len(), 5);

        let slots: Vec<u32> = translated.descriptor_bindings.iter().map(|b| b.slot).collect();
        assert_eq!(slots, vec![0, 1, 2, 3, 4]);

        assert_eq!(translated.descriptor_bindings[0].resource_name, "diffuse.dds");
        assert_eq!(translated.descriptor_bindings[1].resource_name, "normal.dds");
        assert_eq!(translated.descriptor_bindings[2].resource_name, "specular.dds");
        assert_eq!(translated.descriptor_bindings[3].resource_name, "opacity.dds");
        assert_eq!(translated.descriptor_bindings[4].resource_name, "detail.dds");
    }

    #[test]
    fn test_pso_cache_hit_and_serialization() {
        let mut cache = PsoCache::new();
        let key1 = PipelineKey {
            vertex_shader_id: 101,
            fragment_shader_id: 202,
            vertex_layout_hash: 303,
            ..Default::default()
        };

        let key2 = PipelineKey {
            vertex_shader_id: 404,
            fragment_shader_id: 505,
            vertex_layout_hash: 606,
            cull_mode: CullMode::None,
            ..Default::default()
        };

        // First access: miss + compile
        let p1 = cache.get_or_create(&key1, "StaticMeshOpaque", |_| Ok(())).unwrap().clone();
        assert_eq!(p1.id, 1);
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 1);

        // Second access: hit
        let p1_again = cache.get_or_create(&key1, "StaticMeshOpaque", |_| Ok(())).unwrap().clone();
        assert_eq!(p1_again.id, 1);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);

        // Third access: miss + compile key2
        let p2 = cache.get_or_create(&key2, "TwoSidedWireframe", |_| Ok(())).unwrap().clone();
        assert_eq!(p2.id, 2);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.len(), 2);

        // Test serialization and deserialization
        let blob = cache.serialize_cache();
        assert!(blob.len() > 8);

        let mut fresh_cache = PsoCache::new();
        let loaded_count = fresh_cache.deserialize_cache(&blob).unwrap();
        assert_eq!(loaded_count, 2);
        assert_eq!(fresh_cache.len(), 2);

        let retrieved1 = fresh_cache.get(&key1).expect("Key1 should exist in loaded cache");
        assert_eq!(retrieved1.id, 1);
        assert_eq!(retrieved1.debug_name, "StaticMeshOpaque");

        let retrieved2 = fresh_cache.get(&key2).expect("Key2 should exist in loaded cache");
        assert_eq!(retrieved2.id, 2);
        assert_eq!(retrieved2.debug_name, "TwoSidedWireframe");
    }

    #[test]
    fn test_modern_render_device_and_interface_commands() {
        let mut device = ModernRenderDevice::new(1920, 1080);
        device.init().unwrap();
        assert_eq!(device.width, 1920);
        assert_eq!(device.height, 1080);

        // Lock device and begin render pass
        let interface = device.lock(ClearValues::default()).unwrap();

        interface.set_viewport(Viewport {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            min_depth: 0.0,
            max_depth: 1.0,
        });

        interface.set_transform(TransformType::View, Mat4::IDENTITY);
        interface.set_transform(TransformType::Projection, Mat4::IDENTITY);
        interface.set_vertex_buffer(0, BufferHandle(1), 0);
        interface.set_index_buffer(BufferHandle(2), 0, IndexFormat::Uint16);

        let mat = MaterialDefinition {
            name: "TestMat".into(),
            blend_mode: BlendMode::Opaque,
            ..Default::default()
        };
        let translated = MaterialTranslator::translate(&mat, 0.0);
        interface.set_material(&translated).unwrap();

        // Draw 2 triangles (6 indices)
        interface.draw_indexed_primitive(PrimitiveType::TriangleList, 0, 2, 0, 3);

        // End pass
        device.unlock().unwrap();

        assert_eq!(device.frame_draw_calls, 1);
        assert_eq!(device.frame_triangles, 2);
        assert_eq!(device.active_commands().len(), 7);

        device.present().unwrap();
        assert_eq!(device.frame_index, 1);
        assert_eq!(device.active_commands().len(), 0);
    }

    #[test]
    fn test_shader_pipeline_config_integration() {
        let mut pso_cache = PsoCache::new();
        let pipeline_config = ShaderPipelineConfig::default_world_pipeline();
        let pso = pipeline_config.create_pso(&mut pso_cache).unwrap();
        assert_eq!(pso.debug_name, "WorldStaticMeshOpaque");
        assert_eq!(pso.id, 1);

        let wf_config = ShaderPipelineConfig::wireframe_pipeline();
        let wf_pso = wf_config.create_pso(&mut pso_cache).unwrap();
        assert_eq!(wf_pso.key.polygon_mode, PolygonMode::Line);
        assert_eq!(wf_pso.key.cull_mode, CullMode::None);
        assert_eq!(wf_pso.id, 2);
    }

    #[test]
    fn test_texture_sampler_cache_vulkan_contract() {
        let mut cache = TextureSamplerCache::new();

        // 1. VK_SAMPLER_ADDRESS_MODE_REPEAT with 16x aniso and -0.5 LOD bias
        let sampler_repeat = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_REPEAT,
            16.0,
            -0.5,
        );
        assert!(!sampler_repeat.is_null());

        // 2. VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
        let sampler_clamp = cache.get_or_create_sampler(
            VkFilter::Linear,
            VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            1.0,
            0.0,
        );
        assert!(!sampler_clamp.is_null());
        assert_ne!(sampler_repeat, sampler_clamp);

        // 3. Descriptor set pooling
        let mut descriptor_pool = DescriptorSetPool::default();
        let set = descriptor_pool
            .get_or_allocate_texture_descriptor(1, 42, sampler_repeat, 0)
            .expect("should allocate descriptor set");
        assert!(!set.is_null());
    }
}
