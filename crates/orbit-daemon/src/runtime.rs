use async_trait::async_trait;
use orbit_domain::{PermissionProfile, ResourceLimits};
use std::{collections::BTreeMap, path::PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePrerequisite {
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    None,
    Bridge,
}
#[derive(Clone)]
pub struct ContainerSpec {
    pub workspace_id: Uuid,
    pub container_name: String,
    pub volume_name: String,
    pub host_path: PathBuf,
    pub profile: PermissionProfile,
    pub resources: ResourceLimits,
    pub webtop_password: String,
    pub image_ref: String,
    pub labels: BTreeMap<String, String>,
}
impl ContainerSpec {
    pub fn workspace_read_only(&self) -> bool {
        matches!(self.profile, PermissionProfile::Observe)
    }
    pub fn network_mode(&self) -> NetworkMode {
        if matches!(self.profile, PermissionProfile::Observe) {
            NetworkMode::None
        } else {
            NetworkMode::Bridge
        }
    }
}
impl std::fmt::Debug for ContainerSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContainerSpec")
            .field("workspace_id", &self.workspace_id)
            .field("container_name", &self.container_name)
            .field("volume_name", &self.volume_name)
            .field("host_path", &self.host_path)
            .field("profile", &self.profile)
            .field("resources", &self.resources)
            .field("webtop_password", &"[REDACTED]")
            .field("image_ref", &self.image_ref)
            .field("labels", &self.labels)
            .finish()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInfo {
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub running: bool,
    pub published_port: Option<u16>,
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub mounts: Vec<MountInfo>,
}
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime unavailable: {0}")]
    Unavailable(String),
    #[error("runtime command failed: {0}")]
    Command(String),
    #[error("invalid runtime output: {0}")]
    InvalidOutput(String),
    #[error("invalid runtime input: {0}")]
    InvalidInput(String),
    #[error("health probe failed: {0}")]
    HealthProbe(String),
}

#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError>;
    async fn ensure_image(&self, image_ref: &str) -> Result<(), RuntimeError>;
    async fn create(&self, spec: &ContainerSpec) -> Result<ContainerInfo, RuntimeError>;
    async fn start(&self, id: &str) -> Result<(), RuntimeError>;
    async fn exec(&self, id: &str, user: &str, args: &[&str]) -> Result<(), RuntimeError>;
    async fn stop(&self, id: &str) -> Result<(), RuntimeError>;
    async fn remove(&self, id: &str) -> Result<(), RuntimeError>;
    async fn remove_volume(&self, name: &str) -> Result<(), RuntimeError>;
    async fn inspect(&self, id: &str) -> Result<Option<ContainerInfo>, RuntimeError>;
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError>;
    async fn healthy(&self, port: u16, user: &str, password: &str) -> Result<bool, RuntimeError>;
}
