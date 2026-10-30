use glam::{Mat4, Vec3, Vec4};
use openkf_mesh::*;
use std::path::Path;

#[test]
fn test_vertex_stream_36_bytes_alignment_and_layout() {
    // 1. Validate size and alignment requirements
    assert_eq!(std::mem::size_of::<StaticMeshVertex>(), 36, "StaticMeshVertex must be exactly 36 bytes");
    assert_eq!(std::mem::align_of::<StaticMeshVertex>(), 4, "StaticMeshVertex must be 4-byte aligned for GPU buffers");

    // 2. Validate field offsets
    let dummy = StaticMeshVertex::new([0.0; 3], [0.0; 3], [0.0; 2], [0.0; 2]);
    let base = &dummy as *const _ as usize;
    let pos_off = &dummy.position as *const _ as usize - base;
    let norm_off = &dummy.normal as *const _ as usize - base;
    let uv0_off = &dummy.uv0 as *const _ as usize - base;
    let uv1_off = &dummy.uv1 as *const _ as usize - base;

    assert_eq!(pos_off, 0x00, "Position must start at offset 0x00");
    assert_eq!(norm_off, 0x0C, "Normal must start at offset 0x0C (12 bytes)");
    assert_eq!(uv0_off, 0x18, "UV0 must start at offset 0x18 (24 bytes)");
    assert_eq!(uv1_off, 0x20, "UV1 must start at offset 0x20 (32 bytes)");

    // 3. Validate f16 half-precision UV conversion
    let v = StaticMeshVertex::new(
        [10.0, -20.5, 30.25],
        [0.0, 1.0, 0.0],
        [0.5, 0.75],
        [0.125, 0.875],
    );

    assert_eq!(v.position, [10.0, -20.5, 30.25]);
    assert_eq!(v.normal, [0.0, 1.0, 0.0]);
    assert_eq!(v.uv0, [0.5, 0.75]);

    let decoded_uv1 = v.uv1_f32();
    assert!((decoded_uv1[0] - 0.125).abs() < 1e-4);
    assert!((decoded_uv1[1] - 0.875).abs() < 1e-4);

    // 4. Validate bytemuck cast
    let vertices = vec![v, v, v];
    let bytes: &[u8] = bytemuck::cast_slice(&vertices);
    assert_eq!(bytes.len(), 36 * 3);
}

#[test]
fn test_mesh_instance_data_80_bytes_alignment_and_layout() {
    // 1. Validate size and alignment
    assert_eq!(std::mem::size_of::<MeshInstanceData>(), 80, "MeshInstanceData must be exactly 80 bytes");
    assert_eq!(std::mem::align_of::<MeshInstanceData>(), 16, "MeshInstanceData must be 16-byte aligned (Mat4/Vec4)");

    // 2. Validate field offsets
    let dummy = MeshInstanceData::with_identity_lightmap(Mat4::IDENTITY);
    let base = &dummy as *const _ as usize;
    let mat_off = &dummy.world_matrix as *const _ as usize - base;
    let lm_off = &dummy.lightmap_offset_scale as *const _ as usize - base;

    assert_eq!(mat_off, 0x00, "World matrix must start at offset 0x00");
    assert_eq!(lm_off, 0x40, "Lightmap offset/scale must start at offset 0x40 (64 bytes)");

    // 3. Validate bytemuck casting
    let instances = vec![
        MeshInstanceData::new(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)), Vec4::new(0.0, 0.0, 0.5, 0.5)),
        MeshInstanceData::new(Mat4::from_translation(Vec3::new(20.0, 0.0, 0.0)), Vec4::new(0.5, 0.0, 0.5, 0.5)),
    ];
    let bytes = IndirectInstancingBufferGenerator::instance_bytes(&instances);
    assert_eq!(bytes.len(), 80 * 2);
}

#[test]
fn test_draw_indexed_indirect_command_layout() {
    assert_eq!(std::mem::size_of::<DrawIndexedIndirectCommand>(), 20, "DrawIndexedIndirectCommand must be 20 bytes");
    assert_eq!(std::mem::align_of::<DrawIndexedIndirectCommand>(), 4, "DrawIndexedIndirectCommand must be 4-byte aligned");

    let cmd = DrawIndexedIndirectCommand::new(180, 50, 0, 0, 10);
    assert_eq!(cmd.index_count, 180);
    assert_eq!(cmd.instance_count, 50);
    assert_eq!(cmd.first_index, 0);
    assert_eq!(cmd.vertex_offset, 0);
    assert_eq!(cmd.first_instance, 10);

    let cmd_slice = [cmd];
    let bytes = IndirectInstancingBufferGenerator::command_bytes(&cmd_slice);
    assert_eq!(bytes.len(), 20);
}

#[test]
fn test_indirect_instancing_generator() {
    let section0 = FStaticMeshSection::new(0, 0, 60, 0, 100);
    let section1 = FStaticMeshSection::new(1, 180, 120, 101, 250);
    let sections = vec![section0.clone(), section1];

    let commands = IndirectInstancingBufferGenerator::generate_section_commands(&sections, 32, 5);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].index_count, 180);
    assert_eq!(commands[0].instance_count, 32);
    assert_eq!(commands[0].first_index, 0);
    assert_eq!(commands[0].first_instance, 5);

    assert_eq!(commands[1].index_count, 360);
    assert_eq!(commands[1].instance_count, 32);
    assert_eq!(commands[1].first_index, 180);
    assert_eq!(commands[1].first_instance, 5);

    // Test batch builder
    let instances = vec![MeshInstanceData::with_identity_lightmap(Mat4::IDENTITY)];
    let (buf, cmd) = IndirectInstancingBufferGenerator::build_instanced_batch(&section0, &instances, 0);
    assert_eq!(buf.len(), 80);
    assert_eq!(cmd.instance_count, 1);
    assert_eq!(cmd.index_count, 180);
}

#[test]
fn test_lod_model_validation_and_bounds() {
    let mut model = FStaticMeshLODModel::new(0.0);

    // Add 4 vertices forming a quad / 2 triangles
    model.vertices.push(StaticMeshVertex::new([-10.0, 0.0, -10.0], [0.0, 1.0, 0.0], [0.0, 0.0], [0.0, 0.0]));
    model.vertices.push(StaticMeshVertex::new([10.0, 0.0, -10.0], [0.0, 1.0, 0.0], [1.0, 0.0], [1.0, 0.0]));
    model.vertices.push(StaticMeshVertex::new([10.0, 0.0, 10.0], [0.0, 1.0, 0.0], [1.0, 1.0], [1.0, 1.0]));
    model.vertices.push(StaticMeshVertex::new([-10.0, 0.0, 10.0], [0.0, 1.0, 0.0], [0.0, 1.0], [0.0, 1.0]));

    // 2 triangles: (0, 1, 2) and (0, 2, 3)
    model.indices = vec![0, 1, 2, 0, 2, 3];
    model.sections.push(FStaticMeshSection::new(0, 0, 2, 0, 3));

    assert_eq!(model.total_triangles(), 2);
    assert!(model.validate_indices().is_ok());

    let (bbox, sphere) = model.calculate_bounds();
    assert_eq!(bbox.min, Vec3::new(-10.0, 0.0, -10.0));
    assert_eq!(bbox.max, Vec3::new(10.0, 0.0, 10.0));
    assert_eq!(sphere.center, Vec3::ZERO);
    assert!((sphere.radius - (200.0f32).sqrt()).abs() < 1e-4);

    // Test out of bounds index detection
    model.indices.push(999);
    assert!(model.validate_indices().is_err());
}

#[test]
fn test_kdop18_collision_tree_and_raycast() {
    let mut tree = KdopTree::new(KdopType::Kdop18);

    // Single collision triangle on XZ plane at Y = 5.0
    let v0 = Vec3::new(-10.0, 5.0, -10.0);
    let v1 = Vec3::new(10.0, 5.0, -10.0);
    let v2 = Vec3::new(0.0, 5.0, 10.0);
    tree.triangles.push(KdopCollisionTriangle::new(v0, v1, v2, 0));

    // Construct a loose kDOP-18 leaf node containing this triangle
    let mut min_val = [-100.0f32; 13];
    let mut max_val = [100.0f32; 13];
    // Tight Y slab [4.9..5.1]
    min_val[1] = 4.9;
    max_val[1] = 5.1;

    tree.nodes.push(KdopNode::new_leaf(min_val, max_val, 0, 1));
    tree.root_index = 0;

    // Ray shooting straight down from above: (0, 20, 0) -> (0, -1, 0)
    let hit = tree.ray_cast(Vec3::new(0.0, 20.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 100.0);
    assert!(hit.is_some(), "Downward ray must hit horizontal triangle");
    let hit_res = hit.unwrap();
    assert!((hit_res.distance - 15.0).abs() < 1e-3, "Distance from Y=20 to Y=5 must be 15.0");
    assert!((hit_res.position.y - 5.0).abs() < 1e-3);

    // Ray pointing away from the triangle
    let miss = tree.ray_cast(Vec3::new(0.0, 20.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 100.0);
    assert!(miss.is_none(), "Upward ray must miss");

    // Serialization & deserialization roundtrip
    let mut buf = Vec::new();
    tree.serialize(&mut buf).expect("kDOP serialization must succeed");

    let mut cursor = std::io::Cursor::new(buf);
    let loaded_tree = KdopTree::deserialize(&mut cursor, KdopType::Kdop18).expect("kDOP deserialization must succeed");

    assert_eq!(loaded_tree.kdop_type, KdopType::Kdop18);
    assert_eq!(loaded_tree.nodes.len(), 1);
    assert_eq!(loaded_tree.triangles.len(), 1);
    assert_eq!(loaded_tree.triangles[0].material_index, 0);
}

#[test]
fn test_kdop26_collision_tree() {
    let mut tree = KdopTree::new(KdopType::Kdop26);
    let v0 = Vec3::new(0.0, 0.0, 0.0);
    let v1 = Vec3::new(2.0, 0.0, 0.0);
    let v2 = Vec3::new(0.0, 2.0, 0.0);
    tree.triangles.push(KdopCollisionTriangle::new(v0, v1, v2, 3));

    let min_val = [-10.0f32; 13];
    let max_val = [10.0f32; 13];
    tree.nodes.push(KdopNode::new_leaf(min_val, max_val, 0, 1));
    tree.root_index = 0;

    let mut buf = Vec::new();
    tree.serialize(&mut buf).expect("Serialization must succeed");
    let mut cursor = std::io::Cursor::new(buf);
    let loaded = KdopTree::deserialize(&mut cursor, KdopType::Kdop26).expect("Deserialization must succeed");

    assert_eq!(loaded.kdop_type, KdopType::Kdop26);
    assert_eq!(loaded.triangles[0].material_index, 3);
}

#[test]
fn test_compact_index_decoder() {
    // 1. Single byte positive
    let mut c1 = std::io::Cursor::new(vec![0x0A]); // 10
    assert_eq!(read_compact_index(&mut c1).unwrap(), 10);

    // 2. Single byte negative
    let mut c2 = std::io::Cursor::new(vec![0x85]); // sign bit set, value 5 -> -5
    assert_eq!(read_compact_index(&mut c2).unwrap(), -5);

    // 3. Multi-byte positive
    // b0: 0x40 | 0x01 = 0x41 (more bit set, val 1)
    // b1: 0x02 (val 2 << 6 = 128) -> total 129
    let mut c3 = std::io::Cursor::new(vec![0x41, 0x02]);
    assert_eq!(read_compact_index(&mut c3).unwrap(), 129);
}

#[test]
fn test_live_retail_debugobjects_usx_parsing() {
    let retail_path = Path::new(r"E:\SteamLibrary\steamapps\common\KillingFloor\StaticMeshes\DebugObjects.usx");
    if !retail_path.exists() {
        eprintln!("Retail package DebugObjects.usx not found, skipping live package test");
        return;
    }

    let bytes = std::fs::read(retail_path).expect("Failed to read DebugObjects.usx");
    let parser = UsxPackageParser::parse(bytes).expect("Failed to parse DebugObjects.usx");

    assert_eq!(parser.summary.tag, PACKAGE_MAGIC);
    assert!(parser.names.len() >= 15);
    assert!(parser.exports.len() >= 3);

    // Find StaticMesh exports
    let sm_exports = parser.find_static_mesh_exports();
    assert_eq!(sm_exports.len(), 2, "DebugObjects.usx must contain exactly 2 StaticMesh exports (debugarrow1, debugarrow2)");

    let mesh1 = parser.load_static_mesh(sm_exports[0]).expect("Must load debugarrow1");
    assert_eq!(mesh1.name, "debugarrow1");
    assert!(!mesh1.lod_models.is_empty());
    assert!(!mesh1.lod_models[0].sections.is_empty());

    let mesh2 = parser.load_static_mesh(sm_exports[1]).expect("Must load debugarrow2");
    assert_eq!(mesh2.name, "debugarrow2");
    assert!(!mesh2.lod_models.is_empty());
}

#[test]
fn test_live_retail_civilvehicles_usx_parsing() {
    let retail_path = Path::new(r"E:\SteamLibrary\steamapps\common\KillingFloor\StaticMeshes\CivilVehicles_SM.usx");
    if !retail_path.exists() {
        return;
    }

    let bytes = std::fs::read(retail_path).expect("Failed to read CivilVehicles_SM.usx");
    let parser = UsxPackageParser::parse(bytes).expect("Failed to parse CivilVehicles_SM.usx");

    let sm_exports = parser.find_static_mesh_exports();
    assert!(!sm_exports.is_empty(), "CivilVehicles_SM.usx must contain StaticMesh exports");

    let mesh = parser.load_static_mesh(sm_exports[0]).expect("Must load first vehicle mesh");
    assert!(!mesh.name.is_empty());
    assert!(!mesh.lod_models.is_empty());
    assert!(!mesh.lod_models[0].sections.is_empty());
    assert!(mesh.lod_models[0].sections[0].num_triangles > 0);
}

#[test]
fn test_mesh_batch_renderer_contract_and_frustum_culling() {
    let mut renderer = MeshBatchRenderer::new();

    // 1. Register Mesh 100 with 3 sections (multi-material: slot 0, slot 1, slot 2)
    let s0 = FStaticMeshSection::new(0, 0, 120, 0, 119);      // 360 indices
    let s1 = FStaticMeshSection::new(1, 360, 80, 120, 199);   // 240 indices
    let s2 = FStaticMeshSection::new(2, 600, 200, 200, 399);  // 600 indices
    renderer.register_sections(100, vec![s0, s1, s2]);

    // Register Mesh 200 with 1 section
    let s3 = FStaticMeshSection::new(0, 0, 50, 0, 49);        // 150 indices
    renderer.register_sections(200, vec![s3]);

    // 2. Setup Camera: looking down +Z from (0, 0, 0), near=1.0, far=100.0, FOV=90 deg
    let view = Mat4::look_to_rh(Vec3::ZERO, Vec3::Z, Vec3::Y);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 100.0);
    renderer.set_camera(view, proj);

    let box_unit = FBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));

    // 3. Submit instances:
    // Instance 1: Visible (at z = 20)
    renderer.submit_instance(100, Mat4::from_translation(Vec3::new(0.0, 0.0, 20.0)), box_unit);
    // Instance 2: Visible (at x = 5, y = 5, z = 30)
    renderer.submit_instance(100, Mat4::from_translation(Vec3::new(5.0, 5.0, 30.0)), box_unit);
    // Instance 3: Culled (behind camera at z = -10)
    renderer.submit_instance(100, Mat4::from_translation(Vec3::new(0.0, 0.0, -10.0)), box_unit);
    // Instance 4: Culled (beyond far plane at z = 200)
    renderer.submit_instance(100, Mat4::from_translation(Vec3::new(0.0, 0.0, 200.0)), box_unit);
    // Instance 5: Culled (far off to the side at x = 100, z = 20)
    renderer.submit_instance(100, Mat4::from_translation(Vec3::new(100.0, 0.0, 20.0)), box_unit);

    // Instance 6 of Mesh 200: Visible (at z = 50)
    renderer.submit_instance(200, Mat4::from_translation(Vec3::new(0.0, 0.0, 50.0)), box_unit);
    // Instance 7 of Mesh 200: Culled (at z = -5)
    renderer.submit_instance(200, Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0)), box_unit);

    assert_eq!(renderer.submitted_count(), 7);

    // 4. Generate indirect draws
    let batch = renderer.generate_indirect_draws();

    assert_eq!(batch.total_submitted, 7);
    assert_eq!(batch.total_visible, 3); // 2 instances of Mesh 100, 1 instance of Mesh 200
    assert_eq!(batch.total_culled, 4);

    // Instance buffer must contain exactly 3 instances
    assert_eq!(batch.instances.len(), 3);
    assert_eq!(batch.instance_bytes().len(), 3 * 80);

    // Mesh 100 has 3 sections, Mesh 200 has 1 section => 4 indirect draw commands
    assert_eq!(batch.commands.len(), 4);
    assert_eq!(batch.batches.len(), 4);

    // Check Mesh 100 Section 0
    assert_eq!(batch.commands[0].index_count, 360);
    assert_eq!(batch.commands[0].instance_count, 2);
    assert_eq!(batch.commands[0].first_index, 0);
    assert_eq!(batch.commands[0].first_instance, 0);

    // Check Mesh 100 Section 1
    assert_eq!(batch.commands[1].index_count, 240);
    assert_eq!(batch.commands[1].instance_count, 2);
    assert_eq!(batch.commands[1].first_index, 360);
    assert_eq!(batch.commands[1].first_instance, 0);

    // Check Mesh 100 Section 2
    assert_eq!(batch.commands[2].index_count, 600);
    assert_eq!(batch.commands[2].instance_count, 2);
    assert_eq!(batch.commands[2].first_index, 600);
    assert_eq!(batch.commands[2].first_instance, 0);

    // Check Mesh 200 Section 0
    assert_eq!(batch.commands[3].index_count, 150);
    assert_eq!(batch.commands[3].instance_count, 1);
    assert_eq!(batch.commands[3].first_index, 0);
    assert_eq!(batch.commands[3].first_instance, 2); // follows 2 instances of Mesh 100
}

#[test]
fn test_mesh_batch_renderer_material_sorting_and_grouping() {
    let mut renderer = MeshBatchRenderer::new();

    // Mesh 1: material 2 (sec 0), material 0 (sec 1)
    let s0 = FStaticMeshSection::new(2, 0, 10, 0, 9);
    let s1 = FStaticMeshSection::new(0, 30, 20, 10, 29);
    renderer.register_sections(1, vec![s0, s1]);

    // Mesh 2: material 1 (sec 0), material 0 (sec 1)
    let s2 = FStaticMeshSection::new(1, 0, 15, 0, 14);
    let s3 = FStaticMeshSection::new(0, 45, 25, 15, 39);
    renderer.register_sections(2, vec![s2, s3]);

    let bounds = FBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    renderer.submit_instance(1, Mat4::IDENTITY, bounds);
    renderer.submit_instance(2, Mat4::IDENTITY, bounds);

    let batch = renderer.generate_indirect_draws_sorted_by_material();
    assert_eq!(batch.batches.len(), 4);

    // Sorted by material_index: material 0 (Mesh 1 sec 1), material 0 (Mesh 2 sec 1),
    // material 1 (Mesh 2 sec 0), material 2 (Mesh 1 sec 0)
    assert_eq!(batch.batches[0].material_index, 0);
    assert_eq!(batch.batches[1].material_index, 0);
    assert_eq!(batch.batches[2].material_index, 1);
    assert_eq!(batch.batches[3].material_index, 2);

    let grouped = batch.batches_by_material();
    assert_eq!(grouped.get(&0).unwrap().len(), 2);
    assert_eq!(grouped.get(&1).unwrap().len(), 1);
    assert_eq!(grouped.get(&2).unwrap().len(), 1);
}

#[test]
fn test_mesh_batch_renderer_with_live_retail_usx() {
    let retail_path = Path::new(r"E:\SteamLibrary\steamapps\common\KillingFloor\StaticMeshes\DebugObjects.usx");
    if !retail_path.exists() {
        return;
    }

    let bytes = std::fs::read(retail_path).expect("Failed to read DebugObjects.usx");
    let parser = UsxPackageParser::parse(bytes).expect("Failed to parse DebugObjects.usx");
    let sm_exports = parser.find_static_mesh_exports();

    let mesh1 = parser.load_static_mesh(sm_exports[0]).expect("Load debugarrow1");
    let mesh2 = parser.load_static_mesh(sm_exports[1]).expect("Load debugarrow2");

    let mut renderer = MeshBatchRenderer::new();
    renderer.register_mesh(10, &mesh1);
    renderer.register_mesh(20, &mesh2);

    // Setup Camera
    let view = Mat4::look_to_rh(Vec3::new(0.0, 0.0, -50.0), Vec3::Z, Vec3::Y);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 1000.0);
    renderer.set_camera(view, proj);

    // Submit 10 instances of mesh1 along the Z axis (5 in front, 5 behind camera)
    for i in 0..10 {
        let z = (i as f32 - 5.0) * 100.0;
        let transform = Mat4::from_translation(Vec3::new(0.0, 0.0, z));
        assert!(renderer.submit_mesh_instance(10, transform));
    }

    let batch = renderer.generate_indirect_draws();
    assert_eq!(batch.total_submitted, 10);
    // Those behind camera (z <= -50) are culled, those in front (z > -50) are visible
    assert!(batch.total_visible > 0);
    assert!(batch.total_culled > 0);
    assert_eq!(batch.total_submitted, batch.total_visible + batch.total_culled);

    // Each visible instance has commands generated for each section of debugarrow1
    let sec_count = mesh1.base_lod().unwrap().sections.len();
    assert_eq!(batch.commands.len(), sec_count);
    assert_eq!(batch.commands[0].instance_count, batch.total_visible as u32);
    assert_eq!(batch.commands[0].first_index, mesh1.base_lod().unwrap().sections[0].first_index);
}
