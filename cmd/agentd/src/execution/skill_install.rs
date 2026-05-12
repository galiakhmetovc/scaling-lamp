use super::ExecutionError;
use agent_runtime::skills::parse_skill_document;
use agent_runtime::workspace::WorkspaceRef;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SkillInstallSource {
    pub name: String,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SkillInstallCopyStats {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SkillInstallFilesystemResult {
    pub stats: SkillInstallCopyStats,
    pub overwritten: bool,
}

pub(super) fn invalid_skill_tool(reason: impl Into<String>) -> ExecutionError {
    ExecutionError::Tool(agent_runtime::tool::ToolError::InvalidMemoryTool {
        reason: reason.into(),
    })
}

pub(super) fn validate_skill_install_name(name: &str) -> Result<String, ExecutionError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid_skill_tool("skill name must not be empty"));
    }
    if trimmed.starts_with('.') {
        return Err(invalid_skill_tool("skill name must not start with dot"));
    }
    if trimmed.contains("..") {
        return Err(invalid_skill_tool("skill name must not contain '..'"));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(invalid_skill_tool(
            "skill name may contain only ASCII letters, digits, dash, underscore, and dot",
        ));
    }
    Ok(trimmed.to_string())
}

pub(super) fn load_skill_install_source(
    workspace_root: &Path,
    source_dir_input: &str,
    expected_name: Option<&str>,
) -> Result<SkillInstallSource, ExecutionError> {
    let workspace = WorkspaceRef::new(workspace_root);
    let source_dir = workspace
        .resolve(source_dir_input)
        .map_err(|source| invalid_skill_tool(source.to_string()))?;
    let workspace_root = fs::canonicalize(&workspace.root).map_err(|source| {
        invalid_skill_tool(format!(
            "failed to canonicalize workspace root {}: {source}",
            workspace.root.display()
        ))
    })?;
    let source_dir = fs::canonicalize(&source_dir).map_err(|source| {
        invalid_skill_tool(format!(
            "failed to canonicalize source_dir {}: {source}",
            source_dir.display()
        ))
    })?;
    if !source_dir.starts_with(&workspace_root) {
        return Err(invalid_skill_tool(
            "source_dir must stay inside the session workspace",
        ));
    }
    let source_metadata = fs::symlink_metadata(&source_dir).map_err(|source| {
        invalid_skill_tool(format!(
            "failed to read source_dir {}: {source}",
            source_dir.display()
        ))
    })?;
    if source_metadata.file_type().is_symlink() {
        return Err(invalid_skill_tool(
            "source_dir must be a real directory, not a symlink",
        ));
    }
    if !source_metadata.is_dir() {
        return Err(invalid_skill_tool(
            "source_dir must be a directory containing SKILL.md",
        ));
    }

    let source_skill_md_path = source_dir.join("SKILL.md");
    let source_skill_md = fs::read_to_string(&source_skill_md_path).map_err(|source| {
        invalid_skill_tool(format!(
            "failed to read {}: {source}",
            source_skill_md_path.display()
        ))
    })?;
    let document = parse_skill_document(&source_skill_md_path, &source_skill_md)
        .map_err(invalid_skill_tool)?;
    let name = validate_skill_install_name(&document.frontmatter.name)?;
    if let Some(expected_name) = expected_name {
        let expected_name = validate_skill_install_name(expected_name)?;
        if expected_name != name {
            return Err(invalid_skill_tool(format!(
                "requested skill name {expected_name} does not match SKILL.md frontmatter name {name}"
            )));
        }
    }

    Ok(SkillInstallSource { name, source_dir })
}

pub(super) fn install_skill_directory(
    source_dir: &Path,
    agent_skills_dir: &Path,
    skill_name: &str,
    now: i64,
    overwrite: bool,
    max_files: usize,
    max_bytes: usize,
) -> Result<SkillInstallFilesystemResult, ExecutionError> {
    fs::create_dir_all(agent_skills_dir).map_err(|source| {
        invalid_skill_tool(format!(
            "failed to create agent skills directory {}: {source}",
            agent_skills_dir.display()
        ))
    })?;
    let destination_dir = agent_skills_dir.join(skill_name);
    let overwritten = destination_dir.exists();
    if fs::symlink_metadata(&destination_dir)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(invalid_skill_tool(format!(
            "refusing to overwrite symlinked skill directory {}",
            destination_dir.display()
        )));
    }
    if overwritten && !overwrite {
        return Err(invalid_skill_tool(format!(
            "skill {skill_name} already exists; pass overwrite=true to replace it"
        )));
    }

    let temp_dir = agent_skills_dir.join(format!(".install-{skill_name}-{now}"));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|source| {
            invalid_skill_tool(format!(
                "failed to remove stale temp skill directory {}: {source}",
                temp_dir.display()
            ))
        })?;
    }
    let copy_result = copy_skill_directory_checked(source_dir, &temp_dir, max_files, max_bytes);
    let stats = match copy_result {
        Ok(stats) => stats,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(error);
        }
    };
    if overwritten {
        fs::remove_dir_all(&destination_dir).map_err(|source| {
            let _ = fs::remove_dir_all(&temp_dir);
            invalid_skill_tool(format!(
                "failed to remove existing skill directory {}: {source}",
                destination_dir.display()
            ))
        })?;
    }
    fs::rename(&temp_dir, &destination_dir).map_err(|source| {
        let _ = fs::remove_dir_all(&temp_dir);
        invalid_skill_tool(format!(
            "failed to install skill into {}: {source}",
            destination_dir.display()
        ))
    })?;

    Ok(SkillInstallFilesystemResult { stats, overwritten })
}

pub(super) fn copy_skill_directory_checked(
    source: &Path,
    destination: &Path,
    max_files: usize,
    max_bytes: usize,
) -> Result<SkillInstallCopyStats, ExecutionError> {
    let mut stats = SkillInstallCopyStats::default();
    copy_skill_directory_entry(source, destination, max_files, max_bytes, &mut stats)?;
    Ok(stats)
}

fn copy_skill_directory_entry(
    source: &Path,
    destination: &Path,
    max_files: usize,
    max_bytes: usize,
    stats: &mut SkillInstallCopyStats,
) -> Result<(), ExecutionError> {
    let metadata = fs::symlink_metadata(source).map_err(|source_error| {
        invalid_skill_tool(format!(
            "failed to read skill source {}: {source_error}",
            source.display()
        ))
    })?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(invalid_skill_tool(format!(
            "skill source must not contain symlinks: {}",
            source.display()
        )));
    }

    if file_type.is_dir() {
        fs::create_dir_all(destination).map_err(|source_error| {
            invalid_skill_tool(format!(
                "failed to create skill destination {}: {source_error}",
                destination.display()
            ))
        })?;
        let mut entries = fs::read_dir(source)
            .map_err(|source_error| {
                invalid_skill_tool(format!(
                    "failed to list skill source {}: {source_error}",
                    source.display()
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source_error| {
                invalid_skill_tool(format!(
                    "failed to read skill source entry in {}: {source_error}",
                    source.display()
                ))
            })?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let file_name = entry.file_name();
            copy_skill_directory_entry(
                entry.path().as_path(),
                destination.join(file_name).as_path(),
                max_files,
                max_bytes,
                stats,
            )?;
        }
        return Ok(());
    }

    if !file_type.is_file() {
        return Err(invalid_skill_tool(format!(
            "skill source must contain only regular files and directories: {}",
            source.display()
        )));
    }

    if stats.files >= max_files {
        return Err(invalid_skill_tool(format!(
            "skill directory has too many files; max is {max_files}"
        )));
    }
    let next_bytes = stats.bytes.saturating_add(metadata.len());
    let max_bytes_u64 = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if next_bytes > max_bytes_u64 {
        return Err(invalid_skill_tool(format!(
            "skill directory is too large; max is {max_bytes} bytes"
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source_error| {
            invalid_skill_tool(format!(
                "failed to create skill destination {}: {source_error}",
                parent.display()
            ))
        })?;
    }
    fs::copy(source, destination).map_err(|source_error| {
        invalid_skill_tool(format!(
            "failed to copy skill file {} to {}: {source_error}",
            source.display(),
            destination.display()
        ))
    })?;
    stats.files += 1;
    stats.bytes = next_bytes;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validate_skill_install_name_accepts_safe_names() {
        assert_eq!(
            validate_skill_install_name("daily-note").expect("valid skill name"),
            "daily-note"
        );
        assert_eq!(
            validate_skill_install_name("browser_tools.v2").expect("valid skill name"),
            "browser_tools.v2"
        );
    }

    #[test]
    fn validate_skill_install_name_rejects_path_like_names() {
        for name in ["", ".hidden", "../x", "x/y", "name with spaces", "русский"] {
            assert!(
                validate_skill_install_name(name).is_err(),
                "{name:?} must be rejected"
            );
        }
    }

    #[test]
    fn copy_skill_directory_checked_copies_nested_regular_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fs::create_dir_all(source.join("references")).expect("create source dirs");
        fs::write(
            source.join("SKILL.md"),
            "---\nname: daily-note\ndescription: Daily notes.\n---\n",
        )
        .expect("write skill");
        fs::write(source.join("references/format.md"), "# Format\n").expect("write reference");

        let stats = copy_skill_directory_checked(&source, &destination, 10, 4096).expect("copy");

        assert_eq!(stats.files, 2);
        assert!(stats.bytes > 0);
        assert!(destination.join("SKILL.md").is_file());
        assert!(destination.join("references/format.md").is_file());
    }

    #[test]
    fn copy_skill_directory_checked_enforces_file_and_byte_limits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("one.txt"), "12345").expect("write one");
        fs::write(source.join("two.txt"), "67890").expect("write two");

        assert!(
            copy_skill_directory_checked(&source, temp.path().join("too-many").as_path(), 1, 4096)
                .is_err()
        );
        assert!(
            copy_skill_directory_checked(&source, temp.path().join("too-large").as_path(), 10, 5)
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_skill_directory_checked_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join("target.txt"), "target").expect("write target");
        symlink(source.join("target.txt"), source.join("link.txt")).expect("create symlink");

        let error = copy_skill_directory_checked(
            &source,
            temp.path().join("destination").as_path(),
            10,
            4096,
        )
        .expect_err("symlink must be rejected");

        assert!(error.to_string().contains("symlinks"));
    }
}
