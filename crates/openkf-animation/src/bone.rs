use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};

use crate::error::AnimationError;

/// Binary on-disk / in-memory representation of an Unreal Engine 2 bone entry.
/// Exactly 48 bytes matching retail `Engine.dll` `USkeletalMesh::RefSkeleton` memory layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct FBoneBinary {
    /// Bone identifier FName entry index in package NameTable
    pub name_index: i32,
    /// Bone identifier FName suffix/number
    pub name_number: i32,
    /// Bone simulation, rendering, and physics properties flags
    pub flags: u32,
    /// Rest-pose local quaternion orientation relative to parent: [X, Y, Z, W] (16 bytes)
    pub orientation: [f32; 4],
    /// Rest-pose local translation vector relative to parent: [X, Y, Z] (12 bytes)
    pub position: [f32; 3],
    /// Visual bone length for editor wireframe / IK solver
    pub length: f32,
    /// Index of parent bone in skeleton array (-1 for root bone)
    pub parent_index: i32,
}

// Compile-time assertion: FBoneBinary is strictly 48 bytes.
const _: () = assert!(std::mem::size_of::<FBoneBinary>() == 48);

/// High-level representation of an Unreal Engine 2 skeletal bone.
#[derive(Clone, Debug, PartialEq)]
pub struct FBone {
    /// Resolved bone identifier string (e.g. "Bip01", "Bip01_Spine", "Bip01_Head")
    pub name: String,
    /// Bone simulation and physics flags
    pub flags: u32,
    /// Rest-pose local quaternion orientation relative to parent
    pub orientation: Quat,
    /// Rest-pose local translation vector relative to parent
    pub position: Vec3,
    /// Visual bone length
    pub length: f32,
    /// Index of parent bone (-1 for root bone)
    pub parent_index: i32,
}

impl FBone {
    pub fn new(
        name: impl Into<String>,
        parent_index: i32,
        orientation: Quat,
        position: Vec3,
    ) -> Self {
        Self {
            name: name.into(),
            flags: 0,
            orientation,
            position,
            length: 1.0,
            parent_index,
        }
    }

    /// Converts the rest-pose local orientation and position to a local transformation matrix.
    pub fn local_transform_matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.orientation, self.position)
    }

    /// Serializes into a 48-byte binary layout given name table indices.
    pub fn to_binary(&self, name_index: i32, name_number: i32) -> FBoneBinary {
        FBoneBinary {
            name_index,
            name_number,
            flags: self.flags,
            orientation: [
                self.orientation.x,
                self.orientation.y,
                self.orientation.z,
                self.orientation.w,
            ],
            position: [self.position.x, self.position.y, self.position.z],
            length: self.length,
            parent_index: self.parent_index,
        }
    }

    /// Constructs an `FBone` from an `FBoneBinary` and resolved name string.
    pub fn from_binary(binary: &FBoneBinary, name: String) -> Self {
        Self {
            name,
            flags: binary.flags,
            orientation: Quat::from_array(binary.orientation),
            position: Vec3::from_array(binary.position),
            length: binary.length,
            parent_index: binary.parent_index,
        }
    }
}

/// Local transform of a bone during animation keyframe evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneTransform {
    pub rotation: Quat,
    pub translation: Vec3,
    pub scale: Vec3,
}

impl Default for BoneTransform {
    fn default() -> Self {
        Self {
            rotation: Quat::IDENTITY,
            translation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

impl BoneTransform {
    pub fn from_rotation_translation(rotation: Quat, translation: Vec3) -> Self {
        Self {
            rotation,
            translation,
            scale: Vec3::ONE,
        }
    }

    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Complete skeletal hierarchy (RefSkeleton) containing bone definitions and hierarchy traversal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    pub bones: Vec<FBone>,
}

impl Skeleton {
    pub fn new() -> Self {
        Self { bones: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bones: Vec::with_capacity(capacity),
        }
    }

    pub fn add_bone(&mut self, bone: FBone) -> usize {
        let idx = self.bones.len();
        self.bones.push(bone);
        idx
    }

    pub fn len(&self) -> usize {
        self.bones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    pub fn find_bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Computes local rest-pose transformation matrices for all bones.
    pub fn compute_local_transforms(&self) -> Vec<Mat4> {
        self.bones.iter().map(|b| b.local_transform_matrix()).collect()
    }

    /// Computes world-space bind pose transformation matrices for all bones by propagating
    /// parent transforms down the skeletal hierarchy.
    pub fn compute_bind_pose_world_transforms(&self) -> Result<Vec<Mat4>, AnimationError> {
        let mut world_transforms = Vec::with_capacity(self.bones.len());

        for (i, bone) in self.bones.iter().enumerate() {
            let local = bone.local_transform_matrix();
            if bone.parent_index < 0 {
                world_transforms.push(local);
            } else {
                let parent_idx = bone.parent_index as usize;
                if parent_idx >= i {
                    return Err(AnimationError::InvalidHierarchy(i));
                }
                let parent_world = world_transforms[parent_idx];
                world_transforms.push(parent_world * local);
            }
        }

        Ok(world_transforms)
    }

    /// Computes the inverse bind pose matrices: $M_{\text{bind}}^{-1} = (M_{\text{bind}})^{-1}$
    /// for every bone in the skeleton. Used directly in the Linear Blend Skinning (LBS) pipeline.
    pub fn compute_inverse_bind_matrices(&self) -> Result<Vec<Mat4>, AnimationError> {
        let world_bind = self.compute_bind_pose_world_transforms()?;
        Ok(world_bind.into_iter().map(|m| m.inverse()).collect())
    }

    /// Evaluates world-space transformation matrices given an array of local animated bone transforms.
    /// Propagates along the topological bone hierarchy.
    pub fn evaluate_world_transforms(
        &self,
        local_transforms: &[BoneTransform],
    ) -> Result<Vec<Mat4>, AnimationError> {
        if local_transforms.len() != self.bones.len() {
            return Err(AnimationError::BoneIndexOutOfBounds {
                index: local_transforms.len(),
                count: self.bones.len(),
            });
        }

        let mut world_transforms = Vec::with_capacity(self.bones.len());

        for (i, bone) in self.bones.iter().enumerate() {
            let local = local_transforms[i].to_mat4();
            if bone.parent_index < 0 {
                world_transforms.push(local);
            } else {
                let parent_idx = bone.parent_index as usize;
                if parent_idx >= i {
                    return Err(AnimationError::InvalidHierarchy(i));
                }
                let parent_world = world_transforms[parent_idx];
                world_transforms.push(parent_world * local);
            }
        }

        Ok(world_transforms)
    }

    /// Computes runtime skinning matrices $M_{\text{skin}, i} = M_{i} \cdot M_{\text{bind}, i}^{-1}$
    /// for all bones, which are uploaded to the GPU StructuredBuffer for compute shader skinning.
    pub fn compute_skinning_matrices(
        &self,
        current_world_transforms: &[Mat4],
        inverse_bind_matrices: &[Mat4],
    ) -> Result<Vec<Mat4>, AnimationError> {
        let count = self.bones.len();
        if current_world_transforms.len() != count || inverse_bind_matrices.len() != count {
            return Err(AnimationError::BoneIndexOutOfBounds {
                index: current_world_transforms.len().min(inverse_bind_matrices.len()),
                count,
            });
        }

        let skinning_matrices = (0..count)
            .map(|i| current_world_transforms[i] * inverse_bind_matrices[i])
            .collect();

        Ok(skinning_matrices)
    }
}
