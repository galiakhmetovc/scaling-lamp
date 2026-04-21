use agent_persistence::A2APeerConfig;
use reqwest::blocking::Client;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error as _;
use std::time::Duration;

use crate::http::types::{
    A2ADelegationAcceptedResponse, A2ADelegationCompletionRequest, A2ADelegationCreateRequest,
    ErrorResponse,
};

#[derive(Debug, Clone)]
pub struct A2AClient {
    http: Client,
}

impl Default for A2AClient {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(None::<Duration>)
                .build()
                .expect("build a2a http client"),
        }
    }
}

impl A2AClient {
    pub fn send_delegation(
        &self,
        peer: &A2APeerConfig,
        request: &A2ADelegationCreateRequest,
    ) -> Result<A2ADelegationAcceptedResponse, String> {
        self.post_json(
            &peer.base_url,
            "/v1/a2a/delegations",
            peer.bearer_token.as_deref(),
            request,
        )
    }

    pub fn send_completion(
        &self,
        callback_url: &str,
        bearer_token: Option<&str>,
        request: &A2ADelegationCompletionRequest,
    ) -> Result<(), String> {
        let target = callback_url.trim_end_matches('/').to_string();
        if !target.starts_with("http://") && !target.starts_with("https://") {
            return Err(format!("invalid callback url {callback_url}"));
        }
        let mut req = self.http.post(target).json(request);
        if let Some(token) = bearer_token {
            req = req.bearer_auth(token);
        }
        let response = req.send().map_err(format_http_error)?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let error = response
            .json::<ErrorResponse>()
            .ok()
            .map(|payload| payload.error)
            .unwrap_or_else(|| status.to_string());
        Err(error)
    }

    fn post_json<T, B>(
        &self,
        base_url: &str,
        path: &str,
        bearer_token: Option<&str>,
        body: &B,
    ) -> Result<T, String>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{}{}", base_url.trim_end_matches('/'), path);
        let mut request = self.http.post(url).json(body);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(format_http_error)?;
        if response.status().is_success() {
            return response.json::<T>().map_err(|error| error.to_string());
        }
        let status = response.status();
        let error = response
            .json::<ErrorResponse>()
            .ok()
            .map(|payload| payload.error)
            .unwrap_or_else(|| status.to_string());
        Err(error)
    }
}

fn format_http_error(error: reqwest::Error) -> String {
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
