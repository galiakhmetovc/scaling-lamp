use super::*;
use agent_persistence::{PersistenceScaffold, StoreError};
use rusqlite::types::ValueRef;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(super) fn migrate_sqlite_to_postgres(
    app: &App,
    sqlite_path: &str,
    database_url: Option<&str>,
    data_dir: Option<&str>,
) -> Result<String, BootstrapError> {
    let sqlite_path = PathBuf::from(sqlite_path);
    if !sqlite_path.exists() {
        return Err(BootstrapError::Usage {
            reason: format!("sqlite source does not exist: {}", sqlite_path.display()),
        });
    }

    let sqlite =
        rusqlite::Connection::open(&sqlite_path).map_err(|source| BootstrapError::Usage {
            reason: format!(
                "failed to open sqlite source {}: {source}",
                sqlite_path.display()
            ),
        })?;

    let mut config = app.config.clone();
    if let Some(database_url) = database_url {
        config.database.url = database_url.to_string();
    }
    if let Some(data_dir) = data_dir {
        config.data_dir = PathBuf::from(data_dir);
    }
    config.validate()?;

    let scaffold = PersistenceScaffold::from_config(config);
    let store = PersistenceStore::open_bootstrap_schema(&scaffold)?;
    ensure_target_empty(&store)?;

    let mut active_missions = Vec::<(String, String)>::new();
    let mut copied = BTreeMap::<&'static str, usize>::new();

    store.with_postgres_client(|client| {
        let mut transaction = client.transaction()?;
        for table in MIGRATION_TABLES {
            let count = copy_table(&sqlite, &mut transaction, table, &mut active_missions)?;
            copied.insert(table, count);
        }

        for (session_id, mission_id) in &active_missions {
            transaction.batch_execute(&format!(
                "UPDATE sessions SET active_mission_id = {} WHERE id = {}",
                sql_literal(ValueRef::Text(mission_id.as_bytes()), false),
                sql_literal(ValueRef::Text(session_id.as_bytes()), false)
            ))?;
        }

        transaction.commit()?;
        Ok(())
    })?;

    let copied_rows = copied.values().sum::<usize>();
    let table_summary = copied
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(table, count)| format!("{table}={count}"))
        .collect::<Vec<_>>()
        .join(" ");

    Ok(format!(
        "sqlite-to-postgres migration completed source={} target_data_dir={} rows={} {}",
        sqlite_path.display(),
        scaffold.config.data_dir.display(),
        copied_rows,
        table_summary
    ))
}

fn ensure_target_empty(store: &PersistenceStore) -> Result<(), BootstrapError> {
    store.with_postgres_client(|client| {
        for table in MIGRATION_TABLES {
            let count = client
                .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                .map(|row| row.get::<_, i64>(0))
                .map_err(StoreError::from)?;
            if count > 0 {
                return Err(StoreError::SchemaMismatch {
                    table,
                    reason:
                        "target PostgreSQL table is not empty; migrate into a fresh database/schema"
                            .to_string(),
                });
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn copy_table(
    sqlite: &rusqlite::Connection,
    target: &mut postgres::Transaction<'_>,
    table: &'static str,
    active_missions: &mut Vec<(String, String)>,
) -> Result<usize, StoreError> {
    if !sqlite_table_exists(sqlite, table)? {
        return Ok(0);
    }

    let target_columns = target_columns(target, table)?;
    let source_columns = sqlite_columns(sqlite, table)?;
    let selected_columns = source_columns
        .into_iter()
        .filter(|column| target_columns.contains(column))
        .collect::<Vec<_>>();
    if selected_columns.is_empty() {
        return Ok(0);
    }

    let select_sql = format!(
        "SELECT {} FROM {}",
        selected_columns
            .iter()
            .map(|column| sqlite_identifier(column))
            .collect::<Vec<_>>()
            .join(", "),
        sqlite_identifier(table)
    );
    let mut statement = sqlite.prepare(&select_sql).map_err(StoreError::from)?;
    let mut rows = statement.query([]).map_err(StoreError::from)?;
    let mut count = 0;

    while let Some(row) = rows.next().map_err(StoreError::from)? {
        if should_skip_migration_row(table, &selected_columns, row)? {
            continue;
        }

        let mut insert_columns = Vec::new();
        let mut insert_values = Vec::new();
        for (index, column) in selected_columns.iter().enumerate() {
            let value = row.get_ref(index).map_err(StoreError::from)?;
            if table == "sessions" && column == "active_mission_id" {
                if let ValueRef::Text(bytes) = value
                    && let Ok(mission_id) = std::str::from_utf8(bytes)
                {
                    let session_id = row
                        .get_ref(
                            selected_columns
                                .iter()
                                .position(|candidate| candidate == "id")
                                .unwrap_or(0),
                        )
                        .map_err(StoreError::from)
                        .and_then(sqlite_text)?;
                    active_missions.push((session_id, mission_id.to_string()));
                }
                insert_columns.push(postgres_identifier(column));
                insert_values.push("NULL".to_string());
                continue;
            }

            insert_columns.push(postgres_identifier(column));
            insert_values.push(sql_literal(value, is_bool_column(table, column)));
        }

        target.batch_execute(&format!(
            "INSERT INTO {} ({}) VALUES ({})",
            postgres_identifier(table),
            insert_columns.join(", "),
            insert_values.join(", ")
        ))?;
        count += 1;
    }

    Ok(count)
}

fn should_skip_migration_row(
    table: &str,
    selected_columns: &[String],
    row: &rusqlite::Row<'_>,
) -> Result<bool, StoreError> {
    if table != "knowledge_search_docs" {
        return Ok(false);
    }

    let Some(body_index) = selected_columns.iter().position(|column| column == "body") else {
        return Ok(false);
    };

    match row.get_ref(body_index).map_err(StoreError::from)? {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            Ok(bytes.len() > MAX_MIGRATED_KNOWLEDGE_SEARCH_DOC_BYTES)
        }
        _ => Ok(false),
    }
}

fn sqlite_table_exists(sqlite: &rusqlite::Connection, table: &str) -> Result<bool, StoreError> {
    sqlite
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(StoreError::from)
}

fn sqlite_columns(sqlite: &rusqlite::Connection, table: &str) -> Result<Vec<String>, StoreError> {
    let mut statement = sqlite
        .prepare(&format!("PRAGMA table_info({})", sqlite_identifier(table)))
        .map_err(StoreError::from)?;
    let mut rows = statement.query([]).map_err(StoreError::from)?;
    let mut columns = Vec::new();
    while let Some(row) = rows.next().map_err(StoreError::from)? {
        columns.push(row.get::<_, String>(1).map_err(StoreError::from)?);
    }
    Ok(columns)
}

fn target_columns(
    target: &mut postgres::Transaction<'_>,
    table: &str,
) -> Result<BTreeSet<String>, StoreError> {
    target
        .query(
            "SELECT column_name
             FROM information_schema.columns
             WHERE table_schema = current_schema()
               AND table_name = $1",
            &[&table],
        )
        .map(|rows| rows.iter().map(|row| row.get::<_, String>(0)).collect())
        .map_err(StoreError::from)
}

fn sqlite_text(value: ValueRef<'_>) -> Result<String, StoreError> {
    match value {
        ValueRef::Text(bytes) => Ok(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Integer(value) => Ok(value.to_string()),
        ValueRef::Null => Ok(String::new()),
        _ => Err(StoreError::InvalidIdentifier {
            id: "<sqlite-value>".to_string(),
            reason: "expected text-compatible value",
        }),
    }
}

fn sql_literal(value: ValueRef<'_>, bool_column: bool) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(value) if bool_column => {
            if value == 0 {
                "FALSE".to_string()
            } else {
                "TRUE".to_string()
            }
        }
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(bytes) if bool_column => {
            let text = String::from_utf8_lossy(bytes);
            if matches!(text.as_ref(), "1" | "true" | "TRUE" | "yes" | "YES") {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        ValueRef::Text(bytes) => {
            format!("'{}'", escape_sql_string(&String::from_utf8_lossy(bytes)))
        }
        ValueRef::Blob(bytes) => format!("decode('{}', 'hex')", hex_encode(bytes)),
    }
}

fn escape_sql_string(value: &str) -> String {
    value.replace('\'', "''")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn postgres_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn is_bool_column(table: &str, column: &str) -> bool {
    matches!(
        (table, column),
        ("agent_schedules", "enabled")
            | ("mcp_connectors", "enabled")
            | ("tool_calls", "result_truncated")
    )
}

const MIGRATION_TABLES: &[&str] = &[
    "agent_profiles",
    "sessions",
    "missions",
    "runs",
    "jobs",
    "transcripts",
    "artifacts",
    "tool_calls",
    "trace_links",
    "session_inbox_events",
    "daemon_state",
    "kv_entries",
    "agent_chain_continuations",
    "agent_schedules",
    "context_summaries",
    "context_offloads",
    "session_retention",
    "session_search_docs",
    "knowledge_sources",
    "knowledge_search_docs",
    "mcp_connectors",
    "telegram_user_pairings",
    "telegram_chat_bindings",
    "telegram_chat_statuses",
    "telegram_update_cursors",
    "plans",
    "file_delivery_requests",
];

const MAX_MIGRATED_KNOWLEDGE_SEARCH_DOC_BYTES: usize = 1_000_000;
