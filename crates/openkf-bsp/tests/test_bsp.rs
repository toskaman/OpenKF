//! Comprehensive integration and unit tests for openkf-bsp.

use glam::Vec3;
use openkf_bsp::{
    extract_bsp_geometry, extract_unified_mesh, is_convex_polygon, polygon_area,
    project_lightmap_uv, project_texture_uv, triangulate_fan,
    FBox, FBspNode, FBspSurf, FPlane, FSphere, FVector, FVert,
    UModel, UModelParser,
};
use std::mem::{offset_of, size_of};

#[test]
fn test_mandatory_binary_specifications() {
    // 1. FBspNode must be exactly 64 bytes
    assert_eq!(size_of::<FBspNode>(), 64);
    assert_eq!(offset_of!(FBspNode, plane), 0x00);
    assert_eq!(offset_of!(FBspNode, zone_mask), 0x10);
    assert_eq!(offset_of!(FBspNode, node_flags), 0x18);
    assert_eq!(offset_of!(FBspNode, i_vert_pool), 0x1C);
    assert_eq!(offset_of!(FBspNode, i_surf), 0x20);
    assert_eq!(offset_of!(FBspNode, i_child), 0x24);
    assert_eq!(offset_of!(FBspNode, i_collision_bound), 0x2C);
    assert_eq!(offset_of!(FBspNode, i_zone), 0x30);
    assert_eq!(offset_of!(FBspNode, num_vertices), 0x32);

    // 2. FBspSurf must be exactly 60 bytes
    assert_eq!(size_of::<FBspSurf>(), 60);
    assert_eq!(offset_of!(FBspSurf, material), 0x00);
    assert_eq!(offset_of!(FBspSurf, poly_flags), 0x04);
    assert_eq!(offset_of!(FBspSurf, p_base), 0x08);
    assert_eq!(offset_of!(FBspSurf, v_normal), 0x0C);
    assert_eq!(offset_of!(FBspSurf, v_texture_u), 0x10);
    assert_eq!(offset_of!(FBspSurf, v_texture_v), 0x14);
    assert_eq!(offset_of!(FBspSurf, i_light_map), 0x18);
    assert_eq!(offset_of!(FBspSurf, light_map_scale), 0x1C);

    // 3. FVert must be exactly 8 bytes
    assert_eq!(size_of::<FVert>(), 8);
    assert_eq!(offset_of!(FVert, p_vertex), 0x00);
    assert_eq!(offset_of!(FVert, i_side), 0x04);

    // 4. Vectors and Points (FVector) must be 12 bytes
    assert_eq!(size_of::<FVector>(), 12);

    // 5. FPlane must be 16 bytes
    assert_eq!(size_of::<FPlane>(), 16);

    // 6. FBox (25 bytes) and FSphere (16 bytes)
    assert_eq!(size_of::<FBox>(), 25);
    assert_eq!(size_of::<FSphere>(), 16);
}

#[test]
fn test_polygon_triangulator_various_ngons() {
    // Triangle: 1 triangle, 3 indices
    let mut indices_tri = Vec::new();
    assert_eq!(triangulate_fan(3, 0, &mut indices_tri), 1);
    assert_eq!(indices_tri, vec![0, 1, 2]);

    // Quad: 2 triangles, 6 indices
    let mut indices_quad = Vec::new();
    assert_eq!(triangulate_fan(4, 10, &mut indices_quad), 2);
    assert_eq!(indices_quad, vec![10, 11, 12, 10, 12, 13]);

    // Pentagon: 3 triangles, 9 indices
    let mut indices_pent = Vec::new();
    assert_eq!(triangulate_fan(5, 0, &mut indices_pent), 3);
    assert_eq!(indices_pent, vec![0, 1, 2, 0, 2, 3, 0, 3, 4]);

    // Octagon: 6 triangles, 18 indices
    let mut indices_oct = Vec::new();
    assert_eq!(triangulate_fan(8, 100, &mut indices_oct), 6);
    assert_eq!(indices_oct.len(), 18);
    assert_eq!(
        indices_oct,
        vec![
            100, 101, 102,
            100, 102, 103,
            100, 103, 104,
            100, 104, 105,
            100, 105, 106,
            100, 106, 107
        ]
    );

    // Degenerate polygons
    let mut indices_deg = Vec::new();
    assert_eq!(triangulate_fan(0, 0, &mut indices_deg), 0);
    assert_eq!(triangulate_fan(1, 0, &mut indices_deg), 0);
    assert_eq!(triangulate_fan(2, 0, &mut indices_deg), 0);
    assert!(indices_deg.is_empty());
}

#[test]
fn test_polygon_area_and_convexity() {
    // Regular hexagon with radius 10.0
    let mut hex = Vec::new();
    let num_sides = 6;
    for i in 0..num_sides {
        let angle = (i as f32) * std::f32::consts::TAU / (num_sides as f32);
        hex.push(Vec3::new(10.0 * angle.cos(), 10.0 * angle.sin(), 0.0));
    }

    assert!(is_convex_polygon(&hex, Vec3::new(0.0, 0.0, 1.0)));
    let area = polygon_area(&hex);
    // Area of regular hexagon with r=10: 3 * sqrt(3) / 2 * r^2 ≈ 259.8076
    let expected_area = (3.0 * 3.0f32.sqrt() / 2.0) * 100.0;
    assert!((area - expected_area).abs() < 1e-2);
}

#[test]
fn test_uv_projection_and_scaling() {
    let base = Vec3::new(100.0, 100.0, 0.0);
    let u_axis = Vec3::new(2.0, 0.0, 0.0); // Length = 2, so ||U||^2 = 4
    let v_axis = Vec3::new(0.0, 4.0, 0.0); // Length = 4, so ||V||^2 = 16

    let p = Vec3::new(108.0, 132.0, 0.0);
    // diff = (8, 32, 0)
    // u = (8 * 2) / 4 = 16 / 4 = 4.0
    // v = (32 * 4) / 16 = 128 / 16 = 8.0
    let uv = project_texture_uv(p, base, u_axis, v_axis);
    assert!((uv.x - 4.0).abs() < 1e-5);
    assert!((uv.y - 8.0).abs() < 1e-5);

    // Lightmap UV with scale = 32.0:
    let lm_uv = project_lightmap_uv(p, base, u_axis, v_axis, 32.0);
    assert!((lm_uv.x - (4.0 / 32.0)).abs() < 1e-5);
    assert!((lm_uv.y - (8.0 / 32.0)).abs() < 1e-5);
}

#[test]
fn test_extract_full_subtractive_room() {
    // Construct a complete 1024x1024x512 hollow room (6 faces: Floor, Ceiling, 4 Walls)
    let mut model = UModel::new();
    model.bounding_box = FBox {
        min: FVector::new(-512.0, -512.0, 0.0),
        max: FVector::new(512.0, 512.0, 512.0),
        is_valid: 1,
    };
    model.bounding_sphere = FSphere {
        center: FVector::new(0.0, 0.0, 256.0),
        radius: 900.0,
    };

    // Shared normals
    // 0: Floor (0, 0, -1), 1: Ceiling (0, 0, 1), 2: West (-1, 0, 0), 3: East (1, 0, 0), 4: South (0, -1, 0), 5: North (0, 1, 0)
    model.vectors = vec![
        FVector::new(0.0, 0.0, -1.0), // 0: Normal Floor
        FVector::new(0.0, 0.0, 1.0),  // 1: Normal Ceiling
        FVector::new(-1.0, 0.0, 0.0), // 2: Normal West
        FVector::new(1.0, 0.0, 0.0),  // 3: Normal East
        FVector::new(0.0, -1.0, 0.0), // 4: Normal South
        FVector::new(0.0, 1.0, 0.0),  // 5: Normal North
        // Texture axes
        FVector::new(1.0, 0.0, 0.0),  // 6: TexU X
        FVector::new(0.0, 1.0, 0.0),  // 7: TexV Y
        FVector::new(0.0, 0.0, 1.0),  // 8: TexV Z
    ];

    // Helper to add a quad face
    let mut add_quad = |pts: [FVector; 4], norm_idx: i32, u_idx: i32, v_idx: i32, mat_idx: i32| {
        let base_pt_idx = model.points.len() as i32;
        for p in &pts {
            model.points.push(*p);
        }

        let surf_idx = model.bsp_surfs.len() as i32;
        let mut surf = FBspSurf::default();
        surf.material = mat_idx;
        surf.p_base = base_pt_idx;
        surf.v_normal = norm_idx;
        surf.v_texture_u = u_idx;
        surf.v_texture_v = v_idx;
        surf.i_light_map = 1;
        surf.light_map_scale = 32.0;
        model.bsp_surfs.push(surf);

        let vert_pool_start = model.verts.len() as i32;
        for i in 0..4 {
            model.verts.push(FVert {
                p_vertex: base_pt_idx + i,
                i_side: i,
            });
        }

        let mut node = FBspNode::default();
        let norm = model.vectors[norm_idx as usize];
        let p0 = pts[0];
        let d = norm.x * p0.x + norm.y * p0.y + norm.z * p0.z;
        node.plane = FPlane::new(norm.x, norm.y, norm.z, d);
        node.i_surf = surf_idx;
        node.i_vert_pool = vert_pool_start;
        node.num_vertices = 4;
        model.bsp_nodes.push(node);
    };

    // 1. Floor (Z = 0)
    add_quad(
        [
            FVector::new(-512.0, -512.0, 0.0),
            FVector::new(512.0, -512.0, 0.0),
            FVector::new(512.0, 512.0, 0.0),
            FVector::new(-512.0, 512.0, 0.0),
        ],
        0, 6, 7, 100, // Mat 100: Floor
    );

    // 2. Ceiling (Z = 512)
    add_quad(
        [
            FVector::new(-512.0, 512.0, 512.0),
            FVector::new(512.0, 512.0, 512.0),
            FVector::new(512.0, -512.0, 512.0),
            FVector::new(-512.0, -512.0, 512.0),
        ],
        1, 6, 7, 200, // Mat 200: Ceiling
    );

    // 3. West Wall (X = -512)
    add_quad(
        [
            FVector::new(-512.0, -512.0, 0.0),
            FVector::new(-512.0, 512.0, 0.0),
            FVector::new(-512.0, 512.0, 512.0),
            FVector::new(-512.0, -512.0, 512.0),
        ],
        2, 7, 8, 300, // Mat 300: Wall
    );

    // 4. East Wall (X = 512)
    add_quad(
        [
            FVector::new(512.0, 512.0, 0.0),
            FVector::new(512.0, -512.0, 0.0),
            FVector::new(512.0, -512.0, 512.0),
            FVector::new(512.0, 512.0, 512.0),
        ],
        3, 7, 8, 300,
    );

    // 5. South Wall (Y = -512)
    add_quad(
        [
            FVector::new(512.0, -512.0, 0.0),
            FVector::new(-512.0, -512.0, 0.0),
            FVector::new(-512.0, -512.0, 512.0),
            FVector::new(512.0, -512.0, 512.0),
        ],
        4, 6, 8, 300,
    );

    // 6. North Wall (Y = 512)
    add_quad(
        [
            FVector::new(-512.0, 512.0, 0.0),
            FVector::new(512.0, 512.0, 0.0),
            FVector::new(512.0, 512.0, 512.0),
            FVector::new(-512.0, 512.0, 512.0),
        ],
        5, 6, 8, 300,
    );

    assert_eq!(model.bsp_nodes.len(), 6);
    assert_eq!(model.bsp_surfs.len(), 6);
    assert_eq!(model.points.len(), 24);
    assert_eq!(model.verts.len(), 24);

    // Extract by material
    let meshes = extract_bsp_geometry(&model);
    // 3 distinct materials: 100 (floor), 200 (ceiling), 300 (4 walls)
    assert_eq!(meshes.len(), 3);

    let wall_mesh = meshes.iter().find(|m| m.material_index == 300).expect("wall mesh");
    assert_eq!(wall_mesh.positions.len(), 16); // 4 walls * 4 vertices
    assert_eq!(wall_mesh.triangle_count(), 8);  // 4 walls * 2 triangles
    assert_eq!(wall_mesh.indices.len(), 24);

    let floor_mesh = meshes.iter().find(|m| m.material_index == 100).expect("floor mesh");
    assert_eq!(floor_mesh.positions.len(), 4);
    assert_eq!(floor_mesh.triangle_count(), 2);

    let ceil_mesh = meshes.iter().find(|m| m.material_index == 200).expect("ceil mesh");
    assert_eq!(ceil_mesh.positions.len(), 4);
    assert_eq!(ceil_mesh.triangle_count(), 2);

    // Extract unified mesh
    let unified = extract_unified_mesh(&model);
    assert_eq!(unified.positions.len(), 24);
    assert_eq!(unified.triangle_count(), 12); // 6 faces * 2 = 12 triangles
    assert_eq!(unified.indices.len(), 36);

    // Verify GPU interleaved vertex buffer
    let interleaved = unified.to_interleaved_vertices();
    assert_eq!(interleaved.len(), 24);
    assert_eq!(interleaved[0].normal, [0.0, 0.0, -1.0]);

    // Test serialization roundtrip
    let raw_bytes = UModelParser::serialize_raw(&model);
    let parsed_model = UModelParser::parse_raw(&raw_bytes).expect("parse full room");
    assert_eq!(parsed_model.bsp_nodes.len(), 6);
    assert_eq!(parsed_model.bsp_surfs.len(), 6);
    assert_eq!(parsed_model.points.len(), 24);
    assert_eq!(parsed_model.verts.len(), 24);
}
