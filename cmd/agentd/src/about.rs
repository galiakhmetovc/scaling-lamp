use crate::bootstrap::{App, BootstrapError};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(crate) const APP_COMMIT: &str = match option_env!("AGENTD_GIT_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};
pub(crate) const APP_TREE_STATE: &str = match option_env!("AGENTD_GIT_TREE_STATE") {
    Some(state) => state,
    None => "unknown",
};
pub(crate) const APP_BUILD_ID: &str = match option_env!("AGENTD_BUILD_ID") {
    Some(build_id) => build_id,
    None => "unknown",
};
const APP_NAME: &str = env!("CARGO_PKG_NAME");
const APP_REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");
const GITHUB_API_BASE: &str = "https://api.github.com";
const RELEASE_BINARY_ASSET_NAME: &str = "agentd";

pub(crate) fn short_version_label() -> String {
    format!("{APP_NAME} v{APP_VERSION}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositorySlug {
    pub(crate) owner: String,
    pub(crate) repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseAsset {
    name: String,
    download_url: String,
    digest_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseDescriptor {
    tag: String,
    assets: Vec<ReleaseAsset>,
}

trait ReleaseClient: Send + Sync {
    fn latest_release(&self) -> Result<ReleaseDescriptor, BootstrapError>;
    fn release_by_tag(&self, tag: &str) -> Result<ReleaseDescriptor, BootstrapError>;
    fn download_asset(&self, asset: &ReleaseAsset) -> Result<Vec<u8>, BootstrapError>;
}

#[derive(Clone)]
pub(crate) struct RuntimeReleaseUpdater {
    repository: RepositorySlug,
    client: Arc<dyn ReleaseClient>,
    current_executable: PathBuf,
}

impl fmt::Debug for RuntimeReleaseUpdater {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeReleaseUpdater")
            .field("repository", &self.repository)
            .field("current_executable", &self.current_executable)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct GitHubReleaseClient {
    repository: RepositorySlug,
    api_base: String,
    http: Client,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    assets: Vec<GitHubReleaseAssetResponse>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAssetResponse {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

impl RuntimeReleaseUpdater {
    pub(crate) fn github_default() -> Result<Self, BootstrapError> {
        let repository = github_repository_slug_from_url(APP_REPOSITORY_URL)?;
        let current_executable = std::env::current_exe().map_err(BootstrapError::Stream)?;
        Ok(Self {
            repository: repository.clone(),
            client: Arc::new(GitHubReleaseClient::new(repository)),
            current_executable,
        })
    }

    #[cfg(test)]
    fn new(
        repository: RepositorySlug,
        client: Box<dyn ReleaseClient>,
        current_executable: PathBuf,
    ) -> Self {
        Self {
            repository,
            client: Arc::from(client),
            current_executable,
        }
    }

    pub(crate) fn render_version_info(&self) -> Result<String, BootstrapError> {
        let mut lines = vec![
            format!("версия={APP_VERSION}"),
            format!("commit={APP_COMMIT}"),
            format!("tree={APP_TREE_STATE}"),
            format!("build_id={APP_BUILD_ID}"),
            format!("бинарь={}", self.current_executable.display()),
            format!(
                "repository={}/{}",
                self.repository.owner, self.repository.repo
            ),
            "update_source=github-release".to_string(),
        ];

        match self.client.latest_release() {
            Ok(release) => {
                lines.push(format!("latest_release={}", release.tag));
                lines.push(format!(
                    "обновление={}",
                    describe_release_availability(APP_VERSION, &release.tag)
                ));
            }
            Err(error) => {
                lines.push("latest_release=<unavailable>".to_string());
                lines.push(format!("обновление=не удалось проверить: {error}"));
            }
        }

        Ok(lines.join("\n"))
    }

    pub(crate) fn update_runtime(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        let release = match tag {
            Some(tag) if !tag.trim().is_empty() => self.client.release_by_tag(tag.trim())?,
            _ => self.client.latest_release()?,
        };
        let asset = select_release_asset(&release)?;
        let bytes = self.client.download_asset(asset)?;
        verify_release_asset_digest(asset, &bytes)?;

        if self.current_executable.is_file() {
            let current =
                fs::read(&self.current_executable).map_err(|source| BootstrapError::Io {
                    path: self.current_executable.clone(),
                    source,
                })?;
            if current == bytes {
                return Ok(format!(
                    "обновление не требуется: {} уже соответствует релизу {}",
                    self.current_executable.display(),
                    release.tag
                ));
            }
        }

        replace_current_executable(&self.current_executable, &bytes)?;
        Ok(format!(
            "обновлено до {}\nассет={}\nв {}\nперезапустите daemon/TUI, чтобы начала работать новая версия",
            release.tag,
            asset.name,
            self.current_executable.display()
        ))
    }
}

impl GitHubReleaseClient {
    fn new(repository: RepositorySlug) -> Self {
        Self {
            repository,
            api_base: GITHUB_API_BASE.to_string(),
            http: Client::new(),
        }
    }

    fn release_url(&self, suffix: &str) -> String {
        format!(
            "{}/repos/{}/{}/{}",
            self.api_base.trim_end_matches('/'),
            self.repository.owner,
            self.repository.repo,
            suffix
        )
    }

    fn fetch_release(&self, suffix: &str) -> Result<ReleaseDescriptor, BootstrapError> {
        let url = self.release_url(suffix);
        let response = self
            .http
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", format!("{APP_NAME}/{APP_VERSION}"))
            .send()
            .map_err(|error| BootstrapError::Usage {
                reason: format!("не удалось запросить release metadata {url}: {error}"),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(BootstrapError::Usage {
                reason: format!(
                    "release metadata request failed {status} for {}: {}",
                    url,
                    body.trim()
                ),
            });
        }

        let payload: GitHubReleaseResponse =
            response.json().map_err(|error| BootstrapError::Usage {
                reason: format!("не удалось декодировать release metadata {url}: {error}"),
            })?;
        Ok(ReleaseDescriptor {
            tag: payload.tag_name,
            assets: payload
                .assets
                .into_iter()
                .map(|asset| ReleaseAsset {
                    name: asset.name,
                    download_url: asset.browser_download_url,
                    digest_sha256: asset
                        .digest
                        .as_deref()
                        .and_then(normalize_sha256_digest)
                        .map(str::to_string),
                })
                .collect(),
        })
    }
}

impl ReleaseClient for GitHubReleaseClient {
    fn latest_release(&self) -> Result<ReleaseDescriptor, BootstrapError> {
        self.fetch_release("releases/latest")
    }

    fn release_by_tag(&self, tag: &str) -> Result<ReleaseDescriptor, BootstrapError> {
        self.fetch_release(&format!("releases/tags/{tag}"))
    }

    fn download_asset(&self, asset: &ReleaseAsset) -> Result<Vec<u8>, BootstrapError> {
        let response = self
            .http
            .get(&asset.download_url)
            .header("User-Agent", format!("{APP_NAME}/{APP_VERSION}"))
            .send()
            .map_err(|error| BootstrapError::Usage {
                reason: format!(
                    "не удалось скачать release asset {}: {error}",
                    asset.download_url
                ),
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(BootstrapError::Usage {
                reason: format!(
                    "release asset download failed {status} for {}: {}",
                    asset.download_url,
                    body.trim()
                ),
            });
        }

        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|error| BootstrapError::Usage {
                reason: format!(
                    "не удалось прочитать release asset {}: {error}",
                    asset.download_url
                ),
            })
    }
}

fn github_repository_slug_from_url(url: &str) -> Result<RepositorySlug, BootstrapError> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<_> = trimmed.split('/').collect();
    let (owner, repo) = if trimmed.starts_with("https://github.com/")
        || trimmed.starts_with("http://github.com/")
    {
        match parts.as_slice() {
            [_, _, _, owner, repo] => (*owner, *repo),
            _ => {
                return Err(BootstrapError::Usage {
                    reason: format!("unsupported repository URL for self-update: {url}"),
                });
            }
        }
    } else if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        return Err(BootstrapError::Usage {
            reason: format!("unsupported repository URL for self-update: {url}"),
        });
    };

    if owner.is_empty() || repo.is_empty() {
        return Err(BootstrapError::Usage {
            reason: format!("repository owner/repo missing in {url}"),
        });
    }

    Ok(RepositorySlug {
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

fn select_release_asset(release: &ReleaseDescriptor) -> Result<&ReleaseAsset, BootstrapError> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == RELEASE_BINARY_ASSET_NAME)
        .ok_or_else(|| BootstrapError::Usage {
            reason: format!(
                "release {} does not contain required asset {}",
                release.tag, RELEASE_BINARY_ASSET_NAME
            ),
        })
}

fn normalize_sha256_digest(digest: &str) -> Option<&str> {
    digest.strip_prefix("sha256:").or(Some(digest))
}

fn verify_release_asset_digest(asset: &ReleaseAsset, bytes: &[u8]) -> Result<(), BootstrapError> {
    let Some(expected) = asset.digest_sha256.as_deref() else {
        return Ok(());
    };
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        return Ok(());
    }

    Err(BootstrapError::Usage {
        reason: format!(
            "digest mismatch for release asset {}: expected sha256 {}, got {}",
            asset.name, expected, actual
        ),
    })
}

fn replace_current_executable(destination: &Path, bytes: &[u8]) -> Result<(), BootstrapError> {
    let temp_path = destination.with_extension("new");
    fs::write(&temp_path, bytes).map_err(|source| BootstrapError::Io {
        path: temp_path.clone(),
        source,
    })?;

    if destination.is_file() {
        let permissions = fs::metadata(destination)
            .map_err(|source| BootstrapError::Io {
                path: destination.to_path_buf(),
                source,
            })?
            .permissions();
        fs::set_permissions(&temp_path, permissions).map_err(|source| BootstrapError::Io {
            path: temp_path.clone(),
            source,
        })?;
    }

    fs::rename(&temp_path, destination).map_err(|source| BootstrapError::Io {
        path: destination.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn describe_release_availability(current_version: &str, latest_tag: &str) -> String {
    if latest_tag.trim_start_matches('v') == current_version {
        format!("не требуется: установлен актуальный tag {}", latest_tag)
    } else {
        format!("доступно: latest release {}", latest_tag)
    }
}

impl App {
    pub fn render_version_info(&self) -> Result<String, BootstrapError> {
        let mut rendered = self.updater.render_version_info()?;
        rendered.push('\n');
        rendered.push_str(&format!("data_dir={}", self.config.data_dir.display()));
        Ok(rendered)
    }

    pub fn update_runtime_binary(&self, tag: Option<&str>) -> Result<String, BootstrapError> {
        self.updater.update_runtime(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct MockReleaseClient {
        latest: Option<ReleaseDescriptor>,
        by_tag: BTreeMap<String, ReleaseDescriptor>,
        downloads: BTreeMap<String, Vec<u8>>,
    }

    impl ReleaseClient for MockReleaseClient {
        fn latest_release(&self) -> Result<ReleaseDescriptor, BootstrapError> {
            self.latest.clone().ok_or_else(|| BootstrapError::Usage {
                reason: "latest release not configured".to_string(),
            })
        }

        fn release_by_tag(&self, tag: &str) -> Result<ReleaseDescriptor, BootstrapError> {
            self.by_tag
                .get(tag)
                .cloned()
                .ok_or_else(|| BootstrapError::Usage {
                    reason: format!("release {tag} not configured"),
                })
        }

        fn download_asset(&self, asset: &ReleaseAsset) -> Result<Vec<u8>, BootstrapError> {
            self.downloads
                .get(&asset.download_url)
                .cloned()
                .ok_or_else(|| BootstrapError::Usage {
                    reason: format!("asset {} not configured", asset.download_url),
                })
        }
    }

    #[test]
    fn github_repository_slug_extracts_owner_and_repo() {
        let slug = github_repository_slug_from_url("https://github.com/galiakhmetovc/scaling-lamp")
            .expect("repository slug");
        assert_eq!(slug.owner, "galiakhmetovc");
        assert_eq!(slug.repo, "scaling-lamp");
    }

    #[test]
    fn render_version_info_reports_latest_release_when_available() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current_exe = temp.path().join("agentd");
        std::fs::write(&current_exe, b"current-binary").expect("write current exe");

        let updater = RuntimeReleaseUpdater::new(
            RepositorySlug {
                owner: "galiakhmetovc".to_string(),
                repo: "scaling-lamp".to_string(),
            },
            Box::new(MockReleaseClient {
                latest: Some(ReleaseDescriptor {
                    tag: "v1.0.5".to_string(),
                    assets: vec![ReleaseAsset {
                        name: "agentd".to_string(),
                        download_url: "https://example.invalid/agentd".to_string(),
                        digest_sha256: None,
                    }],
                }),
                by_tag: BTreeMap::new(),
                downloads: BTreeMap::new(),
            }),
            current_exe.clone(),
        );

        let about = updater.render_version_info().expect("render version info");
        assert!(about.contains(&format!("версия={APP_VERSION}")));
        assert!(about.contains(&format!("commit={}", APP_COMMIT)));
        assert!(about.contains(&format!("tree={}", APP_TREE_STATE)));
        assert!(about.contains(&format!("build_id={}", APP_BUILD_ID)));
        assert!(about.contains("latest_release=v1.0.5"));
        assert!(about.contains("обновление=доступно"));
    }

    #[test]
    fn update_runtime_binary_downloads_selected_release_and_replaces_current_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current_exe = temp.path().join("agentd");
        std::fs::write(&current_exe, b"current-binary").expect("write current exe");

        let mut downloads = BTreeMap::new();
        downloads.insert(
            "https://example.invalid/v1.0.1/agentd".to_string(),
            b"release-binary".to_vec(),
        );
        let mut by_tag = BTreeMap::new();
        by_tag.insert(
            "v1.0.1".to_string(),
            ReleaseDescriptor {
                tag: "v1.0.1".to_string(),
                assets: vec![ReleaseAsset {
                    name: "agentd".to_string(),
                    download_url: "https://example.invalid/v1.0.1/agentd".to_string(),
                    digest_sha256: None,
                }],
            },
        );
        let updater = RuntimeReleaseUpdater::new(
            RepositorySlug {
                owner: "galiakhmetovc".to_string(),
                repo: "scaling-lamp".to_string(),
            },
            Box::new(MockReleaseClient {
                latest: None,
                by_tag,
                downloads,
            }),
            current_exe.clone(),
        );

        let message = updater
            .update_runtime(Some("v1.0.1"))
            .expect("update runtime");
        assert!(message.contains("v1.0.1"));
        assert_eq!(
            std::fs::read(&current_exe).expect("read replaced exe"),
            b"release-binary".to_vec()
        );
    }
}
