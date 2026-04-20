use super::*;
use std::time::Duration;

impl DaemonClient {
    pub fn new(config: &AppConfig, options: &DaemonConnectOptions) -> Self {
        let host = options
            .host
            .clone()
            .unwrap_or_else(|| config.daemon.bind_host.clone());
        let port = options.port.unwrap_or(config.daemon.bind_port);
        Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()
                .expect("build daemon http client"),
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
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
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
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
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
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
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
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
        decode_response(response)
    }
}
