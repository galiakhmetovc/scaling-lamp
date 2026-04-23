use crate::bootstrap::App;
use crate::diagnostics::DiagnosticEventBuilder;
use crate::http::server;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DaemonHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

pub fn serve(app: App) -> std::io::Result<()> {
    DiagnosticEventBuilder::new(
        &app.config,
        "info",
        "daemon",
        "serve.start",
        "daemon server starting",
    )
    .field("bind_host", &app.config.daemon.bind_host)
    .field("bind_port", app.config.daemon.bind_port)
    .emit(&app.persistence.audit);
    let diagnostic_app = app.clone();
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker = spawn_background_worker(app.clone(), shutdown.clone());
    let result = server::serve(app, shutdown.clone());
    shutdown.store(true, Ordering::Relaxed);
    let _ = worker.join();
    let finish = match &result {
        Ok(()) => DiagnosticEventBuilder::new(
            &diagnostic_app.config,
            "info",
            "daemon",
            "serve.finish",
            "daemon server stopped",
        )
        .outcome("ok"),
        Err(error) => DiagnosticEventBuilder::new(
            &diagnostic_app.config,
            "error",
            "daemon",
            "serve.finish",
            "daemon server stopped with error",
        )
        .error(error.to_string())
        .outcome("error"),
    };
    finish.emit(&diagnostic_app.persistence.audit);
    result
}

pub fn spawn_local_process() -> Result<(), std::io::Error> {
    let current_exe = std::env::current_exe()?;
    Command::new(current_exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

pub fn spawn_for_test(app: App) -> std::io::Result<DaemonHandle> {
    let bind = format!(
        "{}:{}",
        app.config.daemon.bind_host, app.config.daemon.bind_port
    );
    let startup_probe_attempts = app.config.runtime_timing.daemon_test_startup_probe_attempts;
    let startup_probe_interval = app
        .config
        .runtime_timing
        .daemon_test_startup_probe_interval();
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_app = app.clone();
    let worker_shutdown = shutdown.clone();
    let worker = spawn_background_worker(worker_app, worker_shutdown);
    let thread_shutdown = shutdown.clone();
    let thread = thread::spawn(move || {
        let result = server::serve(app, thread_shutdown.clone());
        thread_shutdown.store(true, Ordering::Relaxed);
        let _ = worker.join();
        result
    });

    for _ in 0..startup_probe_attempts {
        if TcpStream::connect(&bind).is_ok() {
            break;
        }
        thread::sleep(startup_probe_interval);
    }

    Ok(DaemonHandle {
        shutdown,
        thread: Some(thread),
    })
}

impl DaemonHandle {
    pub fn stop(mut self) -> std::io::Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };

        match thread.join() {
            Ok(result) => result,
            Err(_) => Err(std::io::Error::other("daemon thread panicked")),
        }
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn spawn_background_worker(app: App, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::spawn(move || {
        while !shutdown.load(Ordering::Relaxed) {
            let _ = app.background_worker_tick(unix_timestamp());
            thread::sleep(
                app.config
                    .runtime_timing
                    .daemon_background_worker_tick_interval(),
            );
        }
    })
}
