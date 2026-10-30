pub mod anim;
pub mod bone;
pub mod error;
pub mod mesh;
pub mod shader;
pub mod skinning;
pub mod slerp;

pub use anim::{AnimBoneTrack, UAnimSequence, UMeshAnimation, UANIM_MAGIC, UANIM_VERSION};
pub use bone::{BoneTransform, FBone, FBoneBinary, Skeleton};
pub use error::AnimationError;
pub use mesh::{
    FBoxSphereBounds, FSkelMaterial, FSkelMeshLODModel, FSkelMeshSection, FSoftVertex,
    USkeletalMesh, USKELMESH_MAGIC, USKELMESH_VERSION,
};
pub use shader::{COMPUTE_SKINNING_HLSL, COMPUTE_SKINNING_WGSL};
pub use skinning::{cpu_skin_vertices, BoneMatrix, VertexInput, VertexOutput};
pub use slerp::{blend_poses, blend_transforms, lerp_vec3, slerp};

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Quat, Vec2, Vec3};

    #[test]
    fn test_fbone_binary_exact_size() {
        assert_eq!(
            std::mem::size_of::<FBoneBinary>(),
            48,
            "FBoneBinary must be strictly 48 bytes matching retail UE2 layout"
        );
        assert_eq!(std::mem::align_of::<FBoneBinary>(), 4);
    }

    #[test]
    fn test_vertex_input_and_output_layout_sizes() {
        assert_eq!(
            std::mem::size_of::<VertexInput>(),
            64,
            "VertexInput must be strictly 64 bytes"
        );
        assert_eq!(
            std::mem::size_of::<VertexOutput>(),
            32,
            "VertexOutput must be strictly 32 bytes"
        );
        assert_eq!(
            std::mem::size_of::<BoneMatrix>(),
            64,
            "BoneMatrix must be strictly 64 bytes"
        );
    }

    #[test]
    fn test_slerp_interpolation() {
        let q0 = Quat::IDENTITY;
        // 90 degrees around Z axis
        let q1 = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);

        // At t = 0.0 -> q0
        let s0 = slerp(q0, q1, 0.0);
        assert!((s0.x - q0.x).abs() < 1e-5);
        assert!((s0.y - q0.y).abs() < 1e-5);
        assert!((s0.z - q0.z).abs() < 1e-5);
        assert!((s0.w - q0.w).abs() < 1e-5);

        // At t = 1.0 -> q1
        let s1 = slerp(q0, q1, 1.0);
        assert!((s1.x - q1.x).abs() < 1e-5);
        assert!((s1.y - q1.y).abs() < 1e-5);
        assert!((s1.z - q1.z).abs() < 1e-5);
        assert!((s1.w - q1.w).abs() < 1e-5);

        // At t = 0.5 -> 45 degrees around Z axis
        let s_mid = slerp(q0, q1, 0.5);
        let q_expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        assert!((s_mid.x - q_expected.x).abs() < 1e-5);
        assert!((s_mid.y - q_expected.y).abs() < 1e-5);
        assert!((s_mid.z - q_expected.z).abs() < 1e-5);
        assert!((s_mid.w - q_expected.w).abs() < 1e-5);
    }

    #[test]
    fn test_slerp_shortest_path_negation() {
        let q0 = Quat::IDENTITY;
        // -q0 represents the identical rotation orientation
        let q_neg = -Quat::IDENTITY;

        // Slerp must detect negative dot product, negate q_b and smoothly interpolate without flipping
        let s = slerp(q0, q_neg, 0.5);
        assert!((s.w - 1.0).abs() < 1e-5);
        assert!(s.x.abs() < 1e-5);
        assert!(s.y.abs() < 1e-5);
        assert!(s.z.abs() < 1e-5);
    }

    #[test]
    fn test_slerp_nearly_identical_nlerp_fallback() {
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_rotation_z(1e-4);

        let s = slerp(q0, q1, 0.5);
        assert!(!s.x.is_nan());
        assert!(!s.y.is_nan());
        assert!(!s.z.is_nan());
        assert!(!s.w.is_nan());
        assert!((s.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_bone_transform_propagation() {
        let mut skeleton = Skeleton::new();

        // Bone 0: Root at (0, 0, 100), rotated 90 degrees yaw (around Z)
        let root_rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        skeleton.add_bone(FBone::new("Root", -1, root_rot, Vec3::new(0.0, 0.0, 100.0)));

        // Bone 1: Child offset along local X by +50.
        // Rotated by 90 degrees in parent space, local X (50, 0, 0) becomes world Y (+50).
        // World position should be (0, 50, 100).
        skeleton.add_bone(FBone::new("Spine", 0, Quat::IDENTITY, Vec3::new(50.0, 0.0, 0.0)));

        // Bone 2: Grandchild offset along local Y by +20.
        // Local Y (0, 20, 0) rotated by 90 degrees becomes world X (-20).
        // World position should be (-20, 50, 100).
        skeleton.add_bone(FBone::new("Head", 1, Quat::IDENTITY, Vec3::new(0.0, 20.0, 0.0)));

        let world_bind = skeleton.compute_bind_pose_world_transforms().expect("Valid hierarchy");

        assert_eq!(world_bind.len(), 3);

        let root_pos = world_bind[0].transform_point3(Vec3::ZERO);
        assert!((root_pos.x - 0.0).abs() < 1e-4);
        assert!((root_pos.y - 0.0).abs() < 1e-4);
        assert!((root_pos.z - 100.0).abs() < 1e-4);

        let spine_pos = world_bind[1].transform_point3(Vec3::ZERO);
        assert!((spine_pos.x - 0.0).abs() < 1e-4);
        assert!((spine_pos.y - 50.0).abs() < 1e-4);
        assert!((spine_pos.z - 100.0).abs() < 1e-4);

        let head_pos = world_bind[2].transform_point3(Vec3::ZERO);
        assert!((head_pos.x - (-20.0)).abs() < 1e-4);
        assert!((head_pos.y - 50.0).abs() < 1e-4);
        assert!((head_pos.z - 100.0).abs() < 1e-4);
    }

    #[test]
    fn test_inverse_bind_matrices_identity() {
        let mut skeleton = Skeleton::new();
        skeleton.add_bone(FBone::new(
            "Root",
            -1,
            Quat::from_rotation_x(0.3),
            Vec3::new(10.0, 20.0, 30.0),
        ));
        skeleton.add_bone(FBone::new(
            "Chest",
            0,
            Quat::from_rotation_y(0.5),
            Vec3::new(0.0, 15.0, 0.0),
        ));

        let world_bind = skeleton.compute_bind_pose_world_transforms().unwrap();
        let inv_bind = skeleton.compute_inverse_bind_matrices().unwrap();

        for i in 0..skeleton.len() {
            let identity_check = world_bind[i] * inv_bind[i];
            let diff = (identity_check - Mat4::IDENTITY).to_cols_array();
            for &val in &diff {
                assert!(val.abs() < 1e-4, "M * M^-1 must be identity");
            }
        }

        // At rest pose, skinning matrices are identity
        let skin_matrices = skeleton
            .compute_skinning_matrices(&world_bind, &inv_bind)
            .unwrap();
        for mat in skin_matrices {
            let diff = (mat - Mat4::IDENTITY).to_cols_array();
            for &val in &diff {
                assert!(val.abs() < 1e-4, "Rest pose skinning matrix must be identity");
            }
        }
    }

    #[test]
    fn test_cpu_skinning_deformation() {
        let mut skeleton = Skeleton::new();
        skeleton.add_bone(FBone::new("Root", -1, Quat::IDENTITY, Vec3::ZERO));
        skeleton.add_bone(FBone::new("Arm", 0, Quat::IDENTITY, Vec3::new(10.0, 0.0, 0.0)));

        let inv_bind = skeleton.compute_inverse_bind_matrices().unwrap();

        // Animate Arm bone: translate it up by +10 in Z
        let animated_local = vec![
            BoneTransform::from_rotation_translation(Quat::IDENTITY, Vec3::ZERO),
            BoneTransform::from_rotation_translation(Quat::IDENTITY, Vec3::new(10.0, 0.0, 10.0)),
        ];

        let anim_world = skeleton.evaluate_world_transforms(&animated_local).unwrap();
        let skin_mats = skeleton.compute_skinning_matrices(&anim_world, &inv_bind).unwrap();

        // Vertex 1: 100% bound to Root (bone 0)
        // Vertex 2: 100% bound to Arm (bone 1)
        // Vertex 3: 50% bound to Root, 50% bound to Arm
        let inputs = vec![
            VertexInput {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
                bones: [0, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0],
            },
            VertexInput {
                position: [10.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.5, 0.5],
                bones: [1, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0],
            },
            VertexInput {
                position: [5.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.25, 0.25],
                bones: [0, 1, 0, 0],
                weights: [0.5, 0.5, 0.0, 0.0],
            },
        ];

        let outputs = cpu_skin_vertices(&inputs, &skin_mats);
        assert_eq!(outputs.len(), 3);

        // Vertex 1 does not move: stays at (0, 0, 0)
        assert!((outputs[0].position[0] - 0.0).abs() < 1e-4);
        assert!((outputs[0].position[1] - 0.0).abs() < 1e-4);
        assert!((outputs[0].position[2] - 0.0).abs() < 1e-4);

        // Vertex 2 translated by (0, 0, +10) -> (10, 0, 10)
        assert!((outputs[1].position[0] - 10.0).abs() < 1e-4);
        assert!((outputs[1].position[1] - 0.0).abs() < 1e-4);
        assert!((outputs[1].position[2] - 10.0).abs() < 1e-4);

        // Vertex 3 blended 50/50: original x=5, z=0 + 0.5*(10) = 5 -> (5, 0, 5)
        assert!((outputs[2].position[0] - 5.0).abs() < 1e-4);
        assert!((outputs[2].position[1] - 0.0).abs() < 1e-4);
        assert!((outputs[2].position[2] - 5.0).abs() < 1e-4);

        // Normals stay pointing up and normalized
        for out in &outputs {
            assert!((out.normal[0] - 0.0).abs() < 1e-4);
            assert!((out.normal[1] - 0.0).abs() < 1e-4);
            assert!((out.normal[2] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn test_uskeletal_mesh_binary_roundtrip() {
        let mut skeleton = Skeleton::new();
        skeleton.add_bone(FBone::new("Root", -1, Quat::IDENTITY, Vec3::ZERO));
        skeleton.add_bone(FBone::new("Hand", 0, Quat::from_rotation_y(0.2), Vec3::new(10.0, 0.0, 0.0)));

        let mut mesh = USkeletalMesh::new(skeleton);
        mesh.default_anim_name = Some("KF_Soldier_Anim".to_string());
        mesh.materials.push(FSkelMaterial {
            material_index: 0,
            material_name: "Soldier_Body_Mat".to_string(),
        });

        let sec = FSkelMeshSection {
            material_index: 0,
            first_index: 0,
            num_triangles: 1,
            min_stream_index: 0,
            max_stream_index: 2,
            bone_map: vec![0, 1],
        };

        let v0 = FSoftVertex {
            position: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 1.0, 0.0),
            uv: Vec2::new(0.0, 0.0),
            bone_indices: [0, 0, 0, 0],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
        };
        let v1 = FSoftVertex {
            position: Vec3::new(1.0, 0.0, 0.0),
            normal: Vec3::new(0.0, 1.0, 0.0),
            uv: Vec2::new(1.0, 0.0),
            bone_indices: [0, 1, 0, 0],
            bone_weights: [0.5, 0.5, 0.0, 0.0],
        };
        let v2 = FSoftVertex {
            position: Vec3::new(0.0, 1.0, 0.0),
            normal: Vec3::new(0.0, 1.0, 0.0),
            uv: Vec2::new(0.0, 1.0),
            bone_indices: [1, 0, 0, 0],
            bone_weights: [1.0, 0.0, 0.0, 0.0],
        };

        mesh.lod_models.push(FSkelMeshLODModel {
            sections: vec![sec],
            vertices: vec![v0, v1, v2],
            raw_indices: vec![0, 1, 2],
            num_bones: 2,
        });

        let bytes = mesh.to_bytes().expect("Serialization succeeded");
        let deserialized = USkeletalMesh::from_bytes(&bytes).expect("Deserialization succeeded");

        assert_eq!(deserialized.default_anim_name, mesh.default_anim_name);
        assert_eq!(deserialized.ref_skeleton.bones.len(), 2);
        assert_eq!(deserialized.ref_skeleton.bones[0].name, "Root");
        assert_eq!(deserialized.ref_skeleton.bones[1].name, "Hand");
        assert_eq!(deserialized.materials.len(), 1);
        assert_eq!(deserialized.lod_models.len(), 1);
        assert_eq!(deserialized.lod_models[0].vertices.len(), 3);
        assert_eq!(deserialized.lod_models[0].raw_indices, vec![0, 1, 2]);

        // Verify conversion to compute shader VertexInput
        let inputs = deserialized.lod_models[0].to_vertex_inputs();
        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[1].weights, [0.5, 0.5, 0.0, 0.0]);
        assert_eq!(inputs[1].bones, [0, 1, 0, 0]);
    }

    #[test]
    fn test_umesh_animation_binary_roundtrip_and_sampling() {
        let mut anim = UMeshAnimation::new();
        let mut seq = UAnimSequence::new("WalkF", 2, 30.0);

        let mut track = AnimBoneTrack::new("Root");
        track.rotations.push(Quat::IDENTITY);
        track.rotations.push(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        track.positions.push(Vec3::new(0.0, 0.0, 0.0));
        track.positions.push(Vec3::new(10.0, 0.0, 0.0));

        seq.tracks.push(track);
        anim.add_sequence(seq);

        let bytes = anim.to_bytes().expect("Serialization succeeded");
        let deserialized = UMeshAnimation::from_bytes(&bytes).expect("Deserialization succeeded");

        assert_eq!(deserialized.sequences.len(), 1);
        let loaded_seq = deserialized.find_sequence("WalkF").expect("Found WalkF");
        assert_eq!(loaded_seq.num_frames, 2);
        assert_eq!(loaded_seq.rate, 30.0);

        let mut skeleton = Skeleton::new();
        skeleton.add_bone(FBone::new("Root", -1, Quat::IDENTITY, Vec3::ZERO));

        // Sample at t = 0.0 -> frame 0
        let pose0 = loaded_seq.sample_pose(&skeleton, 0.0, false);
        assert_eq!(pose0.len(), 1);
        assert!((pose0[0].translation.x - 0.0).abs() < 1e-4);

        // Sample at t = duration/2 -> midpoint
        let mid_time = loaded_seq.duration() * 0.5;
        let pose_mid = loaded_seq.sample_pose(&skeleton, mid_time, false);
        assert!((pose_mid[0].translation.x - 5.0).abs() < 1e-4);

        // Midpoint rotation is 45 degrees
        let expected_rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        assert!((pose_mid[0].rotation.z - expected_rot.z).abs() < 1e-4);
    }

    #[test]
    fn test_compute_shader_sources_contain_spec_elements() {
        assert!(COMPUTE_SKINNING_HLSL.contains("struct VertexInput"));
        assert!(COMPUTE_SKINNING_HLSL.contains("StructuredBuffer<VertexInput> InputVertices"));
        assert!(COMPUTE_SKINNING_HLSL.contains("StructuredBuffer<float4x4>    BoneMatrices"));
        assert!(COMPUTE_SKINNING_HLSL.contains("[numthreads(64, 1, 1)]"));
        assert!(COMPUTE_SKINNING_HLSL.contains("CSMain"));

        assert!(COMPUTE_SKINNING_WGSL.contains("@compute @workgroup_size(64, 1, 1)"));
        assert!(COMPUTE_SKINNING_WGSL.contains("fn cs_main"));
    }
}
