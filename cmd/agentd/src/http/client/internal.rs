use super::*;
use std::error::Error as _;
use std::io::BufRead;
use std::time::{Duration, Instant};

impl DaemonClient {
    pub fn new(config: &AppConfig, options: &DaemonConnectOptions) -> Self {
        let host = options
            .host
            .clone()
            .unwrap_or_else(|| default_connect_host(&config.daemon.bind_host));
        let port = options.port.unwrap_or(config.daemon.bind_port);
        Self {
            http: Client::builder()
                .connect_timeout(config.runtime_timing.daemon_http_connect_timeout())
                .timeout(config.runtime_timing.daemon_http_request_timeout())
                .build()
                .expect("build daemon http client"),
            long_http: Client::builder()
                .connect_timeout(config.runtime_timing.daemon_http_connect_timeout())
                .timeout(None::<Duration>)
                .build()
                .expect("build daemon long http client"),
            base_url: format!("http://{host}:{port}"),
            bearer_token: config.daemon.bearer_token.clone(),
            data_dir: config.data_dir.display().to_string(),
            audit: AuditLogConfig::from_config(config),
            runtime_timing: config.runtime_timing.clone(),
            default_diagnostic_tail_lines: config.runtime_limits.diagnostic_tail_lines,
        }
    }

    pub(super) fn get_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let started = self.log_http_request_start("GET", path, false);
        let mut request = self.http.get(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("GET", path, started, None, message.as_str(), false);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        let decoded = decode_response(response);
        self.log_http_request_outcome("GET", path, started, status, &decoded, false);
        decoded
    }

    pub(super) fn post_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let started = self.log_http_request_start("POST", path, false);
        let mut request = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("POST", path, started, None, message.as_str(), false);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        let decoded = decode_response(response);
        self.log_http_request_outcome("POST", path, started, status, &decoded, false);
        decoded
    }

    pub(super) fn patch_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let started = self.log_http_request_start("PATCH", path, false);
        let mut request = self
            .http
            .patch(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("PATCH", path, started, None, message.as_str(), false);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        let decoded = decode_response(response);
        self.log_http_request_outcome("PATCH", path, started, status, &decoded, false);
        decoded
    }

    pub(super) fn post_json_long<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let started = self.log_http_request_start("POST", path, true);
        let mut request = self
            .long_http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("POST", path, started, None, message.as_str(), true);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        let decoded = decode_response(response);
        self.log_http_request_outcome("POST", path, started, status, &decoded, true);
        decoded
    }

    pub(super) fn delete_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let started = self.log_http_request_start("DELETE", path, false);
        let mut request = self.http.delete(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("DELETE", path, started, None, message.as_str(), false);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        let decoded = decode_response(response);
        self.log_http_request_outcome("DELETE", path, started, status, &decoded, false);
        decoded
    }

    pub(super) fn post_json_long_stream<B, F>(
        &self,
        path: &str,
        body: &B,
        mut on_line: F,
    ) -> Result<(), BootstrapError>
    where
        B: serde::Serialize + ?Sized,
        F: FnMut(&str) -> Result<(), BootstrapError>,
    {
        let started = self.log_http_request_start("POST", path, true);
        let mut request = self
            .long_http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            let message = format_http_error(&error);
            self.log_http_request_error("POST", path, started, None, message.as_str(), true);
            BootstrapError::Stream(std::io::Error::other(message))
        })?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let error = decode_error_response(response);
            let error_text = error.to_string();
            self.log_http_request_error("POST", path, started, Some(status), &error_text, true);
            return Err(error);
        }

        let mut reader = std::io::BufReader::new(response);
        let mut line = String::new();
        let mut line_count = 0usize;
        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).map_err(|error| {
                let message = format!("invalid daemon stream: {error}");
                self.log_http_request_error("POST", path, started, Some(status), &message, true);
                BootstrapError::Stream(std::io::Error::other(message))
            })?;
            if bytes_read == 0 {
                break;
            }

            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                continue;
            }
            line_count += 1;
            on_line(line)?;
        }

        self.log_http_request_finish("POST", path, started, status, true, line_count);
        Ok(())
    }

    fn log_http_request_start(&self, method: &str, path: &str, long: bool) -> Instant {
        let started = Instant::now();
        DiagnosticEventBuilder::from_data_dir(
            self.data_dir.clone(),
            "info",
            "daemon_http_client",
            "request.start",
            "sending daemon HTTP request",
        )
        .daemon_base_url(self.base_url.clone())
        .field("http_method", method)
        .field("path", path)
        .field("long_timeout", long)
        .emit(&self.audit);
        started
    }

    fn log_http_request_finish(
        &self,
        method: &str,
        path: &str,
        started: Instant,
        status_code: u16,
        long: bool,
        line_count: usize,
    ) {
        DiagnosticEventBuilder::from_data_dir(
            self.data_dir.clone(),
            "info",
            "daemon_http_client",
            "request.finish",
            "daemon HTTP request completed",
        )
        .daemon_base_url(self.base_url.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("http_method", method)
        .field("path", path)
        .field("status_code", status_code)
        .field("long_timeout", long)
        .field("stream_line_count", line_count)
        .emit(&self.audit);
    }

    fn log_http_request_error(
        &self,
        method: &str,
        path: &str,
        started: Instant,
        status_code: Option<u16>,
        error: &str,
        long: bool,
    ) {
        let mut event = DiagnosticEventBuilder::from_data_dir(
            self.data_dir.clone(),
            "error",
            "daemon_http_client",
            "request.error",
            "daemon HTTP request failed",
        )
        .daemon_base_url(self.base_url.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .error(error.to_string())
        .outcome("error")
        .field("http_method", method)
        .field("path", path)
        .field("long_timeout", long);
        if let Some(status_code) = status_code {
            event = event.field("status_code", status_code);
        }
        event.emit(&self.audit);
    }

    fn log_http_request_outcome<T>(
        &self,
        method: &str,
        path: &str,
        started: Instant,
        status_code: u16,
        result: &Result<T, BootstrapError>,
        long: bool,
    ) {
        match result {
            Ok(_) => self.log_http_request_finish(method, path, started, status_code, long, 0),
            Err(error) => self.log_http_request_error(
                method,
                path,
                started,
                Some(status_code),
                &error.to_string(),
                long,
            ),
        }
    }
}

fn format_http_error(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(next) = source {
        let text = next.to_string();
        if parts.last() != Some(&text) {
            parts.push(text);
        }
        source = next.source();
    }
    parts.join(": ")
}

fn default_connect_host(bind_host: &str) -> String {
    match bind_host.trim() {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" | "[::]" => "[::1]".to_string(),
        other => other.to_string(),
    }
}

fn decode_error_response(response: reqwest::blocking::Response) -> BootstrapError {
    let status = response.status();
    let error = response
        .json::<ErrorResponse>()
        .ok()
        .map(|error| error.error);
    let reason = error.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("daemon error")
            .to_string()
    });
    let kind = if status == StatusCode::UNAUTHORIZED {
        "daemon authorization failed"
    } else {
        "daemon request failed"
    };
    BootstrapError::Usage {
        reason: format!("{kind}: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::default_connect_host;

    #[test]
    fn default_connect_host_maps_ipv4_wildcard_to_loopback() {
        assert_eq!(default_connect_host("0.0.0.0"), "127.0.0.1");
    }

    #[test]
    fn default_connect_host_maps_ipv6_wildcard_to_loopback() {
        assert_eq!(default_connect_host("::"), "[::1]");
        assert_eq!(default_connect_host("[::]"), "[::1]");
    }

    #[test]
    fn default_connect_host_preserves_explicit_hosts() {
        assert_eq!(default_connect_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(default_connect_host("10.6.5.3"), "10.6.5.3");
        assert_eq!(default_connect_host("example.com"), "example.com");
    }
}
