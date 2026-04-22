use crate::bootstrap::{App, BootstrapError};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = env!("CARGO_PKG_NAME");
const BUILD_WORKSPACE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

pub(crate) fn short_version_label() -> String {
    format!("{APP_NAME} v{APP_VERSION}")
}

pub(crate) fn render_version_info() -> String {
    let current_exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    let release_binary = workspace_release_binary_path();
    let update_status = describe_update_status();

    [
        format!("версия={APP_VERSION}"),
        format!("бинарь={current_exe}"),
        format!("workspace_root={BUILD_WORKSPACE_ROOT}"),
        format!("release_binary={}", release_binary.display()),
        format!("обновление={update_status}"),
    ]
    .join("\n")
}

pub(crate) fn update_current_executable_from_workspace_release() -> Result<String, BootstrapError> {
    let source = workspace_release_binary_path();
    if !source.is_file() {
        return Err(BootstrapError::Usage {
            reason: format!(
                "release-бинарь не найден: {}\nСначала выполните cargo build --release -p agentd",
                source.display()
            ),
        });
    }

    let destination = std::env::current_exe().map_err(BootstrapError::Stream)?;
    if same_file_path(source.as_path(), destination.as_path()) {
        return Ok(format!(
            "обновление не требуется: уже используется {}",
            destination.display()
        ));
    }

    let source_bytes = fs::read(&source).map_err(|source_error| BootstrapError::Io {
        path: source.clone(),
        source: source_error,
    })?;
    let destination_bytes = fs::read(&destination).map_err(|source_error| BootstrapError::Io {
        path: destination.clone(),
        source: source_error,
    })?;
    if source_bytes == destination_bytes {
        return Ok(format!(
            "обновление не требуется: {} уже совпадает с {}",
            destination.display(),
            source.display()
        ));
    }

    let temp_path = destination.with_extension("new");
    fs::write(&temp_path, &source_bytes).map_err(|source_error| BootstrapError::Io {
        path: temp_path.clone(),
        source: source_error,
    })?;

    let permissions = fs::metadata(&source)
        .map_err(|source_error| BootstrapError::Io {
            path: source.clone(),
            source: source_error,
        })?
        .permissions();
    fs::set_permissions(&temp_path, permissions).map_err(|source_error| BootstrapError::Io {
        path: temp_path.clone(),
        source: source_error,
    })?;

    fs::rename(&temp_path, &destination).map_err(|source_error| BootstrapError::Io {
        path: destination.clone(),
        source: source_error,
    })?;

    Ok(format!(
        "обновлено из {}\nв {}\nперезапустите TUI/daemon, чтобы начала работать новая версия",
        source.display(),
        destination.display()
    ))
}

pub(crate) fn workspace_release_binary_path() -> PathBuf {
    Path::new(BUILD_WORKSPACE_ROOT)
        .join("target")
        .join("release")
        .join(APP_NAME)
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    left.components().eq(right.components())
}

fn describe_update_status() -> String {
    let source = workspace_release_binary_path();
    if !source.is_file() {
        return "release-сборка не найдена".to_string();
    }

    let Ok(destination) = std::env::current_exe() else {
        return "не удалось определить текущий бинарь".to_string();
    };
    if same_file_path(source.as_path(), destination.as_path()) {
        return "не требуется: уже используется release-бинарь".to_string();
    }

    let Ok(source_bytes) = fs::read(&source) else {
        return "не удалось прочитать release-бинарь".to_string();
    };
    let Ok(destination_bytes) = fs::read(&destination) else {
        return "не удалось прочитать текущий бинарь".to_string();
    };
    if source_bytes == destination_bytes {
        "не требуется: бинарь уже актуален".to_string()
    } else {
        "доступно; выполните /обновить или \\обновить".to_string()
    }
}

impl App {
    pub fn render_version_info(&self) -> Result<String, BootstrapError> {
        Ok(render_version_info())
    }

    pub fn update_runtime_binary(&self) -> Result<String, BootstrapError> {
        update_current_executable_from_workspace_release()
    }
}
