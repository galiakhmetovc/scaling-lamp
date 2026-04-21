use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[test]
fn operator_can_run_a_chat_turn_that_uses_an_allowed_web_tool() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "tool smoke doc");
    let first_provider_response = format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_tool_smoke_1\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"call_web_fetch\",\"name\":\"web_fetch\",\"arguments\":\"{{\\\"url\\\":\\\"{}\\\"}}\"}}],\"usage\":{{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}}}}\n\n",
        web_base
    );
    let (api_base, provider_requests, provider_handle) = spawn_sse_server_sequence(vec![
        first_provider_response,
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tool_smoke_2\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"tool smoke ok\"}]}],\"usage\":{\"input_tokens\":31,\"output_tokens\":4,\"total_tokens\":35}}}\n\n".to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let daemon_port = free_port();

    let created_session = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["session", "create", "session-tool-smoke", "Tool", "Smoke"],
    );
    assert!(created_session.contains("created session session-tool-smoke"));

    let sent = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["chat", "send", "session-tool-smoke", "Fetch", "the", "doc"],
    );
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(sent.contains("chat send session_id=session-tool-smoke"));
    assert!(sent.contains("response_id=resp_tool_smoke_2"));
    assert!(sent.contains("output=tool smoke ok"));

    let shown = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["chat", "show", "session-tool-smoke"],
    );
    assert!(shown.contains("user: Fetch the doc"));
    assert!(shown.contains("assistant: tool smoke ok"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("/v1/responses"));
    assert!(normalized_first.contains("\"tools\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_smoke_1\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("tool smoke doc"));
    assert!(!normalized_second.contains("\"text\":\"fetch the doc\""));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get /doc http/1.1"));
}

fn run_agentd(
    data_dir: &std::path::Path,
    api_base: &str,
    daemon_port: u16,
    args: &[&str],
) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_agentd"))
        .args(args)
        .env("TEAMD_DATA_DIR", data_dir)
        .env("TEAMD_DAEMON_BIND_PORT", daemon_port.to_string())
        .env("TEAMD_PROVIDER_KIND", "openai_responses")
        .env("TEAMD_PROVIDER_API_BASE", format!("{api_base}/v1"))
        .env("TEAMD_PROVIDER_API_KEY", "test-key")
        .env("TEAMD_PROVIDER_MODEL", "gpt-5.4")
        .output()
        .expect("run agentd");

    assert!(
        output.status.success(),
        "agentd failed: status={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .trim()
        .to_string()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind listener")
        .local_addr()
        .expect("local addr")
        .port()
}

fn spawn_sse_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut request = String::new();
            loop {
                let mut line = String::new();
                let bytes = reader.read_line(&mut line).expect("read request line");
                if bytes == 0 {
                    break;
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }

            let content_length = request
                .lines()
                .find_map(|line| {
                    let mut parts = line.splitn(2, ':');
                    let header = parts.next()?.trim();
                    let value = parts.next()?.trim();
                    if header.eq_ignore_ascii_case("content-length") {
                        Some(value.parse::<usize>().expect("content-length"))
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            if content_length > 0 {
                let mut body_bytes = vec![0; content_length];
                reader.read_exact(&mut body_bytes).expect("read body");
                request.push_str(&String::from_utf8(body_bytes).expect("utf8 body"));
            }
            tx.send(request).expect("send request");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{}", address), rx, handle)
}

fn spawn_text_server(
    path: &'static str,
    body: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .expect("set read timeout");

        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut request = String::new();
        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line).expect("read request line");
            if bytes == 0 {
                break;
            }
            request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }

        tx.send(request).expect("send request");

        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush response");
    });

    (format!("http://{}{path}", address), rx, handle)
}
