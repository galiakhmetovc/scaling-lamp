use crate::session::MessageRole;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub name: String,
    pub model_family: String,
    pub default_model: Option<String>,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelCapabilities {
    pub supports_streaming: bool,
    pub supports_text_input: bool,
    pub supports_tool_calls: bool,
    pub supports_reasoning_summaries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub model: Option<String>,
    pub instructions: Option<String>,
    pub messages: Vec<ProviderMessage>,
    pub max_output_tokens: Option<u32>,
    pub stream: ProviderStreamMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    Completed,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    pub response_id: String,
    pub model: String,
    pub output_text: String,
    pub finish_reason: FinishReason,
    pub usage: Option<ProviderUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamEvent {
    TextDelta(String),
    Completed(ProviderResponse),
}

#[derive(Debug)]
pub enum ProviderError {
    Http(reqwest::Error),
    HttpStatus { status: StatusCode, body: String },
    MissingModel,
    Parse(serde_json::Error),
    ResponseMissingOutputText,
    UnsupportedMessageRole { role: MessageRole },
    UnsupportedStreaming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiResponsesConfig {
    pub api_base: String,
    pub api_key: String,
    pub default_model: Option<String>,
}

pub trait ProviderResponseStream: Send {
    fn next_event(&mut self) -> Result<Option<ProviderStreamEvent>, ProviderError>;
}

pub trait ProviderDriver: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;
    fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    fn stream(
        &self,
        request: &ProviderRequest,
    ) -> Result<Box<dyn ProviderResponseStream>, ProviderError>;
}

#[derive(Debug, Clone)]
pub struct OpenAiResponsesDriver {
    client: Client,
    config: OpenAiResponsesConfig,
    descriptor: ProviderDescriptor,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    input: Vec<OpenAiResponsesInputMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    store: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesInputMessage<'a> {
    role: &'a str,
    content: Vec<OpenAiResponsesInputText<'a>>,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesInputText<'a> {
    #[serde(rename = "type")]
    item_type: &'static str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesResponse {
    id: String,
    model: String,
    output: Vec<OpenAiResponseOutputItem>,
    usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseOutputItem {
    #[serde(rename = "type")]
    item_type: String,
    status: Option<String>,
    content: Option<Vec<OpenAiResponseContentItem>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseContentItem {
    #[serde(rename = "type")]
    item_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesUsage {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

impl Default for ProviderDescriptor {
    fn default() -> Self {
        Self {
            name: "unconfigured".to_string(),
            model_family: "none".to_string(),
            default_model: None,
            capabilities: ModelCapabilities::default(),
        }
    }
}

impl ProviderMessage {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

impl OpenAiResponsesConfig {
    fn normalized_api_base(&self) -> &str {
        self.api_base.trim_end_matches('/')
    }
}

impl OpenAiResponsesDriver {
    pub fn new(config: OpenAiResponsesConfig) -> Self {
        Self {
            client: Client::new(),
            descriptor: ProviderDescriptor {
                name: "openai-responses".to_string(),
                model_family: "openai".to_string(),
                default_model: config.default_model.clone(),
                capabilities: ModelCapabilities {
                    supports_streaming: false,
                    supports_text_input: true,
                    supports_tool_calls: false,
                    supports_reasoning_summaries: true,
                },
            },
            config,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/responses", self.config.normalized_api_base())
    }

    fn resolve_model<'a>(&'a self, request: &'a ProviderRequest) -> Result<&'a str, ProviderError> {
        request
            .model
            .as_deref()
            .or(self.config.default_model.as_deref())
            .ok_or(ProviderError::MissingModel)
    }

    fn build_request_body<'a>(
        &'a self,
        request: &'a ProviderRequest,
        model: &'a str,
    ) -> Result<OpenAiResponsesRequest<'a>, ProviderError> {
        let input = request
            .messages
            .iter()
            .map(|message| {
                let role = match message.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => {
                        return Err(ProviderError::UnsupportedMessageRole { role: message.role });
                    }
                };

                Ok(OpenAiResponsesInputMessage {
                    role,
                    content: vec![OpenAiResponsesInputText {
                        item_type: "input_text",
                        text: message.content.as_str(),
                    }],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(OpenAiResponsesRequest {
            model,
            instructions: request.instructions.as_deref(),
            input,
            max_output_tokens: request.max_output_tokens,
            store: false,
        })
    }
}

impl ProviderDriver for OpenAiResponsesDriver {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn complete(&self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let model = self.resolve_model(request)?;
        let body = self.build_request_body(request, model)?;
        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .map_err(ProviderError::Http)?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().map_err(ProviderError::Http)?;
            return Err(ProviderError::HttpStatus { status, body });
        }

        let response = response
            .json::<OpenAiResponsesResponse>()
            .map_err(ProviderError::Http)?;
        let output_text = response
            .output
            .iter()
            .filter(|item| item.item_type == "message")
            .flat_map(|item| item.content.iter().flatten())
            .filter(|item| item.item_type == "output_text")
            .filter_map(|item| item.text.as_deref())
            .collect::<String>();

        if output_text.is_empty() {
            return Err(ProviderError::ResponseMissingOutputText);
        }

        let finish_reason = if response
            .output
            .iter()
            .filter(|item| item.item_type == "message")
            .all(|item| item.status.as_deref() == Some("completed"))
        {
            FinishReason::Completed
        } else {
            FinishReason::Incomplete
        };

        Ok(ProviderResponse {
            response_id: response.id,
            model: response.model,
            output_text,
            finish_reason,
            usage: response.usage.map(|usage| ProviderUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
            }),
        })
    }

    fn stream(
        &self,
        _request: &ProviderRequest,
    ) -> Result<Box<dyn ProviderResponseStream>, ProviderError> {
        Err(ProviderError::UnsupportedStreaming)
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(source) => write!(formatter, "provider http error: {source}"),
            Self::HttpStatus { status, body } => {
                write!(formatter, "provider request failed with {status}: {body}")
            }
            Self::MissingModel => write!(formatter, "provider request is missing a model"),
            Self::Parse(source) => write!(formatter, "provider parse error: {source}"),
            Self::ResponseMissingOutputText => {
                write!(
                    formatter,
                    "provider response did not include assistant text"
                )
            }
            Self::UnsupportedMessageRole { role } => {
                write!(
                    formatter,
                    "provider does not support message role {}",
                    role.as_str()
                )
            }
            Self::UnsupportedStreaming => {
                write!(formatter, "provider streaming is not implemented")
            }
        }
    }
}

impl Error for ProviderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Http(source) => Some(source),
            Self::Parse(source) => Some(source),
            Self::HttpStatus { .. }
            | Self::MissingModel
            | Self::ResponseMissingOutputText
            | Self::UnsupportedMessageRole { .. }
            | Self::UnsupportedStreaming => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FinishReason, ModelCapabilities, OpenAiResponsesConfig, OpenAiResponsesDriver,
        ProviderDriver, ProviderError, ProviderMessage, ProviderRequest, ProviderStreamMode,
    };
    use crate::session::MessageRole;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn driver_descriptor_exposes_stable_openai_capabilities() {
        let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
            api_base: "http://127.0.0.1:9/v1".to_string(),
            api_key: "test-key".to_string(),
            default_model: Some("gpt-5.4".to_string()),
        });

        assert_eq!(driver.descriptor().name, "openai-responses");
        assert_eq!(driver.descriptor().model_family, "openai");
        assert_eq!(
            driver.descriptor().default_model.as_deref(),
            Some("gpt-5.4")
        );
        assert_eq!(
            driver.descriptor().capabilities,
            ModelCapabilities {
                supports_streaming: false,
                supports_text_input: true,
                supports_tool_calls: false,
                supports_reasoning_summaries: true,
            }
        );
    }

    #[test]
    fn complete_posts_responses_payload_and_extracts_output_text() {
        let (api_base, requests, handle) = spawn_json_server(
            r#"{
                "id":"resp_123",
                "model":"gpt-5.4",
                "output":[
                    {"id":"rs_1","type":"reasoning","content":[],"summary":[]},
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"hello world",
                                "annotations":[],
                                "logprobs":[]
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18}
            }"#,
        );
        let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
            api_base,
            api_key: "test-key".to_string(),
            default_model: Some("gpt-5.4".to_string()),
        });
        let request = ProviderRequest {
            model: None,
            instructions: Some("Be brief".to_string()),
            messages: vec![ProviderMessage::new(MessageRole::User, "Write a haiku")],
            max_output_tokens: None,
            stream: ProviderStreamMode::Disabled,
        };

        let response = driver.complete(&request).expect("complete");
        let raw_request = requests.recv().expect("raw request");
        handle.join().expect("join server");

        assert_eq!(response.response_id, "resp_123");
        assert_eq!(response.model, "gpt-5.4");
        assert_eq!(response.output_text, "hello world");
        assert_eq!(response.finish_reason, FinishReason::Completed);
        assert_eq!(response.usage.expect("usage").total_tokens, 18);

        let normalized_request = raw_request.to_ascii_lowercase();
        assert!(normalized_request.contains("post /v1/responses http/1.1"));
        assert!(normalized_request.contains("authorization: bearer test-key"));
        assert!(normalized_request.contains("\"model\":\"gpt-5.4\""));
        assert!(normalized_request.contains("\"instructions\":\"be brief\""));
        assert!(normalized_request.contains("\"store\":false"));
        assert!(normalized_request.contains("\"role\":\"user\""));
        assert!(normalized_request.contains("\"type\":\"input_text\""));
        assert!(normalized_request.contains("\"text\":\"write a haiku\""));
    }

    #[test]
    fn stream_is_an_explicit_contract_even_when_unimplemented() {
        let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
            api_base: "http://127.0.0.1:9/v1".to_string(),
            api_key: "test-key".to_string(),
            default_model: Some("gpt-5.4".to_string()),
        });
        let request = ProviderRequest {
            model: Some("gpt-5.4".to_string()),
            instructions: None,
            messages: vec![ProviderMessage::new(MessageRole::User, "ping")],
            max_output_tokens: None,
            stream: ProviderStreamMode::Enabled,
        };

        assert!(matches!(
            driver.stream(&request),
            Err(ProviderError::UnsupportedStreaming)
        ));
    }

    fn spawn_json_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_bytes = vec![0; content_length];
            reader
                .read_exact(&mut body_bytes)
                .expect("read request body");
            raw_request.push_str(&String::from_utf8_lossy(&body_bytes));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            sender.send(raw_request).expect("send request");
        });

        (format!("http://{address}/v1"), receiver, handle)
    }
}
