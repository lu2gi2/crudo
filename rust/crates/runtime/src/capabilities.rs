use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    LocalModel,
    LocalRag,
    LocalMcp,
    LocalOcr,
    LocalVlm,
    WebSearch,
    WebFetch,
    RemoteMcp,
    ExternalModel,
    ExternalEmbedding,
    NetworkAccess,
    SubprocessExecution,
    FilesystemAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityMode {
    Sovereign,
    Research,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPolicy {
    pub mode: CapabilityMode,
    pub allow_loopback: bool,
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self::sovereign()
    }
}

impl CapabilityPolicy {
    #[must_use]
    pub const fn sovereign() -> Self {
        Self {
            mode: CapabilityMode::Sovereign,
            allow_loopback: true,
        }
    }

    #[must_use]
    pub const fn research() -> Self {
        Self {
            mode: CapabilityMode::Research,
            allow_loopback: true,
        }
    }

    #[must_use]
    pub const fn allows(self, capability: Capability) -> bool {
        match (self.mode, capability) {
            (_, Capability::LocalModel)
            | (_, Capability::LocalRag)
            | (_, Capability::LocalMcp)
            | (_, Capability::LocalOcr)
            | (_, Capability::LocalVlm)
            | (_, Capability::FilesystemAccess)
            | (_, Capability::SubprocessExecution) => true,
            (CapabilityMode::Research, Capability::WebSearch | Capability::WebFetch) => true,
            _ => false,
        }
    }

    #[must_use]
    pub fn allows_endpoint(&self, endpoint: &str) -> bool {
        if !self.allows(Capability::NetworkAccess) {
            return false;
        }
        endpoint.starts_with("http://127.0.0.1:")
            || endpoint.starts_with("http://localhost:")
            || endpoint.starts_with("http://[::1]:")
            || endpoint.starts_with("https://127.0.0.1:")
            || endpoint.starts_with("https://localhost:")
            || endpoint.starts_with("https://[::1]:")
    }
}

#[cfg(test)]
mod tests {
    use super::{Capability, CapabilityPolicy};

    #[test]
    fn sovereign_denies_external_capabilities() {
        let policy = CapabilityPolicy::sovereign();
        assert!(!policy.allows(Capability::WebSearch));
        assert!(!policy.allows(Capability::NetworkAccess));
        assert!(!policy.allows(Capability::RemoteMcp));
        assert!(policy.allows(Capability::LocalModel));
    }

    #[test]
    fn research_allows_only_public_web_research() {
        let policy = CapabilityPolicy::research();
        assert!(policy.allows(Capability::WebSearch));
        assert!(policy.allows(Capability::WebFetch));
        assert!(!policy.allows(Capability::ExternalModel));
        assert!(!policy.allows(Capability::RemoteMcp));
    }
}
