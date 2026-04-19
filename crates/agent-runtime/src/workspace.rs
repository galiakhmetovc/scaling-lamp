use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub path: String,
    pub kind: WorkspaceEntryKind,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSearchMatch {
    pub path: String,
    pub line_number: usize,
    pub line: String,
}

#[derive(Debug)]
pub enum WorkspaceError {
    InvalidPath {
        path: String,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl Default for WorkspaceRef {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
        }
    }
}

impl WorkspaceRef {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn resolve(&self, path: &str) -> Result<PathBuf, WorkspaceError> {
        if path.is_empty() {
            return Ok(self.root.clone());
        }

        let candidate = Path::new(path);
        if candidate.is_absolute() {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "must be relative to the workspace root",
            });
        }

        let mut resolved = self.root.clone();

        for component in candidate.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => resolved.push(part),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(WorkspaceError::InvalidPath {
                        path: path.to_string(),
                        reason: "must not escape the workspace root",
                    });
                }
            }
        }

        Ok(resolved)
    }

    pub fn read_text(&self, path: &str) -> Result<String, WorkspaceError> {
        let resolved = self.resolve(path)?;
        fs::read_to_string(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved,
            source,
        })
    }

    pub fn write_text(&self, path: &str, content: &str) -> Result<usize, WorkspaceError> {
        let resolved = self.resolve(path)?;
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        fs::write(&resolved, content).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        Ok(content.len())
    }

    pub fn list(&self, path: &str, recursive: bool) -> Result<Vec<WorkspaceEntry>, WorkspaceError> {
        let resolved = self.resolve(path)?;
        let mut entries = Vec::new();
        self.collect_entries(&resolved, recursive, &mut entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    pub fn search(
        &self,
        path: &str,
        query: &str,
    ) -> Result<Vec<WorkspaceSearchMatch>, WorkspaceError> {
        let resolved = self.resolve(path)?;
        let mut matches = Vec::new();
        self.collect_matches(&resolved, query, &mut matches)?;
        matches.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.line_number.cmp(&right.line_number))
        });
        Ok(matches)
    }

    fn collect_entries(
        &self,
        path: &Path,
        recursive: bool,
        entries: &mut Vec<WorkspaceEntry>,
    ) -> Result<(), WorkspaceError> {
        let metadata = fs::metadata(path).map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        if metadata.is_file() {
            entries.push(WorkspaceEntry {
                path: self.relative_path(path),
                kind: WorkspaceEntryKind::File,
                bytes: Some(metadata.len()),
            });
            return Ok(());
        }

        for entry in fs::read_dir(path).map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })? {
            let entry = entry.map_err(|source| WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            let entry_path = entry.path();
            let entry_metadata = entry.metadata().map_err(|source| WorkspaceError::Io {
                path: entry_path.clone(),
                source,
            })?;

            if entry_metadata.is_dir() {
                entries.push(WorkspaceEntry {
                    path: self.relative_path(&entry_path),
                    kind: WorkspaceEntryKind::Directory,
                    bytes: None,
                });
                if recursive {
                    self.collect_entries(&entry_path, true, entries)?;
                }
            } else {
                entries.push(WorkspaceEntry {
                    path: self.relative_path(&entry_path),
                    kind: WorkspaceEntryKind::File,
                    bytes: Some(entry_metadata.len()),
                });
            }
        }

        Ok(())
    }

    fn collect_matches(
        &self,
        path: &Path,
        query: &str,
        matches: &mut Vec<WorkspaceSearchMatch>,
    ) -> Result<(), WorkspaceError> {
        let metadata = fs::metadata(path).map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        if metadata.is_file() {
            let content = fs::read(path).map_err(|source| WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            let text = String::from_utf8_lossy(&content);

            for (index, line) in text.lines().enumerate() {
                if line.contains(query) {
                    matches.push(WorkspaceSearchMatch {
                        path: self.relative_path(path),
                        line_number: index + 1,
                        line: line.to_string(),
                    });
                }
            }
            return Ok(());
        }

        for entry in fs::read_dir(path).map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })? {
            let entry = entry.map_err(|source| WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            self.collect_matches(&entry.path(), query, matches)?;
        }

        Ok(())
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path, reason } => {
                write!(formatter, "invalid workspace path {path}: {reason}")
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "workspace filesystem error at {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidPath { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceRef;

    #[test]
    fn resolve_path_keeps_access_inside_workspace_root() {
        let workspace = WorkspaceRef::new("/tmp/teamd-workspace");

        assert_eq!(
            workspace.resolve("docs/readme.md").expect("resolve"),
            workspace.root.join("docs/readme.md")
        );
        assert!(workspace.resolve("../escape").is_err());
        assert!(workspace.resolve("/abs/path").is_err());
    }
}
