use super::{ToolError, WebFetchOutput, WebSearchOutput, WebSearchResult};
use html_to_markdown_rs::convert as convert_html_to_markdown;
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct WebToolClient {
    client: Client,
    search_backend: WebSearchBackend,
    search_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchBackend {
    #[serde(rename = "duckduckgo_html")]
    #[default]
    DuckDuckGoHtml,
    #[serde(rename = "searxng_json")]
    SearxngJson,
}

impl Default for WebToolClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent/0.1")
                .build()
                .expect("web tool client"),
            search_backend: WebSearchBackend::default(),
            search_url: "https://duckduckgo.com/html/".to_string(),
        }
    }
}

impl WebToolClient {
    pub fn new(search_backend: WebSearchBackend, search_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent/0.1")
                .build()
                .expect("web tool client"),
            search_backend,
            search_url: search_url.into(),
        }
    }

    pub fn for_tests(_base_url: impl Into<String>, search_url: impl Into<String>) -> Self {
        Self::for_tests_with_search_backend(WebSearchBackend::DuckDuckGoHtml, _base_url, search_url)
    }

    pub fn for_tests_with_search_backend(
        search_backend: WebSearchBackend,
        _base_url: impl Into<String>,
        search_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent-test/0.1")
                .build()
                .expect("test web tool client"),
            search_backend,
            search_url: search_url.into(),
        }
    }

    pub(super) fn fetch(&self, url: &str) -> Result<WebFetchOutput, ToolError> {
        let RawWebResponse {
            url,
            status_code,
            content_type,
            body,
        } = self.fetch_raw(url)?;
        let extracted_from_html = is_html_response(content_type.as_deref(), body.as_str());
        let title = extracted_from_html
            .then(|| extract_html_title(body.as_str()))
            .flatten();
        let body = if extracted_from_html {
            render_html_fetch_body(body.as_str())
        } else {
            body
        };

        Ok(WebFetchOutput {
            url,
            status_code,
            content_type,
            title,
            extracted_from_html,
            body,
        })
    }

    pub(super) fn search(&self, query: &str, limit: usize) -> Result<WebSearchOutput, ToolError> {
        if query.trim().is_empty() {
            return Err(ToolError::InvalidWebRequest {
                reason: "query must not be empty".to_string(),
            });
        }

        let mut url = self.search_url()?;
        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("q", query);
            if self.search_backend == WebSearchBackend::SearxngJson {
                query_pairs.append_pair("format", "json");
            }
        }

        let fetch = self.fetch_raw(url.as_str())?;
        let mut results = match self.search_backend {
            WebSearchBackend::DuckDuckGoHtml => {
                parse_search_results(&fetch.body, fetch.url.as_str())?
            }
            WebSearchBackend::SearxngJson => {
                parse_searxng_json_results(&fetch.body, fetch.url.as_str())?
            }
        };
        if limit > 0 && results.len() > limit {
            results.truncate(limit);
        }

        Ok(WebSearchOutput {
            query: query.to_string(),
            results,
        })
    }

    fn fetch_raw(&self, url: &str) -> Result<RawWebResponse, ToolError> {
        let response = self.client.get(url).send().map_err(ToolError::WebHttp)?;
        let status_code = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ToolError::WebHttpStatus {
                url: url.to_string(),
                status_code,
            });
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.text().map_err(ToolError::WebHttp)?;

        Ok(RawWebResponse {
            url: url.to_string(),
            status_code,
            content_type,
            body,
        })
    }

    fn search_url(&self) -> Result<Url, ToolError> {
        Url::parse(&self.search_url).map_err(|_| ToolError::InvalidWebRequest {
            reason: format!("invalid search URL: {}", self.search_url),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawWebResponse {
    url: String,
    status_code: u16,
    content_type: Option<String>,
    body: String,
}

pub(super) fn parse_search_results(
    html: &str,
    source_url: &str,
) -> Result<Vec<WebSearchResult>, ToolError> {
    let mut results = Vec::new();
    let mut cursor = html;

    while let Some((_, tag_end, tag)) = find_anchor_tag_with_class(cursor, "result__a") {
        let Some(raw_url) = extract_html_attr(tag, "href") else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result href was missing".to_string(),
            });
        };
        let url = normalize_duckduckgo_result_url(&decode_html_entities(raw_url));
        cursor = &cursor[tag_end + 1..];

        let Some(title_end) = cursor.find("</a>") else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result title was not terminated".to_string(),
            });
        };
        let title = strip_html_tags(&decode_html_entities(&cursor[..title_end]));
        cursor = &cursor[title_end + 4..];

        let next_result = find_anchor_tag_with_class(cursor, "result__a")
            .map(|(index, _, _)| index)
            .unwrap_or(cursor.len());
        let snippet_region = &cursor[..next_result];
        let snippet = find_anchor_tag_with_class(snippet_region, "result__snippet").and_then(
            |(_, snippet_tag_end, _)| {
                let after_tag = &snippet_region[snippet_tag_end + 1..];
                after_tag.find("</a>").map(|snippet_end| {
                    strip_html_tags(&decode_html_entities(&after_tag[..snippet_end]))
                })
            },
        );

        results.push(WebSearchResult {
            title,
            url,
            snippet,
        });
    }

    Ok(results)
}

#[derive(Debug, Deserialize)]
struct SearxngSearchResponse {
    #[serde(default)]
    results: Vec<SearxngSearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearxngSearchResult {
    title: Option<String>,
    url: Option<String>,
    content: Option<String>,
}

fn parse_searxng_json_results(
    body: &str,
    source_url: &str,
) -> Result<Vec<WebSearchResult>, ToolError> {
    let response: SearxngSearchResponse =
        serde_json::from_str(body).map_err(|error| ToolError::WebParse {
            url: source_url.to_string(),
            reason: format!("invalid SearXNG JSON response: {error}"),
        })?;

    Ok(response
        .results
        .into_iter()
        .filter_map(|result| {
            let title = result.title?.trim().to_string();
            let url = result.url?.trim().to_string();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            Some(WebSearchResult {
                title,
                url,
                snippet: result
                    .content
                    .map(|content| content.trim().to_string())
                    .filter(|content| !content.is_empty()),
            })
        })
        .collect())
}

fn find_anchor_tag_with_class<'a>(
    haystack: &'a str,
    class_name: &str,
) -> Option<(usize, usize, &'a str)> {
    let mut search_from = 0;
    while let Some(relative_start) = haystack[search_from..].find("<a") {
        let start = search_from + relative_start;
        let relative_end = haystack[start..].find('>')?;
        let end = start + relative_end;
        let tag = &haystack[start..=end];
        if html_tag_has_class(tag, class_name) {
            return Some((start, end, tag));
        }
        search_from = end + 1;
    }
    None
}

fn html_tag_has_class(tag: &str, expected: &str) -> bool {
    extract_html_attr(tag, "class")
        .map(|classes| classes.split_whitespace().any(|class| class == expected))
        .unwrap_or(false)
}

fn extract_html_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let prefix = format!("{name}={quote}");
        if let Some(start) = tag.find(prefix.as_str()) {
            let value_start = start + prefix.len();
            let value_end = tag[value_start..].find(quote)?;
            return Some(&tag[value_start..value_start + value_end]);
        }
    }
    None
}

fn normalize_duckduckgo_result_url(url: &str) -> String {
    let absolute = if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    };
    let Ok(parsed) = Url::parse(absolute.as_str()) else {
        return url.to_string();
    };
    let is_duckduckgo_redirect = parsed
        .host_str()
        .is_some_and(|host| host.ends_with("duckduckgo.com"))
        && parsed.path().starts_with("/l/");
    if !is_duckduckgo_redirect {
        return absolute;
    }
    parsed
        .query_pairs()
        .find(|(key, _)| key == "uddg")
        .map(|(_, value)| value.into_owned())
        .unwrap_or(absolute)
}

fn is_html_response(content_type: Option<&str>, body: &str) -> bool {
    if let Some(content_type) = content_type {
        let normalized = content_type.to_ascii_lowercase();
        if normalized.contains("html") || normalized.contains("xhtml") {
            return true;
        }
    }

    let trimmed = body.trim_start().to_ascii_lowercase();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<head")
        || trimmed.starts_with("<body")
}

fn extract_html_title(input: &str) -> Option<String> {
    extract_html_tag_text(input, "title")
}

fn extract_html_tag_text(input: &str, tag_name: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let open_pattern = format!("<{tag_name}");
    let close_pattern = format!("</{tag_name}>");
    let start = lower.find(open_pattern.as_str())?;
    let open_end = input[start..].find('>')? + start + 1;
    let close_start = lower[open_end..].find(close_pattern.as_str())? + open_end;
    let text = strip_html_tags(&decode_html_entities(&input[open_end..close_start]));
    (!text.is_empty()).then_some(text)
}

fn render_html_fetch_body(input: &str) -> String {
    match convert_html_to_markdown(input, None) {
        Ok(markdown) => {
            let normalized = normalize_markdown_output(markdown.content.as_deref().unwrap_or(""));
            if normalized.is_empty() {
                fallback_extract_html_text(input)
            } else {
                normalized
            }
        }
        Err(_) => fallback_extract_html_text(input),
    }
}

fn normalize_markdown_output(input: &str) -> String {
    input.replace("\r\n", "\n").trim().to_string()
}

fn fallback_extract_html_text(input: &str) -> String {
    collapse_inline_whitespace(&strip_html_tags(&decode_html_entities(input)))
}

fn collapse_inline_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in input.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    output.trim().to_string()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}
