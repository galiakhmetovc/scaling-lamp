use agent_persistence::StoreError;
use rusqlite::{Error as SqliteError, ErrorCode};
use std::future::Future;
use std::time::Duration;

pub const SQLITE_LOCK_RETRY_ATTEMPTS: usize = 4;
pub const SQLITE_LOCK_RETRY_DELAY_MS: u64 = 250;

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
        SQLITE_LOCK_RETRY_ATTEMPTS, SQLITE_LOCK_RETRY_DELAY_MS, is_transient_sqlite_lock,
        retry_store_async, retry_store_sync,
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
        let result =
            retry_store_sync(
                SQLITE_LOCK_RETRY_ATTEMPTS,
                Duration::from_millis(0),
                || match attempts.fetch_add(1, Ordering::SeqCst) {
                    0 | 1 => Err(sqlite_failure(ErrorCode::DatabaseBusy)),
                    _ => Ok("ok"),
                },
            )
            .expect("retry succeeds");

        assert_eq!(result, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(SQLITE_LOCK_RETRY_DELAY_MS, 250);
    }

    #[tokio::test]
    async fn retries_async_store_operation_until_lock_clears() {
        let attempts = AtomicUsize::new(0);
        let result = retry_store_async(
            SQLITE_LOCK_RETRY_ATTEMPTS,
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
        let error =
            retry_store_sync::<(), _>(SQLITE_LOCK_RETRY_ATTEMPTS, Duration::from_millis(0), || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), StoreError>(sqlite_failure(ErrorCode::ReadOnly))
            })
            .expect_err("read-only failure should escape");

        assert!(!is_transient_sqlite_lock(&error));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
