use serde::{Deserialize, Serialize};
use url::Url;

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
        matches!(
            (self.mode, capability),
            (_, Capability::LocalModel)
                | (_, Capability::LocalRag)
                | (_, Capability::LocalMcp)
                | (_, Capability::LocalOcr)
                | (_, Capability::LocalVlm)
                | (_, Capability::FilesystemAccess)
                | (_, Capability::SubprocessExecution)
                | (
                    CapabilityMode::Research,
                    Capability::WebSearch | Capability::WebFetch
                )
        )
    }

    #[must_use]
    pub fn allows_endpoint(&self, endpoint: &str) -> bool {
        self.allows(Capability::NetworkAccess) && is_loopback_url(endpoint)
    }

    /// Return whether a discovered MCP tool may execute under this policy.
    ///
    /// OCR is a local capability even when it is exposed through MCP. Keeping
    /// this mapping here prevents a local MCP transport from becoming an
    /// accidental bypass for capability policy.
    #[must_use]
    pub fn allows_mcp_tool(self, tool_name: &str) -> bool {
        if tool_name == "ocr_document" || tool_name.ends_with("/ocr_document") {
            return self.allows(Capability::LocalOcr);
        }
        self.allows(Capability::LocalMcp)
    }

    pub fn allows_mcp_transport(
        &self,
        transport: crate::config::McpTransport,
        endpoint: Option<&str>,
    ) -> bool {
        match transport {
            crate::config::McpTransport::Stdio | crate::config::McpTransport::Sdk => {
                self.allows(Capability::LocalMcp)
            }
            _ => {
                self.allows(Capability::RemoteMcp)
                    && endpoint.is_some_and(|url| self.allows_endpoint(url))
            }
        }
    }
}

#[must_use]
pub fn is_loopback_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value.trim()) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_none()
    {
        return false;
    }
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
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
    fn local_ocr_mcp_tool_is_allowed_by_both_modes() {
        assert!(CapabilityPolicy::sovereign().allows_mcp_tool("ocr_document"));
        assert!(CapabilityPolicy::research().allows_mcp_tool("industrial/ocr_document"));
    }

    #[test]
    fn unknown_mcp_tool_requires_local_mcp_capability() {
        assert!(CapabilityPolicy::sovereign().allows_mcp_tool("search_knowledge"));
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
