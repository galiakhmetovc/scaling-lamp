use crate::session::MessageRole;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub name: String,
    pub model_family: String,
    pub default_model: Option<String>,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[serde(alias = "openai_responses")]
    #[default]
    OpenAiResponses,
    ZaiChatCompletions,
}

pub const DEFAULT_PROVIDER_MAX_TOOL_ROUNDS: u32 = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfiguredProvider {
    pub kind: ProviderKind,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub request_timeout_seconds: Option<u64>,
    pub stream_idle_timeout_seconds: Option<u64>,
    pub max_tool_rounds: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl Default for ConfiguredProvider {
    fn default() -> Self {
        Self {
            kind: ProviderKind::default(),
            api_base: None,
            api_key: None,
            default_model: None,
            connect_timeout_seconds: Some(15),
            request_timeout_seconds: None,
            stream_idle_timeout_seconds: Some(1200),
            max_tool_rounds: Some(DEFAULT_PROVIDER_MAX_TOOL_ROUNDS),
            max_output_tokens: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelCapabilities {
    pub supports_streaming: bool,
    pub supports_text_input: bool,
    pub supports_tool_calls: bool,
    pub supports_previous_response_id: bool,
    pub supports_reasoning_summaries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStreamMode {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub model: Option<String>,
    pub instructions: Option<String>,
    pub messages: Vec<ProviderMessage>,
    pub think_level: Option<String>,
    pub previous_response_id: Option<String>,
    pub continuation_messages: Vec<ProviderContinuationMessage>,
    pub tools: Vec<ProviderToolDefinition>,
    pub tool_outputs: Vec<ProviderToolOutput>,
    pub max_output_tokens: Option<u32>,
    pub stream: ProviderStreamMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolOutput {
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderContinuationMessage {
    AssistantToolCalls {
        tool_calls: Vec<ProviderToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub tool_calls: Vec<ProviderToolCall>,
    pub finish_reason: FinishReason,
    pub usage: Option<ProviderUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamEvent {
    ReasoningDelta(String),
    TextDelta(String),
    Completed(ProviderResponse),
}

#[derive(Debug)]
pub enum ProviderError {
    Http(reqwest::Error),
    HttpStatus { status: StatusCode, body: String },
    MissingApiBase,
    MissingApiKey,
    MissingModel,
    Parse(serde_json::Error),
    Stream(std::io::Error),
    StreamIdleTimeout { seconds: u64 },
    ResponseMissingOutputText,
    ResponseMissingToolCallField { field: &'static str },
    UnsupportedMessageRole { role: MessageRole },
    UnsupportedStreaming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderKindParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderBuildError {
    MissingApiBase,
    MissingApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiResponsesConfig {
    pub api_base: String,
    pub api_key: String,
    pub default_model: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub request_timeout_seconds: Option<u64>,
    pub stream_idle_timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaiChatCompletionsConfig {
    pub api_base: String,
    pub api_key: String,
    pub default_model: Option<String>,
    pub connect_timeout_seconds: Option<u64>,
    pub request_timeout_seconds: Option<u64>,
    pub stream_idle_timeout_seconds: Option<u64>,
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

#[derive(Debug, Clone)]
pub struct ZaiChatCompletionsDriver {
    client: Client,
    config: ZaiChatCompletionsConfig,
    descriptor: ProviderDescriptor,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_response_id: Option<&'a str>,
    input: Vec<OpenAiResponsesInputItem<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<OpenAiResponsesReasoningConfig<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiResponsesToolDefinition<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    stream: bool,
    store: bool,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesReasoningConfig<'a> {
    summary: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiResponsesInputItem<'a> {
    Message(OpenAiResponsesInputMessage<'a>),
    FunctionCallOutput(OpenAiResponsesFunctionCallOutput<'a>),
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

#[derive(Debug, Serialize)]
struct OpenAiResponsesFunctionCallOutput<'a> {
    #[serde(rename = "type")]
    item_type: &'static str,
    call_id: &'a str,
    output: &'a str,
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesToolDefinition<'a> {
    #[serde(rename = "type")]
    item_type: &'static str,
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
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
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
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

#[derive(Debug, Serialize)]
struct ZaiChatCompletionsRequest<'a> {
    model: &'a str,
    messages: Vec<ZaiChatCompletionMessage<'a>>,
    thinking: ZaiThinkingConfig<'static>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ZaiChatCompletionToolDefinition<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ZaiThinkingConfig<'a> {
    #[serde(rename = "type")]
    thinking_type: &'a str,
}

#[derive(Debug, Serialize)]
struct ZaiChatCompletionMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ZaiChatCompletionToolCall<'a>>>,
}

#[derive(Debug, Serialize)]
struct ZaiChatCompletionToolDefinition<'a> {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: ZaiChatCompletionFunctionDefinition<'a>,
}

#[derive(Debug, Serialize)]
struct ZaiChatCompletionFunctionDefinition<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Debug, Serialize)]
struct ZaiChatCompletionToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: ZaiChatCompletionToolCallFunction<'a>,
}

#[derive(Debug, Serialize)]
struct ZaiChatCompletionToolCallFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionsResponse {
    id: String,
    model: String,
    choices: Vec<ZaiChatCompletionChoice>,
    usage: Option<ZaiChatCompletionsUsage>,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionChoice {
    finish_reason: Option<String>,
    message: ZaiChatCompletionResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ZaiChatCompletionResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionResponseToolCall {
    id: String,
    #[serde(rename = "type")]
    _tool_type: String,
    function: ZaiChatCompletionResponseToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionResponseToolCallFunction {
    name: String,
    arguments: ZaiToolCallArguments,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ZaiToolCallArguments {
    String(String),
    Json(Value),
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionsUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionsStreamChunk {
    id: Option<String>,
    model: Option<String>,
    choices: Vec<ZaiChatCompletionsStreamChoice>,
    usage: Option<ZaiChatCompletionsUsage>,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionsStreamChoice {
    finish_reason: Option<String>,
    delta: ZaiChatCompletionsStreamDelta,
}

#[derive(Debug, Deserialize, Default)]
struct ZaiChatCompletionsStreamDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ZaiChatCompletionsStreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ZaiChatCompletionsStreamToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<ZaiChatCompletionsStreamToolCallFunctionDelta>,
}

#[derive(Debug, Deserialize, Default)]
struct ZaiChatCompletionsStreamToolCallFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct ZaiAccumulatedToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug)]
struct ZaiChatCompletionsResponseStream {
    line_reader: TimedSseLineReader,
    pending_events: VecDeque<ProviderStreamEvent>,
    response_id: Option<String>,
    model: Option<String>,
    output_text: String,
    tool_calls: Vec<ZaiAccumulatedToolCall>,
    finish_reason: FinishReason,
    usage: Option<ProviderUsage>,
    done: bool,
}

#[derive(Debug)]
struct OpenAiResponsesResponseStream {
    line_reader: TimedSseLineReader,
    pending_events: VecDeque<ProviderStreamEvent>,
    done: bool,
}

#[derive(Debug)]
struct TimedSseLineReader {
    receiver: Receiver<Result<Option<String>, std::io::Error>>,
    idle_timeout: Option<Duration>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<String>,
    response: Option<OpenAiResponsesResponse>,
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

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai_responses",
            Self::ZaiChatCompletions => "zai_chat_completions",
        }
    }
}

impl TryFrom<&str> for ProviderKind {
    type Error = ProviderKindParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "openai_responses" => Ok(Self::OpenAiResponses),
            "zai_chat_completions" => Ok(Self::ZaiChatCompletions),
            _ => Err(ProviderKindParseError {
                value: value.to_string(),
            }),
        }
    }
}

pub fn build_driver(
    provider: &ConfiguredProvider,
) -> Result<Box<dyn ProviderDriver>, ProviderBuildError> {
    let api_base = provider
        .api_base
        .clone()
        .ok_or(ProviderBuildError::MissingApiBase)?;
    let api_key = provider
        .api_key
        .clone()
        .ok_or(ProviderBuildError::MissingApiKey)?;

    match provider.kind {
        ProviderKind::OpenAiResponses => Ok(Box::new(OpenAiResponsesDriver::new(
            OpenAiResponsesConfig {
                api_base,
                api_key,
                default_model: provider.default_model.clone(),
                connect_timeout_seconds: provider.connect_timeout_seconds,
                request_timeout_seconds: provider.request_timeout_seconds,
                stream_idle_timeout_seconds: provider.stream_idle_timeout_seconds,
            },
        ))),
        ProviderKind::ZaiChatCompletions => Ok(Box::new(ZaiChatCompletionsDriver::new(
            ZaiChatCompletionsConfig {
                api_base,
                api_key,
                default_model: provider.default_model.clone(),
                connect_timeout_seconds: provider.connect_timeout_seconds,
                request_timeout_seconds: provider.request_timeout_seconds,
                stream_idle_timeout_seconds: provider.stream_idle_timeout_seconds,
            },
        ))),
    }
}

pub fn render_http_request_preview(
    provider: &ConfiguredProvider,
    request: &ProviderRequest,
) -> Result<String, ProviderError> {
    let api_base = provider
        .api_base
        .clone()
        .ok_or(ProviderError::MissingApiBase)?;
    let api_key = provider
        .api_key
        .clone()
        .ok_or(ProviderError::MissingApiKey)?;

    match provider.kind {
        ProviderKind::OpenAiResponses => {
            let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
                api_base,
                api_key: api_key.clone(),
                default_model: provider.default_model.clone(),
                connect_timeout_seconds: provider.connect_timeout_seconds,
                request_timeout_seconds: provider.request_timeout_seconds,
                stream_idle_timeout_seconds: provider.stream_idle_timeout_seconds,
            });
            let model = driver.resolve_model(request)?;
            let body = driver.build_request_body(request, model)?;
            let rendered_body =
                serde_json::to_string_pretty(&body).map_err(ProviderError::Parse)?;
            Ok(render_http_preview_text(
                driver.endpoint(),
                request.stream,
                api_key.as_str(),
                rendered_body,
            ))
        }
        ProviderKind::ZaiChatCompletions => {
            let driver = ZaiChatCompletionsDriver::new(ZaiChatCompletionsConfig {
                api_base,
                api_key: api_key.clone(),
                default_model: provider.default_model.clone(),
                connect_timeout_seconds: provider.connect_timeout_seconds,
                request_timeout_seconds: provider.request_timeout_seconds,
                stream_idle_timeout_seconds: provider.stream_idle_timeout_seconds,
            });
            let model = driver.resolve_model(request)?;
            let body = driver.build_request_body(request, model)?;
            let rendered_body =
                serde_json::to_string_pretty(&body).map_err(ProviderError::Parse)?;
            Ok(render_http_preview_text(
                driver.endpoint(),
                request.stream,
                api_key.as_str(),
                rendered_body,
            ))
        }
    }
}

fn render_http_preview_text(
    endpoint: String,
    stream: ProviderStreamMode,
    api_key: &str,
    rendered_body: String,
) -> String {
    let accept = if stream == ProviderStreamMode::Enabled {
        "text/event-stream"
    } else {
        "application/json"
    };
    [
        format!("POST {endpoint}"),
        format!("Authorization: Bearer {}", redact_api_key(api_key)),
        "Content-Type: application/json".to_string(),
        format!("Accept: {accept}"),
        String::new(),
        rendered_body,
    ]
    .join("\n")
}

fn redact_api_key(api_key: &str) -> String {
    if api_key.chars().count() <= 8 {
        return "<redacted>".to_string();
    }
    let prefix = api_key.chars().take(4).collect::<String>();
    let suffix = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}…{suffix}")
}

impl OpenAiResponsesConfig {
    fn normalized_api_base(&self) -> &str {
        self.api_base.trim_end_matches('/')
    }
}

impl ZaiChatCompletionsConfig {
    fn normalized_api_base(&self) -> &str {
        self.api_base.trim_end_matches('/')
    }
}

impl OpenAiResponsesDriver {
    pub fn new(config: OpenAiResponsesConfig) -> Self {
        Self {
            client: build_http_client(
                config.connect_timeout_seconds,
                config.request_timeout_seconds,
            ),
            descriptor: ProviderDescriptor {
                name: "openai-responses".to_string(),
                model_family: "openai".to_string(),
                default_model: config.default_model.clone(),
                capabilities: ModelCapabilities {
                    supports_streaming: true,
                    supports_text_input: true,
                    supports_tool_calls: true,
                    supports_previous_response_id: true,
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
        let mut input = request
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

                Ok(OpenAiResponsesInputItem::Message(
                    OpenAiResponsesInputMessage {
                        role,
                        content: vec![OpenAiResponsesInputText {
                            item_type: "input_text",
                            text: message.content.as_str(),
                        }],
                    },
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        input.extend(request.tool_outputs.iter().map(|output| {
            OpenAiResponsesInputItem::FunctionCallOutput(OpenAiResponsesFunctionCallOutput {
                item_type: "function_call_output",
                call_id: output.call_id.as_str(),
                output: output.output.as_str(),
            })
        }));

        let tools = request
            .tools
            .iter()
            .map(|tool| OpenAiResponsesToolDefinition {
                item_type: "function",
                name: tool.name.as_str(),
                description: tool.description.as_str(),
                parameters: &tool.parameters,
            })
            .collect();

        Ok(OpenAiResponsesRequest {
            model,
            instructions: request.instructions.as_deref(),
            previous_response_id: request.previous_response_id.as_deref(),
            input,
            reasoning: (request.stream == ProviderStreamMode::Enabled
                && !think_level_disables_reasoning(request.think_level.as_deref()))
            .then_some(OpenAiResponsesReasoningConfig { summary: "auto" }),
            tools,
            max_output_tokens: request.max_output_tokens,
            stream: request.stream == ProviderStreamMode::Enabled,
            store: false,
        })
    }
}

impl ZaiChatCompletionsDriver {
    pub fn new(config: ZaiChatCompletionsConfig) -> Self {
        Self {
            client: build_http_client(
                config.connect_timeout_seconds,
                config.request_timeout_seconds,
            ),
            descriptor: ProviderDescriptor {
                name: "zai-chat-completions".to_string(),
                model_family: "zai".to_string(),
                default_model: config.default_model.clone(),
                capabilities: ModelCapabilities {
                    supports_streaming: true,
                    supports_text_input: true,
                    supports_tool_calls: true,
                    supports_previous_response_id: false,
                    supports_reasoning_summaries: false,
                },
            },
            config,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.config.normalized_api_base())
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
    ) -> Result<ZaiChatCompletionsRequest<'a>, ProviderError> {
        let mut messages = Vec::new();
        let tools = request
            .tools
            .iter()
            .map(|tool| ZaiChatCompletionToolDefinition {
                tool_type: "function",
                function: ZaiChatCompletionFunctionDefinition {
                    name: tool.name.as_str(),
                    description: tool.description.as_str(),
                    parameters: &tool.parameters,
                },
            })
            .collect::<Vec<_>>();

        if let Some(instructions) = request.instructions.as_deref() {
            messages.push(ZaiChatCompletionMessage {
                role: "system",
                content: Some(instructions),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        for message in &request.messages {
            let role = match message.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => {
                    return Err(ProviderError::UnsupportedMessageRole { role: message.role });
                }
            };
            messages.push(ZaiChatCompletionMessage {
                role,
                content: Some(message.content.as_str()),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        for message in &request.continuation_messages {
            match message {
                ProviderContinuationMessage::AssistantToolCalls { tool_calls } => {
                    messages.push(ZaiChatCompletionMessage {
                        role: "assistant",
                        content: None,
                        tool_call_id: None,
                        tool_calls: Some(
                            tool_calls
                                .iter()
                                .map(|tool_call| ZaiChatCompletionToolCall {
                                    id: tool_call.call_id.as_str(),
                                    tool_type: "function",
                                    function: ZaiChatCompletionToolCallFunction {
                                        name: tool_call.name.as_str(),
                                        arguments: tool_call.arguments.as_str(),
                                    },
                                })
                                .collect(),
                        ),
                    });
                }
                ProviderContinuationMessage::ToolResult {
                    tool_call_id,
                    content,
                } => {
                    messages.push(ZaiChatCompletionMessage {
                        role: "tool",
                        content: Some(content.as_str()),
                        tool_call_id: Some(tool_call_id.as_str()),
                        tool_calls: None,
                    });
                }
            }
        }

        Ok(ZaiChatCompletionsRequest {
            model,
            messages,
            thinking: ZaiThinkingConfig {
                thinking_type: if request.stream == ProviderStreamMode::Enabled
                    && !think_level_disables_reasoning(request.think_level.as_deref())
                {
                    "enabled"
                } else {
                    "disabled"
                },
            },
            tools,
            tool_choice: (!request.tools.is_empty()).then_some("auto"),
            tool_stream: (request.stream == ProviderStreamMode::Enabled
                && !request.tools.is_empty())
            .then_some(true),
            max_tokens: request.max_output_tokens,
            stream: request.stream == ProviderStreamMode::Enabled,
        })
    }
}

fn think_level_disables_reasoning(think_level: Option<&str>) -> bool {
    matches!(
        think_level.map(str::trim).filter(|value| !value.is_empty()),
        Some(value)
            if value.eq_ignore_ascii_case("off")
                || value.eq_ignore_ascii_case("disabled")
                || value.eq_ignore_ascii_case("none")
    )
}

fn build_http_client(
    connect_timeout_seconds: Option<u64>,
    request_timeout_seconds: Option<u64>,
) -> Client {
    let mut builder = Client::builder();

    if let Some(seconds) = connect_timeout_seconds {
        builder = builder.connect_timeout(Duration::from_secs(seconds));
    }
    builder = match request_timeout_seconds {
        Some(seconds) => builder.timeout(Duration::from_secs(seconds)),
        None => builder.timeout(None::<Duration>),
    };

    builder
        .build()
        .expect("provider http client configuration should be valid")
}

impl TimedSseLineReader {
    fn new(response: reqwest::blocking::Response, idle_timeout_seconds: Option<u64>) -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(response);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = sender.send(Ok(None));
                        break;
                    }
                    Ok(_) => {
                        if sender.send(Ok(Some(line))).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });

        Self {
            receiver,
            idle_timeout: idle_timeout_seconds.map(Duration::from_secs),
        }
    }

    fn next_line(&mut self) -> Result<Option<String>, ProviderError> {
        match self.idle_timeout {
            Some(duration) => match self.receiver.recv_timeout(duration) {
                Ok(result) => result.map_err(ProviderError::Stream),
                Err(RecvTimeoutError::Timeout) => Err(ProviderError::StreamIdleTimeout {
                    seconds: duration.as_secs(),
                }),
                Err(RecvTimeoutError::Disconnected) => Ok(None),
            },
            None => self
                .receiver
                .recv()
                .map_err(|_| {
                    ProviderError::Stream(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
                })?
                .map_err(ProviderError::Stream),
        }
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
        openai_provider_response_from_response(response)
    }

    fn stream(
        &self,
        request: &ProviderRequest,
    ) -> Result<Box<dyn ProviderResponseStream>, ProviderError> {
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

        Ok(Box::new(OpenAiResponsesResponseStream {
            line_reader: TimedSseLineReader::new(response, self.config.stream_idle_timeout_seconds),
            pending_events: VecDeque::new(),
            done: false,
        }))
    }
}

impl ZaiChatCompletionsResponseStream {
    fn finalize_response(&mut self) -> Result<Option<ProviderResponse>, ProviderError> {
        if self.done {
            return Ok(None);
        }
        self.done = true;

        let Some(response_id) = self.response_id.clone() else {
            return Ok(None);
        };
        let Some(model) = self.model.clone() else {
            return Ok(None);
        };
        let tool_calls = self
            .tool_calls
            .iter()
            .map(|tool_call| ProviderToolCall {
                call_id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
            })
            .collect::<Vec<_>>();
        ensure_response_has_primary_output(self.output_text.as_str(), &tool_calls)?;

        Ok(Some(ProviderResponse {
            response_id,
            model,
            output_text: self.output_text.clone(),
            tool_calls,
            finish_reason: self.finish_reason.clone(),
            usage: self.usage.clone(),
        }))
    }

    fn parse_next_sse_payload(&mut self) -> Result<Option<String>, ProviderError> {
        let mut payload_lines = Vec::new();

        loop {
            let Some(line) = self.line_reader.next_line()? else {
                if payload_lines.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(payload_lines.join("\n")));
            };

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if payload_lines.is_empty() {
                    continue;
                }
                return Ok(Some(payload_lines.join("\n")));
            }

            if let Some(data) = trimmed.strip_prefix("data:") {
                payload_lines.push(data.trim_start().to_string());
            }
        }
    }

    fn apply_chunk(&mut self, chunk: ZaiChatCompletionsStreamChunk) -> Result<(), ProviderError> {
        if let Some(response_id) = chunk.id {
            self.response_id = Some(response_id);
        }
        if let Some(model) = chunk.model {
            self.model = Some(model);
        }
        if let Some(usage) = chunk.usage {
            self.usage = Some(ProviderUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            });
        }

        for choice in chunk.choices {
            if let Some(content) = choice.delta.content
                && !content.is_empty()
            {
                self.output_text.push_str(&content);
                self.pending_events
                    .push_back(ProviderStreamEvent::TextDelta(content));
            }
            if let Some(reasoning) = choice.delta.reasoning_content {
                self.pending_events
                    .push_back(ProviderStreamEvent::ReasoningDelta(reasoning));
            }

            if let Some(tool_calls) = choice.delta.tool_calls {
                for tool_call in tool_calls {
                    while self.tool_calls.len() <= tool_call.index {
                        self.tool_calls.push(ZaiAccumulatedToolCall::default());
                    }
                    let entry = &mut self.tool_calls[tool_call.index];
                    if let Some(id) = tool_call.id {
                        entry.id = id;
                    }
                    if let Some(function) = tool_call.function {
                        if let Some(name) = function.name {
                            entry.name = name;
                        }
                        if let Some(arguments) = function.arguments {
                            entry.arguments.push_str(&arguments);
                        }
                    }
                }
            }

            if let Some(finish_reason) = choice.finish_reason.as_deref() {
                self.finish_reason = match finish_reason {
                    "stop" => FinishReason::Completed,
                    _ => FinishReason::Incomplete,
                };
                if let Some(response) = self.finalize_response()? {
                    self.pending_events
                        .push_back(ProviderStreamEvent::Completed(response));
                }
            }
        }

        Ok(())
    }
}

impl OpenAiResponsesResponseStream {
    fn parse_next_sse_payload(&mut self) -> Result<Option<String>, ProviderError> {
        let mut payload_lines = Vec::new();

        loop {
            let Some(line) = self.line_reader.next_line()? else {
                if payload_lines.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(payload_lines.join("\n")));
            };

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if payload_lines.is_empty() {
                    continue;
                }
                return Ok(Some(payload_lines.join("\n")));
            }

            if let Some(data) = trimmed.strip_prefix("data:") {
                payload_lines.push(data.trim_start().to_string());
            }
        }
    }

    fn apply_event(&mut self, event: OpenAiResponsesStreamEvent) -> Result<(), ProviderError> {
        match event.event_type.as_str() {
            "response.output_text.delta" => {
                if let Some(delta) = event.delta {
                    self.pending_events
                        .push_back(ProviderStreamEvent::TextDelta(delta));
                }
            }
            "response.reasoning_summary_text.delta" => {
                if let Some(delta) = event.delta {
                    self.pending_events
                        .push_back(ProviderStreamEvent::ReasoningDelta(delta));
                }
            }
            "response.completed" | "response.incomplete" => {
                let response = event
                    .response
                    .ok_or_else(|| ProviderError::ResponseMissingOutputText)?;
                self.pending_events
                    .push_back(ProviderStreamEvent::Completed(
                        openai_provider_response_from_response(response)?,
                    ));
                self.done = true;
            }
            _ => {}
        }
        Ok(())
    }
}

impl ProviderResponseStream for OpenAiResponsesResponseStream {
    fn next_event(&mut self) -> Result<Option<ProviderStreamEvent>, ProviderError> {
        loop {
            if let Some(event) = self.pending_events.pop_front() {
                return Ok(Some(event));
            }
            if self.done {
                return Ok(None);
            }

            let Some(payload) = self.parse_next_sse_payload()? else {
                return Ok(None);
            };
            if payload == "[DONE]" {
                self.done = true;
                return Ok(None);
            }

            let event = serde_json::from_str::<OpenAiResponsesStreamEvent>(&payload)
                .map_err(ProviderError::Parse)?;
            self.apply_event(event)?;
        }
    }
}

impl ProviderResponseStream for ZaiChatCompletionsResponseStream {
    fn next_event(&mut self) -> Result<Option<ProviderStreamEvent>, ProviderError> {
        loop {
            if let Some(event) = self.pending_events.pop_front() {
                return Ok(Some(event));
            }
            if self.done {
                return Ok(None);
            }

            let Some(payload) = self.parse_next_sse_payload()? else {
                if let Some(response) = self.finalize_response()? {
                    return Ok(Some(ProviderStreamEvent::Completed(response)));
                }
                return Ok(None);
            };

            if payload == "[DONE]" {
                if let Some(response) = self.finalize_response()? {
                    return Ok(Some(ProviderStreamEvent::Completed(response)));
                }
                return Ok(None);
            }

            let chunk = serde_json::from_str::<ZaiChatCompletionsStreamChunk>(&payload)
                .map_err(ProviderError::Parse)?;
            self.apply_chunk(chunk)?;
        }
    }
}

impl ProviderDriver for ZaiChatCompletionsDriver {
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
            .json::<ZaiChatCompletionsResponse>()
            .map_err(ProviderError::Http)?;
        let output_text = response
            .choices
            .iter()
            .filter_map(|choice| choice.message.content.as_deref())
            .collect::<String>();
        let tool_calls = response
            .choices
            .iter()
            .flat_map(|choice| choice.message.tool_calls.iter().flatten())
            .map(|tool_call| {
                Ok(ProviderToolCall {
                    call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    arguments: match &tool_call.function.arguments {
                        ZaiToolCallArguments::Json(value) => {
                            serde_json::to_string(value).map_err(ProviderError::Parse)?
                        }
                        ZaiToolCallArguments::String(value) => value.clone(),
                    },
                })
            })
            .collect::<Result<Vec<_>, ProviderError>>()?;

        ensure_response_has_primary_output(output_text.as_str(), &tool_calls)?;

        let finish_reason = if response
            .choices
            .iter()
            .all(|choice| choice.finish_reason.as_deref() == Some("stop"))
        {
            FinishReason::Completed
        } else {
            FinishReason::Incomplete
        };

        Ok(ProviderResponse {
            response_id: response.id,
            model: response.model,
            output_text,
            tool_calls,
            finish_reason,
            usage: response.usage.map(|usage| ProviderUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            }),
        })
    }

    fn stream(
        &self,
        request: &ProviderRequest,
    ) -> Result<Box<dyn ProviderResponseStream>, ProviderError> {
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

        Ok(Box::new(ZaiChatCompletionsResponseStream {
            line_reader: TimedSseLineReader::new(response, self.config.stream_idle_timeout_seconds),
            pending_events: VecDeque::new(),
            response_id: None,
            model: None,
            output_text: String::new(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Incomplete,
            usage: None,
            done: false,
        }))
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(source) => write!(formatter, "provider http error: {source}"),
            Self::HttpStatus { status, body } => {
                write!(formatter, "provider request failed with {status}: {body}")
            }
            Self::MissingApiBase => write!(formatter, "provider config is missing api_base"),
            Self::MissingApiKey => write!(formatter, "provider config is missing api_key"),
            Self::MissingModel => write!(formatter, "provider request is missing a model"),
            Self::Parse(source) => write!(formatter, "provider parse error: {source}"),
            Self::Stream(source) => write!(formatter, "provider stream error: {source}"),
            Self::StreamIdleTimeout { seconds } => {
                write!(
                    formatter,
                    "provider stream idle timeout after {} seconds without new bytes",
                    seconds
                )
            }
            Self::ResponseMissingOutputText => {
                write!(
                    formatter,
                    "provider response did not include assistant text"
                )
            }
            Self::ResponseMissingToolCallField { field } => {
                write!(formatter, "provider function call response missing {field}")
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
            Self::Stream(source) => Some(source),
            Self::HttpStatus { .. }
            | Self::MissingApiBase
            | Self::MissingApiKey
            | Self::MissingModel
            | Self::StreamIdleTimeout { .. }
            | Self::ResponseMissingOutputText
            | Self::ResponseMissingToolCallField { .. }
            | Self::UnsupportedMessageRole { .. }
            | Self::UnsupportedStreaming => None,
        }
    }
}

impl ProviderError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Http(_)
            | Self::Stream(_)
            | Self::StreamIdleTimeout { .. }
            | Self::ResponseMissingOutputText => true,
            Self::HttpStatus { status, .. } => {
                status.is_server_error()
                    || *status == StatusCode::TOO_MANY_REQUESTS
                    || *status == StatusCode::REQUEST_TIMEOUT
            }
            Self::MissingApiBase
            | Self::MissingApiKey
            | Self::MissingModel
            | Self::Parse(_)
            | Self::ResponseMissingToolCallField { .. }
            | Self::UnsupportedMessageRole { .. }
            | Self::UnsupportedStreaming => false,
        }
    }

    pub fn approval_summary(&self) -> String {
        match self {
            Self::HttpStatus { status, body } => {
                let body = body.trim();
                if body.is_empty() {
                    format!("provider request failed with {status}")
                } else {
                    format!("provider request failed with {status}: {body}")
                }
            }
            Self::Http(source) => format!("provider http error: {source}"),
            Self::Stream(source) => format!("provider stream error: {source}"),
            Self::StreamIdleTimeout { seconds } => format!(
                "provider stream idle timeout after {} seconds without new bytes",
                seconds
            ),
            _ => self.to_string(),
        }
    }
}

impl fmt::Display for ProviderKindParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown provider kind {}", self.value)
    }
}

impl Error for ProviderKindParseError {}

impl fmt::Display for ProviderBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiBase => write!(formatter, "provider config is missing api_base"),
            Self::MissingApiKey => write!(formatter, "provider config is missing api_key"),
        }
    }
}

impl Error for ProviderBuildError {}

fn openai_provider_response_from_response(
    response: OpenAiResponsesResponse,
) -> Result<ProviderResponse, ProviderError> {
    let output_text = response
        .output
        .iter()
        .filter(|item| item.item_type == "message")
        .flat_map(|item| item.content.iter().flatten())
        .filter(|item| item.item_type == "output_text")
        .filter_map(|item| item.text.as_deref())
        .collect::<String>();

    let tool_calls = response
        .output
        .iter()
        .filter(|item| item.item_type == "function_call")
        .map(|item| {
            Ok(ProviderToolCall {
                call_id: item
                    .call_id
                    .clone()
                    .ok_or(ProviderError::ResponseMissingToolCallField { field: "call_id" })?,
                name: item
                    .name
                    .clone()
                    .ok_or(ProviderError::ResponseMissingToolCallField { field: "name" })?,
                arguments: item
                    .arguments
                    .clone()
                    .ok_or(ProviderError::ResponseMissingToolCallField { field: "arguments" })?,
            })
        })
        .collect::<Result<Vec<_>, ProviderError>>()?;

    ensure_response_has_primary_output(output_text.as_str(), &tool_calls)?;

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
        tool_calls,
        finish_reason,
        usage: response.usage.map(|usage| ProviderUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        }),
    })
}

fn ensure_response_has_primary_output(
    output_text: &str,
    tool_calls: &[ProviderToolCall],
) -> Result<(), ProviderError> {
    if output_text.is_empty() && tool_calls.is_empty() {
        return Err(ProviderError::ResponseMissingOutputText);
    }
    Ok(())
}
