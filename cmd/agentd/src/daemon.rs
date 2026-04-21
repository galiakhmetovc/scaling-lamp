use crate::bootstrap::App;
use crate::http::server;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct DaemonHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

pub fn serve(app: App) -> std::io::Result<()> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker = spawn_background_worker(app.clone(), shutdown.clone());
    let result = server::serve(app, shutdown.clone());
    shutdown.store(true, Ordering::Relaxed);
    let _ = worker.join();
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

    for _ in 0..50 {
        if TcpStream::connect(&bind).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
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
            thread::sleep(Duration::from_millis(100));
        }
    })
}
