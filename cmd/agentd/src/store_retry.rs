use agent_persistence::StoreError;
use rusqlite::{Error as SqliteError, ErrorCode};
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

pub const DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS: usize = 4;
pub const DEFAULT_SQLITE_LOCK_RETRY_DELAY_MS: u64 = 250;

static SQLITE_LOCK_RETRY_ATTEMPTS: AtomicUsize =
    AtomicUsize::new(DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS);
static SQLITE_LOCK_RETRY_DELAY_MS: AtomicU64 = AtomicU64::new(DEFAULT_SQLITE_LOCK_RETRY_DELAY_MS);

pub fn configure_sqlite_lock_retry(attempts: usize, delay_ms: u64) {
    SQLITE_LOCK_RETRY_ATTEMPTS.store(attempts.max(1), Ordering::Relaxed);
    SQLITE_LOCK_RETRY_DELAY_MS.store(delay_ms.max(1), Ordering::Relaxed);
}

pub fn sqlite_lock_retry_attempts() -> usize {
    SQLITE_LOCK_RETRY_ATTEMPTS.load(Ordering::Relaxed).max(1)
}

pub fn sqlite_lock_retry_delay() -> Duration {
    Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS.load(Ordering::Relaxed).max(1))
}

pub fn is_transient_sqlite_lock(error: &StoreError) -> bool {
    match error {
        StoreError::Sqlite(SqliteError::SqliteFailure(code, _)) => matches!(
            code.code,
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
        ),
        _ => false,
    }
}

pub fn retry_store_sync<T, F>(
    attempts: usize,
    base_delay: Duration,
    mut operation: F,
) -> Result<T, StoreError>
where
    F: FnMut() -> Result<T, StoreError>,
{
    let mut remaining_attempts = attempts.max(1);
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if is_transient_sqlite_lock(&error) && remaining_attempts > 1 => {
                remaining_attempts -= 1;
                std::thread::sleep(base_delay);
            }
            Err(error) => return Err(error),
        }
    }
}

pub async fn retry_store_async<T, F, Fut>(
    attempts: usize,
    base_delay: Duration,
    mut operation: F,
) -> Result<T, StoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, StoreError>>,
{
    let mut remaining_attempts = attempts.max(1);
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if is_transient_sqlite_lock(&error) && remaining_attempts > 1 => {
                remaining_attempts -= 1;
                tokio::time::sleep(base_delay).await;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS, DEFAULT_SQLITE_LOCK_RETRY_DELAY_MS,
        is_transient_sqlite_lock, retry_store_async, retry_store_sync,
    };
    use agent_persistence::StoreError;
    use rusqlite::{Error as SqliteError, ErrorCode, ffi};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn sqlite_failure(code: ErrorCode) -> StoreError {
        StoreError::Sqlite(SqliteError::SqliteFailure(
            ffi::Error {
                code,
                extended_code: match code {
                    ErrorCode::DatabaseBusy => ffi::SQLITE_BUSY,
                    ErrorCode::DatabaseLocked => ffi::SQLITE_LOCKED,
                    _ => ffi::SQLITE_ERROR,
                },
            },
            Some("synthetic sqlite failure".to_string()),
        ))
    }

    #[test]
    fn classifies_busy_and_locked_as_transient() {
        assert!(is_transient_sqlite_lock(&sqlite_failure(
            ErrorCode::DatabaseBusy
        )));
        assert!(is_transient_sqlite_lock(&sqlite_failure(
            ErrorCode::DatabaseLocked
        )));
        assert!(!is_transient_sqlite_lock(&sqlite_failure(
            ErrorCode::ReadOnly
        )));
    }

    #[test]
    fn retries_sync_store_operation_until_lock_clears() {
        let attempts = AtomicUsize::new(0);
        let result = retry_store_sync(
            DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || match attempts.fetch_add(1, Ordering::SeqCst) {
                0 | 1 => Err(sqlite_failure(ErrorCode::DatabaseBusy)),
                _ => Ok("ok"),
            },
        )
        .expect("retry succeeds");

        assert_eq!(result, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(DEFAULT_SQLITE_LOCK_RETRY_DELAY_MS, 250);
    }

    #[tokio::test]
    async fn retries_async_store_operation_until_lock_clears() {
        let attempts = AtomicUsize::new(0);
        let result = retry_store_async(
            DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || async {
                match attempts.fetch_add(1, Ordering::SeqCst) {
                    0 => Err(sqlite_failure(ErrorCode::DatabaseLocked)),
                    _ => Ok("ok"),
                }
            },
        )
        .await
        .expect("retry succeeds");

        assert_eq!(result, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn does_not_retry_non_transient_errors() {
        let attempts = AtomicUsize::new(0);
        let error = retry_store_sync::<(), _>(
            DEFAULT_SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), StoreError>(sqlite_failure(ErrorCode::ReadOnly))
            },
        )
        .expect_err("read-only failure should escape");

        assert!(!is_transient_sqlite_lock(&error));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
