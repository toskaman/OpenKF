use thiserror::Error;

#[derive(Error, Debug)]
pub enum AnimationError {
    #[error("Buffer too small: expected at least {expected} bytes, got {got}")]
    BufferTooSmall { expected: usize, got: usize },

    #[error("Invalid magic identifier: expected {expected:?}, got {got:?}")]
    InvalidMagic { expected: [u8; 4], got: [u8; 4] },

    #[error("Invalid format version: expected {expected}, got {got}")]
    InvalidVersion { expected: u32, got: u32 },

    #[error("Bone index {index} out of bounds (skeleton size {count})")]
    BoneIndexOutOfBounds { index: usize, count: usize },

    #[error("Invalid animation sequence: {0}")]
    InvalidSequence(String),

    #[error("Invalid bone hierarchy: loop or invalid parent index detected at bone {0}")]
    InvalidHierarchy(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
