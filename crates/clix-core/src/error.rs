use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClixError {
    #[error("capability not found: {0}")]
    CapabilityNotFound(String),
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),
    #[error("input validation failed: {0}")]
    InputValidation(String),
    #[error("policy denied: {0}")]
    Denied(String),
    #[error("approval denied: {0}")]
    ApprovalDenied(String),
    #[error("approval gate error: {0}")]
    ApprovalGate(String),
    #[error("credential resolution failed: {0}")]
    CredentialResolution(String),
    #[error("template render error: {0}")]
    TemplateRender(String),
    #[error("sandbox error: {0}")]
    Sandbox(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("pack error: {0}")]
    Pack(String),
    #[error("schema error: {0}")]
    Schema(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, ClixError>;
