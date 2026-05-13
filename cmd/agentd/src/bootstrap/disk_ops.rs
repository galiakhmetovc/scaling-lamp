use super::{App, BootstrapError, unix_timestamp};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskUsageReport {
    pub generated_at: i64,
    pub data_dir: String,
    pub total_bytes: u64,
    pub categories: Vec<DiskUsageCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskUsageCategory {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub path: Option<String>,
    pub exists: bool,
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPruneReport {
    pub generated_at: i64,
    pub dry_run: bool,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub deleted_files: u64,
    pub deleted_dirs: u64,
    pub deleted_bytes: u64,
    pub candidates: Vec<DiskPruneCandidate>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPruneCandidate {
    pub category: String,
    pub path: String,
    pub kind: String,
    pub bytes: u64,
    pub reason: String,
    pub removed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPruneOptions {
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
struct UsageTarget {
    id: &'static str,
    label: &'static str,
    path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct PruneTarget {
    category: &'static str,
    root: PathBuf,
    max_age_days: u64,
    reason: &'static str,
}

#[derive(Debug, Clone, Copy, Default)]
struct ScanStats {
    bytes: u64,
    files: u64,
    dirs: u64,
}

impl App {
    pub fn disk_usage_report(&self) -> Result<DiskUsageReport, BootstrapError> {
        let categories = self
            .disk_usage_targets()
            .into_iter()
            .map(scan_usage_category)
            .collect::<Vec<_>>();
        let total_bytes = categories.iter().map(|category| category.bytes).sum();

        Ok(DiskUsageReport {
            generated_at: unix_timestamp()?,
            data_dir: self.config.data_dir.display().to_string(),
            total_bytes,
            categories,
        })
    }

    pub fn disk_prune_report(
        &self,
        options: DiskPruneOptions,
    ) -> Result<DiskPruneReport, BootstrapError> {
        let now = SystemTime::now();
        let mut candidates = Vec::new();
        let mut errors = Vec::new();

        for target in self.disk_prune_targets() {
            collect_prune_candidates(&target, now, &mut candidates, &mut errors);
        }

        let candidate_bytes = candidates.iter().map(|candidate| candidate.bytes).sum();
        let candidate_count = candidates.len();
        let mut deleted_files = 0;
        let mut deleted_dirs = 0;
        let mut deleted_bytes = 0;

        if !options.dry_run {
            for candidate in &mut candidates {
                let path = PathBuf::from(&candidate.path);
                match remove_candidate(&path, candidate.kind.as_str()) {
                    Ok(()) => {
                        candidate.removed = true;
                        deleted_bytes += candidate.bytes;
                        if candidate.kind == "dir" {
                            deleted_dirs += 1;
                        } else {
                            deleted_files += 1;
                        }
                    }
                    Err(error) => {
                        errors.push(format!("{}: {}", candidate.path, error));
                    }
                }
            }
        }

        Ok(DiskPruneReport {
            generated_at: unix_timestamp()?,
            dry_run: options.dry_run,
            candidate_count,
            candidate_bytes,
            deleted_files,
            deleted_dirs,
            deleted_bytes,
            candidates,
            errors,
        })
    }

    fn disk_usage_targets(&self) -> Vec<UsageTarget> {
        let data_dir = self.config.data_dir.as_path();
        vec![
            UsageTarget {
                id: "artifacts",
                label: "Artifact payloads",
                path: Some(data_dir.join("artifacts")),
            },
            UsageTarget {
                id: "transcripts",
                label: "Transcript payloads",
                path: Some(data_dir.join("transcripts")),
            },
            UsageTarget {
                id: "archives",
                label: "Session archives",
                path: Some(data_dir.join("archives")),
            },
            UsageTarget {
                id: "runs",
                label: "Run payloads",
                path: Some(data_dir.join("runs")),
            },
            UsageTarget {
                id: "agents",
                label: "Agent profiles",
                path: Some(data_dir.join("agents")),
            },
            UsageTarget {
                id: "audit",
                label: "Audit logs",
                path: Some(data_dir.join("audit")),
            },
            UsageTarget {
                id: "legacy-sqlite",
                label: "Legacy SQLite leftovers",
                path: Some(data_dir.to_path_buf()),
            },
            UsageTarget {
                id: "workspaces",
                label: "Agent workspaces",
                path: Some(agent_workspaces_root(data_dir)),
            },
            UsageTarget {
                id: "deploy-backups",
                label: "Deploy backups",
                path: self.config.retention.deploy_backup_dir.clone(),
            },
            UsageTarget {
                id: "diagnostics",
                label: "Diagnostic bundles",
                path: self.config.retention.diagnostics_dir.clone(),
            },
        ]
    }

    fn disk_prune_targets(&self) -> Vec<PruneTarget> {
        let data_dir = self.config.data_dir.as_path();
        let retention = &self.config.retention;
        let mut targets = vec![
            PruneTarget {
                category: "audit-rotated-logs",
                root: data_dir.join("audit"),
                max_age_days: retention.audit_rotated_log_max_age_days,
                reason: "rotated audit log older than retention",
            },
            PruneTarget {
                category: "debug-bundles",
                root: data_dir.join("audit").join("debug-bundles"),
                max_age_days: retention.debug_bundle_max_age_days,
                reason: "debug bundle older than retention",
            },
            PruneTarget {
                category: "archives",
                root: data_dir.join("archives"),
                max_age_days: retention.session_archive_max_age_days,
                reason: "session archive older than retention",
            },
            PruneTarget {
                category: "legacy-sqlite",
                root: data_dir.to_path_buf(),
                max_age_days: retention.legacy_sqlite_max_age_days,
                reason: "legacy SQLite file older than retention",
            },
        ];

        let workspaces_root = agent_workspaces_root(data_dir);
        for agent_dir in list_child_dirs(&workspaces_root) {
            targets.push(PruneTarget {
                category: "workspaces-trash",
                root: agent_dir.join(".trash"),
                max_age_days: retention.workspace_trash_max_age_days,
                reason: "workspace trash entry older than retention",
            });
            targets.push(PruneTarget {
                category: "workspaces-scratch",
                root: agent_dir.join("scratch"),
                max_age_days: retention.workspace_scratch_max_age_days,
                reason: "workspace scratch entry older than retention",
            });
        }

        if let Some(path) = &retention.deploy_backup_dir {
            targets.push(PruneTarget {
                category: "deploy-backups",
                root: path.clone(),
                max_age_days: retention.deploy_backup_max_age_days,
                reason: "deploy backup older than retention",
            });
        }
        if let Some(path) = &retention.diagnostics_dir {
            targets.push(PruneTarget {
                category: "diagnostics",
                root: path.clone(),
                max_age_days: retention.diagnostics_max_age_days,
                reason: "diagnostic bundle older than retention",
            });
        }

        targets
    }
}

fn scan_usage_category(target: UsageTarget) -> DiskUsageCategory {
    let Some(path) = target.path.clone() else {
        return DiskUsageCategory {
            id: target.id.to_string(),
            label: target.label.to_string(),
            path: None,
            exists: false,
            bytes: 0,
            files: 0,
            dirs: 0,
            errors: Vec::new(),
        };
    };

    if target.id == "legacy-sqlite" {
        let mut stats = ScanStats::default();
        let mut errors = Vec::new();
        for path in legacy_sqlite_paths(&path) {
            match scan_path(&path) {
                Ok(value) => {
                    stats.bytes += value.bytes;
                    stats.files += value.files;
                    stats.dirs += value.dirs;
                }
                Err(error) => errors.push(format!("{}: {}", path.display(), error)),
            }
        }
        return usage_category_from_stats(target, Some(path), true, stats, errors);
    }

    match scan_path(&path) {
        Ok(stats) => usage_category_from_stats(target, Some(path), true, stats, Vec::new()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            usage_category_from_stats(target, Some(path), false, ScanStats::default(), Vec::new())
        }
        Err(error) => usage_category_from_stats(
            target,
            Some(path.clone()),
            path.exists(),
            ScanStats::default(),
            vec![error.to_string()],
        ),
    }
}

fn usage_category_from_stats(
    target: UsageTarget,
    path: Option<PathBuf>,
    exists: bool,
    stats: ScanStats,
    errors: Vec<String>,
) -> DiskUsageCategory {
    DiskUsageCategory {
        id: target.id.to_string(),
        label: target.label.to_string(),
        path: path.map(|path| path.display().to_string()),
        exists,
        bytes: stats.bytes,
        files: stats.files,
        dirs: stats.dirs,
        errors,
    }
}

fn scan_path(path: &Path) -> std::io::Result<ScanStats> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(ScanStats {
            bytes: metadata.len(),
            files: 1,
            dirs: 0,
        });
    }
    if metadata.is_file() {
        return Ok(ScanStats {
            bytes: metadata.len(),
            files: 1,
            dirs: 0,
        });
    }
    if metadata.is_dir() {
        let mut stats = ScanStats {
            bytes: metadata.len(),
            files: 0,
            dirs: 1,
        };
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = scan_path(&entry.path())?;
            stats.bytes += child.bytes;
            stats.files += child.files;
            stats.dirs += child.dirs;
        }
        return Ok(stats);
    }

    Ok(ScanStats::default())
}

fn collect_prune_candidates(
    target: &PruneTarget,
    now: SystemTime,
    candidates: &mut Vec<DiskPruneCandidate>,
    errors: &mut Vec<String>,
) {
    if !target.root.exists() {
        return;
    }

    if target.category == "legacy-sqlite" {
        for path in legacy_sqlite_paths(&target.root) {
            collect_single_candidate(target, now, path, candidates, errors);
        }
        return;
    }

    if target.category == "audit-rotated-logs" {
        for path in audit_rotated_log_paths(&target.root) {
            collect_single_candidate(target, now, path, candidates, errors);
        }
        return;
    }

    let entries = match fs::read_dir(&target.root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("{}: {}", target.root.display(), error));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => collect_single_candidate(target, now, entry.path(), candidates, errors),
            Err(error) => errors.push(format!("{}: {}", target.root.display(), error)),
        }
    }
}

fn collect_single_candidate(
    target: &PruneTarget,
    now: SystemTime,
    path: PathBuf,
    candidates: &mut Vec<DiskPruneCandidate>,
    errors: &mut Vec<String>,
) {
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            errors.push(format!("{}: {}", path.display(), error));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        errors.push(format!("{}: symlink pruning is skipped", path.display()));
        return;
    }
    let modified = match metadata.modified() {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("{}: {}", path.display(), error));
            return;
        }
    };
    if !is_older_than(modified, now, target.max_age_days) {
        return;
    }

    let stats = scan_path(&path).unwrap_or(ScanStats {
        bytes: metadata.len(),
        files: if metadata.is_file() { 1 } else { 0 },
        dirs: if metadata.is_dir() { 1 } else { 0 },
    });
    candidates.push(DiskPruneCandidate {
        category: target.category.to_string(),
        path: path.display().to_string(),
        kind: if metadata.is_dir() { "dir" } else { "file" }.to_string(),
        bytes: stats.bytes,
        reason: target.reason.to_string(),
        removed: false,
    });
}

fn is_older_than(modified: SystemTime, now: SystemTime, max_age_days: u64) -> bool {
    if max_age_days == 0 {
        return true;
    }
    match now.duration_since(modified) {
        Ok(age) => age >= Duration::from_secs(max_age_days.saturating_mul(86_400)),
        Err(_) => false,
    }
}

fn remove_candidate(path: &Path, kind: &str) -> std::io::Result<()> {
    if kind == "dir" {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn legacy_sqlite_paths(data_dir: &Path) -> Vec<PathBuf> {
    ["state.sqlite", "state.sqlite-wal", "state.sqlite-shm"]
        .into_iter()
        .map(|name| data_dir.join(name))
        .filter(|path| path.exists())
        .collect()
}

fn audit_rotated_log_paths(audit_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(audit_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name != "runtime.jsonl"
                        && (name.starts_with("runtime.jsonl.")
                            || name.starts_with("runtime-")
                            || name.ends_with(".jsonl.old"))
                })
        })
        .collect()
}

fn list_child_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn agent_workspaces_root(data_dir: &Path) -> PathBuf {
    data_dir
        .parent()
        .unwrap_or(data_dir)
        .join("workspaces")
        .join("agents")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_persistence::{AppConfig, RetentionConfig};

    #[test]
    fn dry_run_prune_does_not_remove_legacy_sqlite() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state");
        fs::create_dir_all(&data_dir).expect("mkdir");
        let legacy = data_dir.join("state.sqlite");
        fs::write(&legacy, b"legacy").expect("write sqlite");

        let app = crate::bootstrap::build_from_config(AppConfig {
            data_dir,
            retention: RetentionConfig {
                legacy_sqlite_max_age_days: 0,
                ..RetentionConfig::default()
            },
            ..AppConfig::default()
        })
        .expect("build app");

        let report = app
            .disk_prune_report(DiskPruneOptions { dry_run: true })
            .expect("prune");

        assert!(legacy.exists());
        assert!(report.dry_run);
        assert_eq!(report.candidate_count, 1);
        assert_eq!(report.deleted_files, 0);
    }

    #[test]
    fn execute_prune_removes_legacy_sqlite() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state");
        fs::create_dir_all(&data_dir).expect("mkdir");
        let legacy = data_dir.join("state.sqlite");
        fs::write(&legacy, b"legacy").expect("write sqlite");

        let app = crate::bootstrap::build_from_config(AppConfig {
            data_dir,
            retention: RetentionConfig {
                legacy_sqlite_max_age_days: 0,
                ..RetentionConfig::default()
            },
            ..AppConfig::default()
        })
        .expect("build app");

        let report = app
            .disk_prune_report(DiskPruneOptions { dry_run: false })
            .expect("prune");

        assert!(!legacy.exists());
        assert!(!report.dry_run);
        assert_eq!(report.candidate_count, 1);
        assert_eq!(report.deleted_files, 1);
    }
}
