use thiserror::Error;

/// Rendering and HAL errors in OpenKF.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RenderError {
    #[error("Device initialization failed: {0}")]
    DeviceInitializationFailed(String),

    #[error("Swapchain error: {0}")]
    SwapchainError(String),

    #[error("Pipeline creation failed for key: {0}")]
    PipelineCreationFailed(String),

    #[error("Shader compilation or loading failed: {0}")]
    ShaderError(String),

    #[error("Buffer allocation failed: {0} bytes")]
    BufferAllocationFailed(usize),

    #[error("Resource allocation failed: {0}")]
    ResourceAllocationFailed(String),

    #[error("Resource not found in cache: {0}")]
    ResourceNotFound(String),

    #[error("Invalid vertex format or attribute alignment: {0}")]
    InvalidVertexFormat(String),

    #[error("Material translation failed: {0}")]
    MaterialTranslationError(String),

    #[error("Descriptor binding error at slot {slot}: {reason}")]
    DescriptorBindingError { slot: u32, reason: String },

    #[error("Serialization error: {0}")]
    SerializationError(String),
}
