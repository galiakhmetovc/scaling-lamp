use super::*;
use std::error::Error as _;
use std::io::BufRead;
use std::time::Duration;

impl DaemonClient {
    pub fn new(config: &AppConfig, options: &DaemonConnectOptions) -> Self {
        let host = options
            .host
            .clone()
            .unwrap_or_else(|| default_connect_host(&config.daemon.bind_host));
        let port = options.port.unwrap_or(config.daemon.bind_port);
        Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()
                .expect("build daemon http client"),
            long_http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(None::<Duration>)
                .build()
                .expect("build daemon long http client"),
            base_url: format!("http://{host}:{port}"),
            bearer_token: config.daemon.bearer_token.clone(),
        }
    }

    pub(super) fn get_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let mut request = self.http.get(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format_http_error(&error)))
        })?;
        decode_response(response)
    }

    pub(super) fn post_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let mut request = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format_http_error(&error)))
        })?;
        decode_response(response)
    }

    pub(super) fn patch_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let mut request = self
            .http
            .patch(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format_http_error(&error)))
        })?;
        decode_response(response)
    }

    pub(super) fn delete_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let mut request = self.http.delete(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format_http_error(&error)))
        })?;
        decode_response(response)
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
        let mut request = self
            .long_http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format_http_error(&error)))
        })?;
        if !response.status().is_success() {
            return Err(decode_error_response(response));
        }

        let mut reader = std::io::BufReader::new(response);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).map_err(|error| {
                BootstrapError::Stream(std::io::Error::other(format!(
                    "invalid daemon stream: {error}"
                )))
            })?;
            if bytes_read == 0 {
                break;
            }

            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                continue;
            }
            on_line(line)?;
        }

        Ok(())
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
