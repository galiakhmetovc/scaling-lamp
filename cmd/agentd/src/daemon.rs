use crate::bootstrap::App;
use crate::http::server;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct DaemonHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

pub fn serve(app: App) -> std::io::Result<()> {
    server::serve(app, Arc::new(AtomicBool::new(false)))
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
    let thread_shutdown = shutdown.clone();
    let thread = thread::spawn(move || server::serve(app, thread_shutdown));

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
