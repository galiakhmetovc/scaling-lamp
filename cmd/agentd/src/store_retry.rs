use agent_persistence::StoreError;
use postgres::error::SqlState;
use std::future::Future;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

pub const DEFAULT_STORE_RETRY_ATTEMPTS: usize = 4;
pub const DEFAULT_STORE_RETRY_DELAY_MS: u64 = 250;

static STORE_RETRY_ATTEMPTS: AtomicUsize = AtomicUsize::new(DEFAULT_STORE_RETRY_ATTEMPTS);
static STORE_RETRY_DELAY_MS: AtomicU64 = AtomicU64::new(DEFAULT_STORE_RETRY_DELAY_MS);

pub fn configure_store_retry(attempts: usize, delay_ms: u64) {
    STORE_RETRY_ATTEMPTS.store(attempts.max(1), Ordering::Relaxed);
    STORE_RETRY_DELAY_MS.store(delay_ms.max(1), Ordering::Relaxed);
}

pub fn store_retry_attempts() -> usize {
    STORE_RETRY_ATTEMPTS.load(Ordering::Relaxed).max(1)
}

pub fn store_retry_delay() -> Duration {
    Duration::from_millis(STORE_RETRY_DELAY_MS.load(Ordering::Relaxed).max(1))
}

pub fn is_transient_store_error(error: &StoreError) -> bool {
    match error {
        StoreError::Postgres(source) => is_transient_postgres_sqlstate(source.code()),
        _ => false,
    }
}

fn is_transient_postgres_sqlstate(code: Option<&SqlState>) -> bool {
    matches!(
        code,
        Some(&SqlState::T_R_SERIALIZATION_FAILURE)
            | Some(&SqlState::T_R_DEADLOCK_DETECTED)
            | Some(&SqlState::LOCK_NOT_AVAILABLE)
            | Some(&SqlState::CONNECTION_EXCEPTION)
            | Some(&SqlState::CONNECTION_DOES_NOT_EXIST)
            | Some(&SqlState::CONNECTION_FAILURE)
            | Some(&SqlState::SQLCLIENT_UNABLE_TO_ESTABLISH_SQLCONNECTION)
            | Some(&SqlState::SQLSERVER_REJECTED_ESTABLISHMENT_OF_SQLCONNECTION)
            | Some(&SqlState::TRANSACTION_RESOLUTION_UNKNOWN)
            | Some(&SqlState::PROTOCOL_VIOLATION)
    )
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
            Err(error) if is_transient_store_error(&error) && remaining_attempts > 1 => {
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
            Err(error) if is_transient_store_error(&error) && remaining_attempts > 1 => {
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
        DEFAULT_STORE_RETRY_ATTEMPTS, DEFAULT_STORE_RETRY_DELAY_MS, is_transient_postgres_sqlstate,
        is_transient_store_error, retry_store_async, retry_store_sync,
    };
    use agent_persistence::StoreError;
    use postgres::error::SqlState;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn classifies_postgres_retryable_sqlstates_as_transient() {
        assert!(is_transient_postgres_sqlstate(Some(
            &SqlState::T_R_SERIALIZATION_FAILURE
        )));
        assert!(is_transient_postgres_sqlstate(Some(
            &SqlState::T_R_DEADLOCK_DETECTED
        )));
        assert!(is_transient_postgres_sqlstate(Some(
            &SqlState::LOCK_NOT_AVAILABLE
        )));
        assert!(!is_transient_postgres_sqlstate(Some(
            &SqlState::UNIQUE_VIOLATION
        )));
    }

    #[test]
    fn retries_sync_store_operation_until_transient_error_clears() {
        let attempts = AtomicUsize::new(0);
        let result = retry_store_sync(
            DEFAULT_STORE_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || match attempts.fetch_add(1, Ordering::SeqCst) {
                0 | 1 => Err(StoreError::StoreLockPoisoned),
                _ => Ok("ok"),
            },
        )
        .expect_err("lock poisoning is not transient");

        assert!(!is_transient_store_error(&result));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(DEFAULT_STORE_RETRY_DELAY_MS, 250);
    }

    #[tokio::test]
    async fn returns_success_without_retry_for_successful_async_operation() {
        let attempts = AtomicUsize::new(0);
        let result = retry_store_async(
            DEFAULT_STORE_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok("ok")
            },
        )
        .await
        .expect("operation succeeds");

        assert_eq!(result, "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn does_not_retry_non_transient_errors() {
        let attempts = AtomicUsize::new(0);
        let error = retry_store_sync::<(), _>(
            DEFAULT_STORE_RETRY_ATTEMPTS,
            Duration::from_millis(0),
            || {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err::<(), StoreError>(StoreError::InvalidIdentifier {
                    id: "bad/id".to_string(),
                    reason: "test",
                })
            },
        )
        .expect_err("non-transient failure should escape");

        assert!(!is_transient_store_error(&error));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
