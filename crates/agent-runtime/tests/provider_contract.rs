use agent_runtime::provider::{
    ConfiguredProvider, FinishReason, ModelCapabilities, OpenAiResponsesConfig,
    OpenAiResponsesDriver, ProviderBuildError, ProviderDriver, ProviderKind, ProviderMessage,
    ProviderRequest, ProviderResponse, ProviderStreamEvent, ProviderStreamMode,
    ProviderToolDefinition, build_driver,
};
use agent_runtime::session::MessageRole;
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
            supports_streaming: true,
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
fn zai_stream_emits_text_deltas_and_final_response() {
    let (api_base, requests, handle) = spawn_sse_server(
        "data: {\"id\":\"chatcmpl-stream-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"thinking... \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-stream-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-stream-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"world\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n",
    );
    let driver = build_driver(&ConfiguredProvider {
        kind: ProviderKind::ZaiChatCompletions,
        api_base: Some(api_base),
        api_key: Some("zai-key".to_string()),
        default_model: Some("glm-5-turbo".to_string()),
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
        stream: ProviderStreamMode::Enabled,
    };

    let mut stream = driver.stream(&request).expect("stream");
    let first = stream
        .next_event()
        .expect("first event")
        .expect("some first event");
    let second = stream
        .next_event()
        .expect("second event")
        .expect("some second event");
    let third = stream
        .next_event()
        .expect("third event")
        .expect("some third event");
    let fourth = stream
        .next_event()
        .expect("fourth event")
        .expect("some fourth event");
    let done = stream.next_event().expect("done");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(
        first,
        ProviderStreamEvent::ReasoningDelta("thinking... ".to_string())
    );
    assert_eq!(second, ProviderStreamEvent::TextDelta("hello ".to_string()));
    assert_eq!(third, ProviderStreamEvent::TextDelta("world".to_string()));
    assert_eq!(
        fourth,
        ProviderStreamEvent::Completed(ProviderResponse {
            response_id: "chatcmpl-stream-1".to_string(),
            model: "glm-5-turbo".to_string(),
            output_text: "hello world".to_string(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Completed,
            usage: None,
        })
    );
    assert!(done.is_none());

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/chat/completions"));
    assert!(normalized_request.contains("\"stream\":true"));
    assert!(normalized_request.contains("\"thinking\":{\"type\":\"enabled\"}"));
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
fn openai_stream_emits_reasoning_summary_text_and_final_response() {
    let (api_base, requests, handle) = spawn_sse_server(
        "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_123\",\"output_index\":0,\"summary_index\":0,\"delta\":\"Plan: inspect the question. \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"world\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_123\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_123\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Plan: inspect the question. \"}]},{\"id\":\"msg_123\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello world\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n",
    );
    let driver = OpenAiResponsesDriver::new(OpenAiResponsesConfig {
        api_base,
        api_key: "test-key".to_string(),
        default_model: Some("gpt-5.4".to_string()),
    });
    let request = ProviderRequest {
        model: Some("gpt-5.4".to_string()),
        instructions: Some("Be brief".to_string()),
        messages: vec![ProviderMessage::new(MessageRole::User, "ping")],
        previous_response_id: None,
        continuation_messages: Vec::new(),
        tools: Vec::new(),
        tool_outputs: Vec::new(),
        max_output_tokens: None,
        stream: ProviderStreamMode::Enabled,
    };

    let mut stream = driver.stream(&request).expect("stream");
    let first = stream
        .next_event()
        .expect("first event")
        .expect("some first event");
    let second = stream
        .next_event()
        .expect("second event")
        .expect("some second event");
    let third = stream
        .next_event()
        .expect("third event")
        .expect("some third event");
    let fourth = stream
        .next_event()
        .expect("fourth event")
        .expect("some fourth event");
    let done = stream.next_event().expect("done");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(
        first,
        ProviderStreamEvent::ReasoningDelta("Plan: inspect the question. ".to_string())
    );
    assert_eq!(second, ProviderStreamEvent::TextDelta("hello ".to_string()));
    assert_eq!(third, ProviderStreamEvent::TextDelta("world".to_string()));
    assert_eq!(
        fourth,
        ProviderStreamEvent::Completed(ProviderResponse {
            response_id: "resp_stream_123".to_string(),
            model: "gpt-5.4".to_string(),
            output_text: "hello world".to_string(),
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Completed,
            usage: Some(agent_runtime::provider::ProviderUsage {
                input_tokens: 11,
                output_tokens: 7,
                total_tokens: 18,
            }),
        })
    );
    assert!(done.is_none());

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("post /v1/responses http/1.1"));
    assert!(normalized_request.contains("\"stream\":true"));
    assert!(normalized_request.contains("\"reasoning\":{\"summary\":\"auto\"}"));
}

fn spawn_sse_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
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
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
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
