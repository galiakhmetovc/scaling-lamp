use super::*;
use crate::records::{DeliveryTargetRecord, SessionOutputRouteRecord};

impl DeliveryRepository for PersistenceStore {
    fn put_delivery_target(&self, record: &DeliveryTargetRecord) -> Result<(), StoreError> {
        validate_identifier(&record.target_id)?;
        validate_delivery_target(record)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO delivery_targets (
                    target_id, kind, address, scope, owner_user_id,
                    allowed_agent_ids_json, allowed_session_ids_json, send_policy_json,
                    format_policy, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(target_id) DO UPDATE SET
                    kind = excluded.kind,
                    address = excluded.address,
                    scope = excluded.scope,
                    owner_user_id = excluded.owner_user_id,
                    allowed_agent_ids_json = excluded.allowed_agent_ids_json,
                    allowed_session_ids_json = excluded.allowed_session_ids_json,
                    send_policy_json = excluded.send_policy_json,
                    format_policy = excluded.format_policy,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.target_id,
                    &record.kind,
                    &record.address,
                    &record.scope,
                    &record.owner_user_id,
                    &record.allowed_agent_ids_json,
                    &record.allowed_session_ids_json,
                    &record.send_policy_json,
                    &record.format_policy,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_delivery_target(
        &self,
        target_id: &str,
    ) -> Result<Option<DeliveryTargetRecord>, StoreError> {
        validate_identifier(target_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT target_id, kind, address, scope, owner_user_id,
                            allowed_agent_ids_json, allowed_session_ids_json, send_policy_json,
                            format_policy, created_at, updated_at
                     FROM delivery_targets
                     WHERE target_id = $1",
                    &[&target_id],
                )
                .map(|row| row.map(|row| delivery_target_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_delivery_targets(&self) -> Result<Vec<DeliveryTargetRecord>, StoreError> {
        self.query_delivery_targets(
            "SELECT target_id, kind, address, scope, owner_user_id,
                    allowed_agent_ids_json, allowed_session_ids_json, send_policy_json,
                    format_policy, created_at, updated_at
             FROM delivery_targets
             ORDER BY target_id ASC",
            &[],
        )
    }

    fn put_session_output_route(
        &self,
        record: &SessionOutputRouteRecord,
    ) -> Result<(), StoreError> {
        validate_identifier(&record.route_id)?;
        validate_identifier(&record.session_id)?;
        validate_identifier(&record.target_id)?;
        validate_output_route(record)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO session_output_routes (
                    route_id, session_id, target_id, filter_json, format_policy, enabled,
                    last_delivered_transcript_created_at, last_delivered_transcript_id,
                    created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT(route_id) DO UPDATE SET
                    session_id = excluded.session_id,
                    target_id = excluded.target_id,
                    filter_json = excluded.filter_json,
                    format_policy = excluded.format_policy,
                    enabled = excluded.enabled,
                    last_delivered_transcript_created_at = excluded.last_delivered_transcript_created_at,
                    last_delivered_transcript_id = excluded.last_delivered_transcript_id,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.route_id,
                    &record.session_id,
                    &record.target_id,
                    &record.filter_json,
                    &record.format_policy,
                    &record.enabled,
                    &record.last_delivered_transcript_created_at,
                    &record.last_delivered_transcript_id,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_session_output_route(
        &self,
        route_id: &str,
    ) -> Result<Option<SessionOutputRouteRecord>, StoreError> {
        validate_identifier(route_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT route_id, session_id, target_id, filter_json, format_policy, enabled,
                            last_delivered_transcript_created_at, last_delivered_transcript_id,
                            created_at, updated_at
                     FROM session_output_routes
                     WHERE route_id = $1",
                    &[&route_id],
                )
                .map(|row| row.map(|row| session_output_route_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_enabled_session_output_routes(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionOutputRouteRecord>, StoreError> {
        validate_identifier(session_id)?;
        self.query_session_output_routes(
            "SELECT route_id, session_id, target_id, filter_json, format_policy, enabled,
                    last_delivered_transcript_created_at, last_delivered_transcript_id,
                    created_at, updated_at
             FROM session_output_routes
             WHERE session_id = $1 AND enabled = TRUE
             ORDER BY updated_at ASC, route_id ASC",
            &[&session_id],
        )
    }

    fn list_enabled_session_output_routes_for_target_kind(
        &self,
        target_kind: &str,
    ) -> Result<Vec<SessionOutputRouteRecord>, StoreError> {
        validate_non_empty("delivery target kind", target_kind)?;
        self.query_session_output_routes(
            "SELECT r.route_id, r.session_id, r.target_id, r.filter_json, r.format_policy, r.enabled,
                    r.last_delivered_transcript_created_at, r.last_delivered_transcript_id,
                    r.created_at, r.updated_at
             FROM session_output_routes r
             JOIN delivery_targets t ON t.target_id = r.target_id
             WHERE r.enabled = TRUE AND t.kind = $1
             ORDER BY r.updated_at ASC, r.route_id ASC",
            &[&target_kind],
        )
    }
}

impl PersistenceStore {
    fn query_delivery_targets(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<DeliveryTargetRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(delivery_target_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_session_output_routes(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<SessionOutputRouteRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(session_output_route_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn delivery_target_from_row(row: &Row) -> DeliveryTargetRecord {
    DeliveryTargetRecord {
        target_id: row.get(0),
        kind: row.get(1),
        address: row.get(2),
        scope: row.get(3),
        owner_user_id: row.get(4),
        allowed_agent_ids_json: row.get(5),
        allowed_session_ids_json: row.get(6),
        send_policy_json: row.get(7),
        format_policy: row.get(8),
        created_at: row.get(9),
        updated_at: row.get(10),
    }
}

fn session_output_route_from_row(row: &Row) -> SessionOutputRouteRecord {
    SessionOutputRouteRecord {
        route_id: row.get(0),
        session_id: row.get(1),
        target_id: row.get(2),
        filter_json: row.get(3),
        format_policy: row.get(4),
        enabled: row.get(5),
        last_delivered_transcript_created_at: row.get(6),
        last_delivered_transcript_id: row.get(7),
        created_at: row.get(8),
        updated_at: row.get(9),
    }
}

fn validate_delivery_target(record: &DeliveryTargetRecord) -> Result<(), StoreError> {
    validate_non_empty("delivery target kind", &record.kind)?;
    validate_non_empty("delivery target address", &record.address)?;
    validate_non_empty("delivery target scope", &record.scope)?;
    validate_format_policy(&record.format_policy)
}

fn validate_output_route(record: &SessionOutputRouteRecord) -> Result<(), StoreError> {
    validate_non_empty("session output route filter_json", &record.filter_json)?;
    validate_format_policy(&record.format_policy)
}

fn validate_format_policy(value: &str) -> Result<(), StoreError> {
    match value {
        "full_text" | "summary" | "status_only" | "errors_only" => Ok(()),
        _ => Err(StoreError::InvalidIdentifier {
            id: value.to_string(),
            reason: "delivery format_policy must be one of full_text, summary, status_only, errors_only",
        }),
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::InvalidIdentifier {
            id: value.to_string(),
            reason: field,
        });
    }
    Ok(())
}
