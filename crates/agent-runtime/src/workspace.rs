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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceReadChunk {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub eof: bool,
    pub next_start_line: Option<usize>,
    pub content: String,
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

    pub fn write_text_with_mode(
        &self,
        path: &str,
        content: &str,
        mode: WriteMode,
    ) -> Result<WriteTextResult, WorkspaceError> {
        let resolved = self.resolve(path)?;
        let existed = resolved.exists();

        match mode {
            WriteMode::Create if existed => {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_string(),
                    reason: "write target already exists",
                });
            }
            WriteMode::Overwrite if !existed => {
                return Err(WorkspaceError::InvalidPath {
                    path: path.to_string(),
                    reason: "write target does not exist",
                });
            }
            WriteMode::Upsert | WriteMode::Create | WriteMode::Overwrite => {}
        }

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

        Ok(WriteTextResult {
            bytes_written: content.len(),
            created: !existed,
            overwritten: existed,
        })
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
        self.find_in_files(path, query)
    }

    pub fn search_text(
        &self,
        path: &str,
        query: &str,
    ) -> Result<Vec<WorkspaceSearchMatch>, WorkspaceError> {
        let resolved = self.resolve(path)?;
        let metadata = fs::metadata(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "must point to a file",
            });
        }

        self.read_matches_in_file(&resolved, query)
    }

    pub fn find_in_files(
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

    pub fn read_lines(
        &self,
        path: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<WorkspaceReadChunk, WorkspaceError> {
        let resolved = self.resolve(path)?;
        let content = fs::read_to_string(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        let lines = split_lines(content.as_str());
        if start_line == 0 || end_line == 0 || start_line > end_line {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "line range must be 1-based and inclusive",
            });
        }
        if lines.is_empty() {
            return Ok(WorkspaceReadChunk {
                path: self.relative_path(&resolved),
                start_line,
                end_line: 0,
                total_lines: 0,
                eof: true,
                next_start_line: None,
                content: String::new(),
            });
        }

        let total_lines = lines.len();
        if start_line > total_lines {
            return Err(WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "start_line exceeds file length",
            });
        }

        let bounded_end = end_line.min(total_lines);
        let selected = lines[start_line - 1..bounded_end].join("\n");
        let content = if bounded_end < total_lines || !selected.is_empty() {
            format!("{selected}\n")
        } else {
            selected
        };
        let eof = bounded_end >= total_lines;

        Ok(WorkspaceReadChunk {
            path: self.relative_path(&resolved),
            start_line,
            end_line: bounded_end,
            total_lines,
            eof,
            next_start_line: (!eof).then_some(bounded_end + 1),
            content,
        })
    }

    pub fn mkdir(&self, path: &str) -> Result<String, WorkspaceError> {
        let resolved = self.resolve(path)?;
        fs::create_dir_all(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        Ok(self.relative_path(&resolved))
    }

    pub fn move_path(&self, src: &str, dest: &str) -> Result<(String, String), WorkspaceError> {
        let resolved_src = self.resolve(src)?;
        let resolved_dest = self.resolve(dest)?;
        if let Some(parent) = resolved_dest.parent() {
            fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::rename(&resolved_src, &resolved_dest).map_err(|source| WorkspaceError::Io {
            path: resolved_src.clone(),
            source,
        })?;
        Ok((
            self.relative_path(&resolved_src),
            self.relative_path(&resolved_dest),
        ))
    }

    pub fn trash_path(&self, path: &str) -> Result<(String, String), WorkspaceError> {
        let resolved = self.resolve(path)?;
        let trash_root = self.root.join(".trash");
        fs::create_dir_all(&trash_root).map_err(|source| WorkspaceError::Io {
            path: trash_root.clone(),
            source,
        })?;
        let file_name = resolved
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "trash target must have a file name",
            })?;
        let target = trash_root.join(format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default(),
            file_name
        ));
        fs::rename(&resolved, &target).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        Ok((self.relative_path(&resolved), self.relative_path(&target)))
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
            matches.extend(self.read_matches_in_file(path, query)?);
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

    fn read_matches_in_file(
        &self,
        path: &Path,
        query: &str,
    ) -> Result<Vec<WorkspaceSearchMatch>, WorkspaceError> {
        let content = fs::read(path).map_err(|source| WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let text = String::from_utf8_lossy(&content);
        Ok(text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(query))
            .map(|(index, line)| WorkspaceSearchMatch {
                path: self.relative_path(path),
                line_number: index + 1,
                line: line.to_string(),
            })
            .collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Create,
    Overwrite,
    Upsert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTextResult {
    pub bytes_written: usize,
    pub created: bool,
    pub overwritten: bool,
}

fn split_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    content.lines().map(str::to_string).collect()
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
