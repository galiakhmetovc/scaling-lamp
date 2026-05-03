use super::ToolError;
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_BROWSER_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_OUTPUT_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserToolConfig {
    pub enabled: bool,
    pub command: String,
    pub provider: Option<String>,
    pub session_name: String,
    pub default_timeout_ms: u64,
    pub max_output_chars: usize,
    pub browserless_api_key: Option<String>,
    pub browserless_api_url: Option<String>,
    pub browserless_cdp_url: Option<String>,
    pub browserless_browser_type: Option<String>,
    pub browserless_ttl_ms: Option<u64>,
    pub browserless_stealth: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCommandResult {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserToolClient {
    config: BrowserToolConfig,
}

impl Default for BrowserToolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "agent-browser".to_string(),
            provider: None,
            session_name: "teamd-default".to_string(),
            default_timeout_ms: DEFAULT_BROWSER_TIMEOUT_MS,
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
            browserless_api_key: None,
            browserless_api_url: None,
            browserless_cdp_url: None,
            browserless_browser_type: None,
            browserless_ttl_ms: None,
            browserless_stealth: None,
        }
    }
}

impl BrowserToolClient {
    pub fn new(config: BrowserToolConfig) -> Self {
        Self { config }
    }

    pub fn disabled() -> Self {
        Self::new(BrowserToolConfig::default())
    }

    pub fn config(&self) -> &BrowserToolConfig {
        &self.config
    }

    pub fn with_session_name(mut self, session_name: impl Into<String>) -> Self {
        self.config.session_name = session_name.into();
        self
    }

    pub fn invoke<I, S>(
        &self,
        action: &str,
        args: I,
        max_output_chars: Option<usize>,
    ) -> Result<BrowserCommandResult, ToolError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if !self.config.enabled {
            return Err(ToolError::InvalidBrowserRequest {
                reason: "browser tools are disabled; enable [browser] config first".to_string(),
            });
        }

        let mut command = Command::new(&self.config.command);
        command.args(args);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        self.apply_env(&mut command, max_output_chars);

        let display = format_browser_command_display(&self.config.command, action);
        let mut child = command.spawn().map_err(|source| ToolError::BrowserIo {
            command: display.clone(),
            source,
        })?;
        let timeout = Duration::from_millis(self.config.default_timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|source| ToolError::BrowserIo {
                command: display.clone(),
                source,
            })? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ToolError::BrowserFailed {
                    command: display,
                    status_code: None,
                    stderr: format!("timed out after {} ms", timeout.as_millis()),
                });
            }
            thread::sleep(Duration::from_millis(25));
        };

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut pipe) = child.stdout.take() {
            pipe.read_to_end(&mut stdout)
                .map_err(|source| ToolError::BrowserIo {
                    command: display.clone(),
                    source,
                })?;
        }
        if let Some(mut pipe) = child.stderr.take() {
            pipe.read_to_end(&mut stderr)
                .map_err(|source| ToolError::BrowserIo {
                    command: display.clone(),
                    source,
                })?;
        }

        let stdout = String::from_utf8_lossy(&stdout).to_string();
        let stderr = String::from_utf8_lossy(&stderr).to_string();
        if !status.success() {
            return Err(ToolError::BrowserFailed {
                command: display,
                status_code: status.code(),
                stderr,
            });
        }

        Ok(BrowserCommandResult { stdout, stderr })
    }

    fn apply_env(&self, command: &mut Command, max_output_chars: Option<usize>) {
        command.env("NO_COLOR", "1");
        command.env("AGENT_BROWSER_SESSION", &self.config.session_name);
        command.env(
            "AGENT_BROWSER_MAX_OUTPUT",
            max_output_chars
                .unwrap_or(self.config.max_output_chars)
                .max(1)
                .to_string(),
        );
        if self.config.provider.as_deref() == Some("cdp") {
            command.env_remove("AGENT_BROWSER_PROVIDER");
            if let Some(cdp_url) = &self.config.browserless_cdp_url {
                command.env("AGENT_BROWSER_CDP", cdp_url);
            }
        } else if let Some(provider) = &self.config.provider {
            command.env("AGENT_BROWSER_PROVIDER", provider);
        }
        if let Some(api_key) = &self.config.browserless_api_key {
            command.env("BROWSERLESS_API_KEY", api_key);
        }
        if let Some(api_url) = &self.config.browserless_api_url {
            command.env("BROWSERLESS_API_URL", api_url);
        }
        if let Some(browser_type) = &self.config.browserless_browser_type {
            command.env("BROWSERLESS_BROWSER_TYPE", browser_type);
        }
        if let Some(ttl_ms) = self.config.browserless_ttl_ms {
            command.env("BROWSERLESS_TTL", ttl_ms.to_string());
        }
        if let Some(stealth) = self.config.browserless_stealth {
            command.env("BROWSERLESS_STEALTH", stealth.to_string());
        }
    }
}

pub(super) fn normalize_browser_workspace_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn ensure_browser_output_parent(path: &Path) -> Result<(), ToolError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ToolError::BrowserIo {
            command: format!("mkdir {}", parent.display()),
            source,
        })?;
    }
    Ok(())
}

pub(super) fn default_browser_screenshot_path() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("scratch/browser/screenshot-{millis}.png")
}

fn format_browser_command_display(command: &str, action: &str) -> String {
    format!("{command} {action}")
}
