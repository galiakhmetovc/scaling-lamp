use super::*;

#[test]
fn telegram_repository_round_trips_pairings_bindings_and_update_cursor() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });

    let pairing = TelegramUserPairingRecord {
        token: "pair-123".to_string(),
        telegram_user_id: 42,
        telegram_chat_id: 42,
        telegram_username: Some("alice".to_string()),
        telegram_display_name: "Alice".to_string(),
        status: "pending".to_string(),
        created_at: 100,
        expires_at: 1000,
        activated_at: None,
    };
    let binding = TelegramChatBindingRecord {
        telegram_chat_id: 42,
        scope: "private".to_string(),
        owner_telegram_user_id: Some(42),
        selected_session_id: Some("session-telegram-1".to_string()),
        default_agent_profile_id: Some("judge".to_string()),
        last_delivered_transcript_created_at: Some(115),
        last_delivered_transcript_id: Some("transcript-telegram-1".to_string()),
        inbound_queue_mode: "coalesce".to_string(),
        inbound_coalesce_window_ms: None,
        created_at: 110,
        updated_at: 120,
    };
    let cursor = TelegramUpdateCursorRecord {
        consumer: "telegram-long-poll".to_string(),
        update_id: 501,
        updated_at: 130,
    };
    let status = TelegramChatStatusRecord {
        telegram_chat_id: 42,
        message_id: 9001,
        state: "stale".to_string(),
        expires_at: Some(1800),
        created_at: 125,
        updated_at: 126,
    };

    {
        let store = super::super::PersistenceStore::open(&scaffold).expect("open store");
        store
            .put_telegram_user_pairing(&pairing)
            .expect("store pairing");
        store
            .put_telegram_chat_binding(&binding)
            .expect("store binding");
        store
            .put_telegram_chat_status(&status)
            .expect("store status");
        store
            .put_telegram_update_cursor(&cursor)
            .expect("store cursor");
    }

    let reopened = super::super::PersistenceStore::open(&scaffold).expect("reopen store");

    assert_eq!(
        reopened
            .get_telegram_user_pairing_by_token("pair-123")
            .expect("get pairing by token"),
        Some(pairing.clone())
    );
    assert_eq!(
        reopened
            .get_telegram_user_pairing_by_user_id(42)
            .expect("get pairing by user id"),
        Some(pairing)
    );
    assert_eq!(
        reopened
            .get_telegram_chat_binding(42)
            .expect("get chat binding"),
        Some(binding.clone())
    );
    assert_eq!(
        reopened
            .list_telegram_chat_bindings()
            .expect("list chat bindings"),
        vec![binding]
    );
    assert_eq!(
        reopened
            .get_telegram_chat_status(42)
            .expect("get chat status"),
        Some(status.clone())
    );
    assert_eq!(
        reopened
            .list_telegram_chat_statuses()
            .expect("list chat statuses"),
        vec![status.clone()]
    );
    assert!(
        reopened
            .delete_telegram_chat_status(42)
            .expect("delete chat status")
    );
    assert_eq!(
        reopened
            .get_telegram_chat_status(42)
            .expect("status removed"),
        None
    );
    assert_eq!(
        reopened
            .get_telegram_update_cursor("telegram-long-poll")
            .expect("get update cursor"),
        Some(cursor)
    );
}

#[test]
fn put_telegram_user_pairing_serializes_concurrent_replacements() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let _bootstrap = super::super::PersistenceStore::open(&scaffold).expect("bootstrap store");

    let barrier = Arc::new(Barrier::new(3));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();

    for worker_id in 0..2 {
        let scaffold_clone = scaffold.clone();
        let barrier_clone = Arc::clone(&barrier);
        let tx_clone = tx.clone();
        handles.push(thread::spawn(move || {
            let store = super::super::PersistenceStore::open_runtime(&scaffold_clone)
                .expect("open runtime");
            barrier_clone.wait();
            let result = (0..100).try_for_each(|attempt| {
                store.put_telegram_user_pairing(&TelegramUserPairingRecord {
                    token: format!("pair-{worker_id}-{attempt}"),
                    telegram_user_id: 42,
                    telegram_chat_id: 4200 + worker_id as i64,
                    telegram_username: Some("alice".to_string()),
                    telegram_display_name: format!("Alice worker {worker_id}"),
                    status: "pending".to_string(),
                    created_at: attempt,
                    expires_at: 10_000 + attempt,
                    activated_at: None,
                })
            });
            tx_clone.send(result).expect("send pairing result");
        }));
    }
    drop(tx);

    barrier.wait();

    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("receive pairing result")
            .expect("concurrent telegram pairing replacement");
    }

    for handle in handles {
        handle.join().expect("join pairing thread");
    }

    let reopened = super::super::PersistenceStore::open_runtime(&scaffold).expect("reopen store");
    let pairings = reopened
        .list_telegram_user_pairings()
        .expect("list telegram pairings");
    assert_eq!(pairings.len(), 1);
    assert_eq!(pairings[0].telegram_user_id, 42);
}
