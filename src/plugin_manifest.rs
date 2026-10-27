//! Versioned, default-deny plugin manifest. It grants application commands,
//! not WASI filesystem or network access.
use crate::plugin_permissions::{Origin, Rule};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use thiserror::Error;

pub const MANIFEST_VERSION: u32 = 1;
pub const WIT_VERSION: &str = "0.2.0";
pub const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_COMPONENT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Compiled,
    Interpreter,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// Local and same-user automation are origin classes, not individual agents.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub automation: bool,
    #[serde(default)]
    pub local_source: bool,
    #[serde(default)]
    pub automation_source: bool,
}
impl Invocation {
    pub fn allows(&self, origin: Origin, source: bool) -> bool {
        match (origin, source) {
            (Origin::Local, false) => self.local,
            (Origin::Local, true) => self.local && self.local_source,
            (_, false) => self.automation,
            (_, true) => self.automation && self.automation_source,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_calls: u32,
    pub max_host_bytes: u64,
    pub timeout_ms: u64,
    pub max_downstream_jobs: u32,
}
impl Limits {
    pub const HOST: Self = Self {
        max_calls: 128,
        max_host_bytes: 1024 * 1024,
        timeout_ms: 10_000,
        max_downstream_jobs: 32,
    };
    pub fn capped(self) -> Self {
        Self {
            max_calls: self.max_calls.min(Self::HOST.max_calls),
            max_host_bytes: self.max_host_bytes.min(Self::HOST.max_host_bytes),
            timeout_ms: self.timeout_ms.min(Self::HOST.timeout_ms),
            max_downstream_jobs: self.max_downstream_jobs.min(Self::HOST.max_downstream_jobs),
        }
    }
    fn valid(self) -> bool {
        self.max_calls > 0
            && self.max_host_bytes > 0
            && self.timeout_ms > 0
            && self.max_downstream_jobs > 0
    }
}
impl Default for Limits {
    fn default() -> Self {
        Self::HOST
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub manifest_version: u32,
    pub id: String,
    pub version: String,
    pub kind: Kind,
    #[serde(default)]
    pub languages: Vec<String>,
    pub wit_version: String,
    pub component: String,
    pub component_sha256: String,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub invocation: Invocation,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("invalid manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

/// IDs are local registry keys, not paths or client identities.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_'))
}
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(ManifestError::Invalid("manifest too large".into()));
        }
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<(), ManifestError> {
        let invalid = |s: &str| ManifestError::Invalid(s.into());
        if self.manifest_version != MANIFEST_VERSION {
            return Err(invalid("unsupported manifest version"));
        }
        if self.wit_version != WIT_VERSION {
            return Err(invalid("unsupported WIT version"));
        }
        if !valid_id(&self.id) {
            return Err(invalid("invalid plugin ID"));
        }
        if self.version.is_empty()
            || self.version.len() > 128
            || self.version.chars().any(char::is_control)
        {
            return Err(invalid("invalid plugin version"));
        }
        let path = Path::new(&self.component);
        if path.as_os_str().is_empty()
            || !path.components().all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(invalid("component must be bundle-relative"));
        }
        if self.component_sha256.len() != 64
            || !self
                .component_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(invalid("component SHA-256 must be lowercase hexadecimal"));
        }
        if self.languages.len() > 32
            || self
                .languages
                .iter()
                .any(|l| l.is_empty() || l.len() > 64 || l.chars().any(char::is_control))
        {
            return Err(invalid("invalid language metadata"));
        }
        if self.kind == Kind::Interpreter && self.languages.is_empty() {
            return Err(invalid("interpreter must name at least one language"));
        }
        if self.kind == Kind::Compiled
            && (!self.languages.is_empty()
                || self.invocation.local_source
                || self.invocation.automation_source)
        {
            return Err(invalid("compiled plugins do not accept script sources"));
        }
        if !self.limits.valid() {
            return Err(invalid("limits must be positive"));
        }
        if self.permissions.rules.len() > 512 {
            return Err(invalid("too many permission rules"));
        }
        for rule in &self.permissions.rules {
            rule.validate().map_err(|e| invalid(&e.to_string()))?;
        }
        Ok(())
    }
}
