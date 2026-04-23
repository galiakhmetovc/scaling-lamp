use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpConnectorTransport {
    Stdio,
}

impl McpConnectorTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConnectorTransportParseError {
    value: String,
}

impl fmt::Display for McpConnectorTransportParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MCP connector transport {}", self.value)
    }
}

impl Error for McpConnectorTransportParseError {}

impl TryFrom<&str> for McpConnectorTransport {
    type Error = McpConnectorTransportParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "stdio" => Ok(Self::Stdio),
            other => Err(McpConnectorTransportParseError {
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpConnectorConfig {
    pub id: String,
    pub transport: McpConnectorTransport,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::McpConnectorTransport;

    #[test]
    fn mcp_connector_transport_round_trips_through_str() {
        assert_eq!(McpConnectorTransport::Stdio.as_str(), "stdio");
        assert_eq!(
            McpConnectorTransport::try_from("stdio").expect("parse stdio"),
            McpConnectorTransport::Stdio
        );
    }

    #[test]
    fn mcp_connector_transport_rejects_unknown_value() {
        let error = McpConnectorTransport::try_from("http").expect_err("invalid transport");
        assert_eq!(error.to_string(), "invalid MCP connector transport http");
    }
}
