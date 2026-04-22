use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[test]
fn operator_can_run_the_first_autonomous_mission_smoke_flow() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_smoke",
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
                            "text":"smoke ok"
                        }
                    ]
                }
            ],
            "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let daemon_port = free_port();

    let created_session = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["session", "create", "session-smoke", "Smoke", "Session"],
    );
    assert!(created_session.contains("created session session-smoke"));

    let created_mission = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &[
            "mission",
            "create",
            "mission-smoke",
            "session-smoke",
            "Run",
            "the",
            "autonomous",
            "smoke",
        ],
    );
    assert!(created_mission.contains("created mission mission-smoke"));

    let tick = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["mission", "tick", "60"],
    );
    assert!(tick.contains("queued_jobs=1"));
    assert!(tick.contains("queue_job:mission-smoke-mission-turn-60"));

    let execute = run_agentd(
        &data_dir,
        &api_base,
        daemon_port,
        &["job", "execute", "mission-smoke-mission-turn-60", "61"],
    );
    let raw_request = requests.recv().expect("raw request");

    assert!(execute.contains("job execute id=mission-smoke-mission-turn-60"));
    assert!(execute.contains("run_id=run-mission-smoke-mission-turn-60"));
    assert!(execute.contains("response_id=resp_smoke"));
    assert!(execute.contains("output=smoke ok"));

    let run_show = wait_for_agentd_output(
        &data_dir,
        &api_base,
        daemon_port,
        &["run", "show", "run-mission-smoke-mission-turn-60"],
        "status=completed",
    );
    assert!(run_show.contains("status=completed"));

    let job_show = wait_for_agentd_output(
        &data_dir,
        &api_base,
        daemon_port,
        &["job", "show", "mission-smoke-mission-turn-60"],
        "status=completed",
    );
    assert!(job_show.contains("status=completed"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"run the autonomous smoke\""));

    handle.join().expect("join server");
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

fn wait_for_agentd_output(
    data_dir: &std::path::Path,
    api_base: &str,
    daemon_port: u16,
    args: &[&str],
    expected_fragment: &str,
) -> String {
    let mut last = String::new();
    for _ in 0..100 {
        last = run_agentd(data_dir, api_base, daemon_port, args);
        if last.contains(expected_fragment) {
            return last;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let approvals = if let Some(run_id) = args
        .windows(2)
        .find_map(|window| (window[0] == "run" && window[1] == "show").then_some(()))
        .and_then(|_| args.last())
    {
        run_agentd(
            data_dir,
            api_base,
            daemon_port,
            &["approval", "list", run_id],
        )
    } else {
        "<approval diagnostics unavailable>".to_string()
    };
    panic!(
        "timed out waiting for {:?} to contain {:?}; last output: {}; approvals: {}",
        args, expected_fragment, last, approvals
    );
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind listener")
        .local_addr()
        .expect("local addr")
        .port()
}

fn spawn_json_server(body: &'static str) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let address = listener.local_addr().expect("local addr");
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        ready_tx.send(()).expect("send server ready");
        let started_at = std::time::Instant::now();
        let mut served_requests = 0usize;
        loop {
            let accept = listener.accept();
            let (mut stream, _) = match accept {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let idle_limit = if served_requests == 0 {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_millis(250)
                    };
                    if started_at.elapsed() >= idle_limit {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept connection: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
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
            let _ = tx.send(request);

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
            served_requests += 1;
        }
    });
    ready_rx.recv().expect("await server ready");

    (format!("http://{}", address), rx, handle)
}
