use agent_persistence::AppConfig;
use agentd::bootstrap;
use agentd::cli;
use agentd::daemon;
use std::io::Cursor;
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_config() -> (tempfile::TempDir, AppConfig) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("secret-token".to_string());
    (temp, config)
}

#[test]
fn process_cli_routes_status_and_session_skills_over_remote_daemon() {
    let (temp, mut config) = test_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("create skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors.\n---\n\n# rust-debug\n",
    )
    .expect("write skill");
    config.daemon.skills_dir = skills_dir;

    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let session = app
        .create_session_auto(Some("Daemon CLI Session"))
        .expect("create session");
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");

    let mut status_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &config.daemon.bind_port.to_string(),
            "status",
        ],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut status_output,
    )
    .expect("process status");
    let status_output = String::from_utf8(status_output).expect("utf8");
    assert!(status_output.contains("permission_mode=default"));
    assert!(status_output.contains("sessions=1"));

    let mut skills_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &config.daemon.bind_port.to_string(),
            "session",
            "skills",
            session.id.as_str(),
        ],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut skills_output,
    )
    .expect("remote session skills");
    let skills_output = String::from_utf8(skills_output).expect("utf8");
    assert!(skills_output.contains("rust-debug"));

    handle.stop().expect("stop daemon");
}

#[test]
fn process_cli_routes_agent_profile_commands_over_remote_daemon() {
    let (_temp, config) = test_config();
    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");

    let port = config.daemon.bind_port.to_string();
    let base_args = ["--host", "127.0.0.1", "--port", port.as_str()];

    let mut list_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        base_args.into_iter().chain(["agent", "list"]),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut list_output,
    )
    .expect("remote agent list");
    let list_output = String::from_utf8(list_output).expect("utf8");
    assert!(list_output.contains("default"));
    assert!(list_output.contains("judge"));

    let mut create_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        base_args
            .into_iter()
            .chain(["agent", "create", "Remote Reviewer", "from", "judge"]),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut create_output,
    )
    .expect("remote agent create");
    let create_output = String::from_utf8(create_output).expect("utf8");
    assert!(create_output.contains("Remote Reviewer"));

    let mut show_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        base_args
            .into_iter()
            .chain(["agent", "show", "Remote Reviewer"]),
        &mut Cursor::new(Vec::<u8>::new()),
        &mut show_output,
    )
    .expect("remote agent show");
    let show_output = String::from_utf8(show_output).expect("utf8");
    assert!(show_output.contains("id=remote-reviewer"));
    assert!(show_output.contains("default_workspace_root="));

    handle.stop().expect("stop daemon");
}

#[test]
fn process_cli_chat_repl_uses_remote_daemon_for_skill_commands() {
    let (temp, mut config) = test_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("create skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors.\n---\n\n# rust-debug\n",
    )
    .expect("write skill");
    config.daemon.skills_dir = skills_dir;

    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let session = app
        .create_session_auto(Some("Remote REPL Session"))
        .expect("create session");
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");

    let mut input = Cursor::new("\\скиллы\n\\выход\n".as_bytes().to_vec());
    let mut output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &config.daemon.bind_port.to_string(),
            "chat",
            "repl",
            session.id.as_str(),
        ],
        &mut input,
        &mut output,
    )
    .expect("remote repl");

    let output = String::from_utf8(output).expect("utf8");
    assert!(output.contains("чатовый режим session_id="));
    assert!(output.contains("Скиллы:"));
    assert!(output.contains("rust-debug"));
    assert!(output.contains("выход из чатового режима"));

    handle.stop().expect("stop daemon");
}

#[test]
fn process_cli_can_create_and_show_sessions_over_remote_daemon() {
    let (_temp, config) = test_config();
    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");

    let port = config.daemon.bind_port.to_string();

    let mut create_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "session",
            "create",
            "session-remote",
            "Remote",
            "Session",
        ],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut create_output,
    )
    .expect("remote create session");
    let create_output = String::from_utf8(create_output).expect("utf8");
    assert!(create_output.contains("created session session-remote"));

    let mut list_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        ["--host", "127.0.0.1", "--port", &port, "session", "list"],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut list_output,
    )
    .expect("remote list sessions");
    let list_output = String::from_utf8(list_output).expect("utf8");
    assert!(list_output.contains("Sessions"));
    assert!(list_output.contains("total: 1"));
    assert!(list_output.contains("1. Remote Session"));
    assert!(list_output.contains("id: session-remote"));

    let mut show_output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "session",
            "show",
            "session-remote",
        ],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut show_output,
    )
    .expect("remote show session");
    let show_output = String::from_utf8(show_output).expect("utf8");
    assert!(show_output.contains("session id=session-remote"));
    assert!(show_output.contains("Remote Session"));

    handle.stop().expect("stop daemon");
}

#[test]
fn process_cli_can_stop_a_remote_daemon_without_autospawning() {
    let (_temp, config) = test_config();
    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app.clone()).expect("spawn daemon");

    let mut output = Vec::new();
    cli::execute_process_with_io(
        &app,
        [
            "--host",
            "127.0.0.1",
            "--port",
            &config.daemon.bind_port.to_string(),
            "daemon",
            "stop",
        ],
        &mut Cursor::new(Vec::<u8>::new()),
        &mut output,
    )
    .expect("daemon stop");

    let output = String::from_utf8(output).expect("utf8");
    assert!(output.contains("daemon stopping"));

    handle.stop().expect("join stopped daemon");
}

#[test]
fn process_cli_shuts_down_autospawned_local_daemon_after_one_shot_command() {
    let temp = tempfile::tempdir().expect("tempdir");
    let daemon_port = free_port();
    let data_dir = temp.path().join("state-root");

    let output = Command::new(env!("CARGO_BIN_EXE_agentd"))
        .args(["session", "create", "session-auto", "Auto", "Spawned"])
        .env("TEAMD_DATA_DIR", &data_dir)
        .env("TEAMD_DAEMON_BIND_HOST", "127.0.0.1")
        .env("TEAMD_DAEMON_BIND_PORT", daemon_port.to_string())
        .output()
        .expect("run agentd");

    assert!(
        output.status.success(),
        "agentd failed: status={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = String::from_utf8(output.stdout).expect("utf8");
    assert!(output.contains("created session session-auto"));

    let bind = format!("127.0.0.1:{daemon_port}");
    for _ in 0..50 {
        if TcpStream::connect(&bind).is_err() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }

    panic!("autospawned daemon at {bind} stayed alive after one-shot process command");
}
