use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[test]
fn operator_can_run_the_normal_chat_smoke_flow() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_chat_smoke",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_1",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[
                        {
                            "type":"output_text",
                            "text":"chat smoke ok"
                        }
                    ]
                }
            ],
            "usage":{"input_tokens":11,"output_tokens":3,"total_tokens":14}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");

    let created_session = run_agentd(
        &data_dir,
        &api_base,
        &["session", "create", "session-chat-smoke", "Chat", "Smoke"],
    );
    assert!(created_session.contains("created session session-chat-smoke"));

    let empty_chat = run_agentd(
        &data_dir,
        &api_base,
        &["chat", "show", "session-chat-smoke"],
    );
    assert_eq!(empty_chat, "<empty>");

    let sent = run_agentd(
        &data_dir,
        &api_base,
        &["chat", "send", "session-chat-smoke", "Hello", "chat"],
    );
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(sent.contains("chat send session_id=session-chat-smoke"));
    assert!(sent.contains("response_id=resp_chat_smoke"));
    assert!(sent.contains("output=chat smoke ok"));

    let shown = run_agentd(
        &data_dir,
        &api_base,
        &["chat", "show", "session-chat-smoke"],
    );
    assert!(shown.contains("user: Hello chat"));
    assert!(shown.contains("assistant: chat smoke ok"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"hello chat\""));
}

fn run_agentd(data_dir: &std::path::Path, api_base: &str, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_agentd"))
        .args(args)
        .env("TEAMD_DATA_DIR", data_dir)
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

fn spawn_json_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
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
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush response");
    });

    (format!("http://{}", address), rx, handle)
}
