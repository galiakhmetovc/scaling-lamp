use super::*;

pub(super) fn prepare_layout(layout: &StoreLayout) -> Result<(), StoreError> {
    create_directory(
        layout
            .metadata_db
            .parent()
            .unwrap_or(layout.metadata_db.as_path()),
    )?;
    create_directory(&layout.runs_dir)?;
    create_directory(&layout.transcripts_dir)?;
    create_directory(&layout.artifacts_dir)?;
    create_directory(&layout.archives_dir)?;
    Ok(())
}

pub(super) fn reconcile_directory(
    connection: &Connection,
    query: &str,
    directory: &Path,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(query)?;
    let mut rows = statement.query([])?;
    let mut expected = std::collections::BTreeMap::new();

    while let Some(row) = rows.next()? {
        let stored_path: String = row.get(0)?;
        let byte_len: i64 = row.get(1)?;
        let sha256: String = row.get(2)?;
        let file_name = PathBuf::from(stored_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);

        if let Some(file_name) = file_name {
            expected.insert(file_name, (byte_len as u64, sha256));
        }
    }

    if !directory.exists() {
        return Ok(());
    }

    restore_backups(directory, &expected)?;

    for entry in fs::read_dir(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| StoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".bak") || name.ends_with(".pending"))
        {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let should_remove = match expected.get(file_name) {
            Some((expected_len, expected_sha256)) => {
                let (actual_len, actual_sha256) = payload_fingerprint(&path)?;
                actual_len != *expected_len || actual_sha256 != *expected_sha256
            }
            None => true,
        };

        if should_remove {
            fs::remove_file(&path).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
        }
    }

    Ok(())
}

pub(super) fn persist_payload_with_commit<F>(
    path: &Path,
    bytes: &[u8],
    commit: F,
) -> Result<(), StoreError>
where
    F: FnOnce() -> Result<(), StoreError>,
{
    if let Some(parent) = path.parent() {
        create_directory(parent)?;
    }
    let temp_path = pending_path(path);
    let backup_path = backup_path(path);
    let had_existing = path.exists();

    write_temp_payload(&temp_path, bytes)?;

    if had_existing {
        fs::rename(path, &backup_path).map_err(|source| StoreError::Io {
            path: backup_path.clone(),
            source,
        })?;
    }

    match commit() {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                create_directory(parent)?;
            }
            fs::rename(&temp_path, path).map_err(|source| StoreError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            if had_existing && backup_path.exists() {
                fs::remove_file(&backup_path).map_err(|source| StoreError::Io {
                    path: backup_path,
                    source,
                })?;
            }
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = fs::remove_file(&temp_path);
                if backup_path.exists() {
                    let _ = fs::rename(&backup_path, path);
                }
            } else {
                let _ = fs::remove_file(&temp_path);
            }
            Err(error)
        }
    }
}

pub(super) fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.to_string_lossy()))
}

pub(super) fn pending_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.pending", path.to_string_lossy()))
}

pub(super) fn remove_payload_if_exists(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn read_string_payload(path: &Path) -> Result<String, StoreError> {
    read_payload_with_stage_fallback(path, |candidate| fs::read_to_string(candidate))
}

pub(super) fn read_binary_payload(path: &Path) -> Result<Vec<u8>, StoreError> {
    read_payload_with_stage_fallback(path, |candidate| fs::read(candidate))
}

pub(super) fn validate_integrity(
    path: &Path,
    actual_len: u64,
    bytes: &[u8],
    expected_len: u64,
    expected_sha256: &str,
) -> Result<(), StoreError> {
    let actual_sha256 = sha256_hex(bytes);

    if actual_len != expected_len || actual_sha256 != expected_sha256 {
        return Err(StoreError::IntegrityMismatch {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

fn read_payload_with_stage_fallback<T, F>(path: &Path, read: F) -> Result<T, StoreError>
where
    F: for<'a> Fn(&'a Path) -> Result<T, std::io::Error>,
{
    match read(path) {
        Ok(payload) => Ok(payload),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            for staged_path in [pending_path(path), backup_path(path)] {
                match read(&staged_path) {
                    Ok(payload) => return Ok(payload),
                    Err(stage_error) if stage_error.kind() == std::io::ErrorKind::NotFound => {
                        continue;
                    }
                    Err(stage_error) => {
                        return Err(StoreError::Io {
                            path: staged_path,
                            source: stage_error,
                        });
                    }
                }
            }

            Err(StoreError::MissingPayload {
                path: path.to_path_buf(),
            })
        }
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }

    encoded
}

pub(super) fn create_directory(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn write_temp_payload(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    fs::write(path, bytes).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn payload_fingerprint(path: &Path) -> Result<(u64, String), StoreError> {
    let bytes = read_binary_payload(path)?;
    Ok((bytes.len() as u64, sha256_hex(&bytes)))
}

pub(super) fn restore_backups(
    directory: &Path,
    expected: &std::collections::BTreeMap<String, (u64, String)>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| StoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let (original_name, is_backup) = if let Some(original_name) = file_name.strip_suffix(".bak")
        {
            (original_name, true)
        } else if let Some(original_name) = file_name.strip_suffix(".pending") {
            (original_name, false)
        } else {
            continue;
        };
        let Some((expected_len, expected_sha256)) = expected.get(original_name) else {
            if is_backup {
                fs::remove_file(&path).map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
            }
            continue;
        };

        let original_path = directory.join(original_name);
        let staged_payload_matches = payload_fingerprint(&path)
            .map(|(len, sha256)| len == *expected_len && sha256 == *expected_sha256)
            .unwrap_or(false);

        if original_path.exists() {
            let original_matches = payload_fingerprint(&original_path)
                .map(|(len, sha256)| len == *expected_len && sha256 == *expected_sha256)
                .unwrap_or(false);

            if original_matches {
                fs::remove_file(&path).map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
                continue;
            }

            fs::remove_file(&original_path).map_err(|source| StoreError::Io {
                path: original_path.clone(),
                source,
            })?;
        }

        if staged_payload_matches {
            fs::rename(&path, &original_path).map_err(|source| StoreError::Io {
                path: original_path,
                source,
            })?;
        } else {
            fs::remove_file(&path).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
        }
    }

    Ok(())
}
