use crate::session::MessageRole;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::fmt;

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
    #[default]
    OpenAiResponses,
    ZaiChatCompletions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ConfiguredProvider {
    pub kind: ProviderKind,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelCapabilities {
    pub supports_streaming: bool,
    pub supports_text_input: bool,
    pub supports_tool_calls: bool,
    pub supports_previous_response_id: bool,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub model: Option<String>,
    pub instructions: Option<String>,
    pub messages: Vec<ProviderMessage>,
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
    pub tool_calls: Vec<ProviderToolCall>,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZaiChatCompletionsConfig {
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiResponsesToolDefinition<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    store: bool,
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
            },
        ))),
        ProviderKind::ZaiChatCompletions => Ok(Box::new(ZaiChatCompletionsDriver::new(
            ZaiChatCompletionsConfig {
                api_base,
                api_key,
                default_model: provider.default_model.clone(),
            },
        ))),
    }
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
            client: Client::new(),
            descriptor: ProviderDescriptor {
                name: "openai-responses".to_string(),
                model_family: "openai".to_string(),
                default_model: config.default_model.clone(),
                capabilities: ModelCapabilities {
                    supports_streaming: false,
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
            tools,
            max_output_tokens: request.max_output_tokens,
            store: false,
        })
    }
}

impl ZaiChatCompletionsDriver {
    pub fn new(config: ZaiChatCompletionsConfig) -> Self {
        Self {
            client: Client::new(),
            descriptor: ProviderDescriptor {
                name: "zai-chat-completions".to_string(),
                model_family: "zai".to_string(),
                default_model: config.default_model.clone(),
                capabilities: ModelCapabilities {
                    supports_streaming: false,
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
                thinking_type: "disabled",
            },
            tools,
            tool_choice: (!request.tools.is_empty()).then_some("auto"),
            max_tokens: request.max_output_tokens,
            stream: false,
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
                    arguments: item.arguments.clone().ok_or(
                        ProviderError::ResponseMissingToolCallField { field: "arguments" },
                    )?,
                })
            })
            .collect::<Result<Vec<_>, ProviderError>>()?;

        if output_text.is_empty() && tool_calls.is_empty() {
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
            tool_calls,
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

        if output_text.is_empty() && tool_calls.is_empty() {
            return Err(ProviderError::ResponseMissingOutputText);
        }

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
            Self::HttpStatus { .. }
            | Self::MissingModel
            | Self::ResponseMissingOutputText
            | Self::ResponseMissingToolCallField { .. }
            | Self::UnsupportedMessageRole { .. }
            | Self::UnsupportedStreaming => None,
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

#[cfg(test)]
mod tests {
    use super::{
        ConfiguredProvider, FinishReason, ModelCapabilities, OpenAiResponsesConfig,
        OpenAiResponsesDriver, ProviderBuildError, ProviderDriver, ProviderError, ProviderKind,
        ProviderMessage, ProviderRequest, ProviderStreamMode, ProviderToolDefinition, build_driver,
    };
    use crate::session::MessageRole;
    use serde_json::json;
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
                supports_tool_calls: true,
                supports_previous_response_id: true,
                supports_reasoning_summaries: true,
            }
        );
    }

    #[test]
    fn build_driver_uses_explicit_openai_selection() {
        let driver = build_driver(&ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some("http://127.0.0.1:9/v1".to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
        })
        .expect("build openai driver");

        assert_eq!(driver.descriptor().name, "openai-responses");
        assert_eq!(
            driver.descriptor().default_model.as_deref(),
            Some("gpt-5.4")
        );
    }

    #[test]
    fn build_driver_requires_api_base_and_api_key() {
        let missing_base = build_driver(&ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: None,
            api_key: Some("test-key".to_string()),
            default_model: None,
        })
        .err()
        .expect("missing api base must fail");
        assert!(matches!(missing_base, ProviderBuildError::MissingApiBase));

        let missing_key = build_driver(&ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some("https://api.openai.com/v1".to_string()),
            api_key: None,
            default_model: None,
        })
        .err()
        .expect("missing api key must fail");
        assert!(matches!(missing_key, ProviderBuildError::MissingApiKey));
    }

    #[test]
    fn build_driver_uses_explicit_zai_selection() {
        let driver = build_driver(&ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some("https://api.z.ai/api/paas/v4".to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
        })
        .expect("build zai driver");

        assert_eq!(driver.descriptor().name, "zai-chat-completions");
        assert_eq!(driver.descriptor().model_family, "zai");
        assert_eq!(
            driver.descriptor().default_model.as_deref(),
            Some("glm-5.1")
        );
    }

    #[test]
    fn zai_complete_posts_chat_completions_payload_and_extracts_output_text() {
        let (api_base, requests, handle) = spawn_json_server(
            r#"{
                "id":"chatcmpl-123",
                "model":"glm-5.1",
                "choices":[
                    {
                        "index":0,
                        "finish_reason":"stop",
                        "message":{
                            "role":"assistant",
                            "content":"hello from z.ai"
                        }
                    }
                ],
                "usage":{"prompt_tokens":21,"completion_tokens":9,"total_tokens":30}
            }"#,
        );
        let driver = build_driver(&ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(api_base),
            api_key: Some("zai-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
        })
        .expect("build zai driver");
        let request = ProviderRequest {
            model: None,
            instructions: Some("Be brief".to_string()),
            messages: vec![ProviderMessage::new(MessageRole::User, "Say hi")],
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: Vec::new(),
            tool_outputs: Vec::new(),
            max_output_tokens: Some(64),
            stream: ProviderStreamMode::Disabled,
        };

        let response = driver.complete(&request).expect("complete");
        let raw_request = requests.recv().expect("raw request");
        handle.join().expect("join server");

        assert_eq!(response.response_id, "chatcmpl-123");
        assert_eq!(response.model, "glm-5.1");
        assert_eq!(response.output_text, "hello from z.ai");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.finish_reason, FinishReason::Completed);
        assert_eq!(response.usage.expect("usage").total_tokens, 30);

        let normalized_request = raw_request.to_ascii_lowercase();
        assert!(normalized_request.contains("/chat/completions"));
        assert!(normalized_request.contains("authorization: bearer zai-key"));
        assert!(normalized_request.contains("\"model\":\"glm-5.1\""));
        assert!(normalized_request.contains("\"role\":\"system\""));
        assert!(normalized_request.contains("\"content\":\"be brief\""));
        assert!(normalized_request.contains("\"role\":\"user\""));
        assert!(normalized_request.contains("\"content\":\"say hi\""));
        assert!(normalized_request.contains("\"thinking\":{\"type\":\"disabled\"}"));
        assert!(normalized_request.contains("\"stream\":false"));
    }

    #[test]
    fn zai_complete_accepts_function_call_only_responses() {
        let (api_base, requests, handle) = spawn_json_server(
            r#"{
                "id":"chatcmpl-tool-123",
                "model":"glm-5.1",
                "choices":[
                    {
                        "index":0,
                        "finish_reason":"tool_calls",
                        "message":{
                            "role":"assistant",
                            "content":"",
                            "tool_calls":[
                                {
                                    "id":"call_web_fetch",
                                    "type":"function",
                                    "function":{
                                        "name":"web_fetch",
                                        "arguments":"{\"url\":\"http://127.0.0.1:9999/doc\"}"
                                    }
                                }
                            ]
                        }
                    }
                ],
                "usage":{"prompt_tokens":21,"completion_tokens":9,"total_tokens":30}
            }"#,
        );
        let driver = build_driver(&ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(api_base),
            api_key: Some("zai-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
        })
        .expect("build zai driver");
        let request = ProviderRequest {
            model: None,
            instructions: Some("Use tools when needed".to_string()),
            messages: vec![ProviderMessage::new(MessageRole::User, "Fetch the doc")],
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: vec![ProviderToolDefinition {
                name: "web_fetch".to_string(),
                description: "Fetch a URL".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"],
                    "additionalProperties": false,
                }),
            }],
            tool_outputs: Vec::new(),
            max_output_tokens: Some(64),
            stream: ProviderStreamMode::Disabled,
        };

        let response = driver
            .complete(&request)
            .expect("z.ai tool-call response should be accepted");
        let raw_request = requests.recv().expect("raw request");
        handle.join().expect("join server");

        assert_eq!(response.response_id, "chatcmpl-tool-123");
        assert_eq!(response.model, "glm-5.1");
        assert_eq!(response.output_text, "");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].call_id, "call_web_fetch");
        assert_eq!(response.tool_calls[0].name, "web_fetch");
        assert_eq!(
            response.tool_calls[0].arguments,
            r#"{"url":"http://127.0.0.1:9999/doc"}"#
        );
        assert_eq!(response.finish_reason, FinishReason::Incomplete);

        let normalized_request = raw_request.to_ascii_lowercase();
        assert!(normalized_request.contains("/chat/completions"));
        assert!(normalized_request.contains("\"tool_choice\":\"auto\""));
        assert!(normalized_request.contains("\"name\":\"web_fetch\""));
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
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: Vec::new(),
            tool_outputs: Vec::new(),
            max_output_tokens: None,
            stream: ProviderStreamMode::Disabled,
        };

        let response = driver.complete(&request).expect("complete");
        let raw_request = requests.recv().expect("raw request");
        handle.join().expect("join server");

        assert_eq!(response.response_id, "resp_123");
        assert_eq!(response.model, "gpt-5.4");
        assert_eq!(response.output_text, "hello world");
        assert!(response.tool_calls.is_empty());
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
    fn complete_accepts_function_call_only_responses_for_openai() {
        let (api_base, requests, handle) = spawn_json_server(
            r#"{
                "id":"resp_tool_123",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{\"url\":\"http://127.0.0.1:9999/doc\"}"
                    }
                ],
                "usage":{"input_tokens":14,"output_tokens":6,"total_tokens":20}
            }"#,
        );
        let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
            api_base,
            api_key: "test-key".to_string(),
            default_model: Some("gpt-5.4".to_string()),
        });
        let request = ProviderRequest {
            model: None,
            instructions: Some("Use tools when needed".to_string()),
            messages: vec![ProviderMessage::new(
                MessageRole::User,
                "Fetch the local document",
            )],
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: vec![ProviderToolDefinition {
                name: "web_fetch".to_string(),
                description: "Fetch a URL".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"],
                    "additionalProperties": false,
                }),
            }],
            tool_outputs: Vec::new(),
            max_output_tokens: None,
            stream: ProviderStreamMode::Disabled,
        };

        let response = driver
            .complete(&request)
            .expect("function-call response should be accepted");
        let raw_request = requests.recv().expect("raw request");
        handle.join().expect("join server");

        assert_eq!(response.response_id, "resp_tool_123");
        assert_eq!(response.model, "gpt-5.4");
        assert_eq!(response.output_text, "");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].call_id, "call_web_fetch");
        assert_eq!(response.tool_calls[0].name, "web_fetch");
        assert_eq!(
            response.tool_calls[0].arguments,
            r#"{"url":"http://127.0.0.1:9999/doc"}"#
        );
        assert_eq!(response.finish_reason, FinishReason::Completed);
        assert_eq!(response.usage.expect("usage").total_tokens, 20);

        let normalized_request = raw_request.to_ascii_lowercase();
        assert!(normalized_request.contains("post /v1/responses http/1.1"));
        assert!(normalized_request.contains("\"text\":\"fetch the local document\""));
        assert!(normalized_request.contains("\"tools\":["));
        assert!(normalized_request.contains("\"name\":\"web_fetch\""));
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
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: Vec::new(),
            tool_outputs: Vec::new(),
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
