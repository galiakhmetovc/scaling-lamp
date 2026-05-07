use super::*;

impl EventRepository for PersistenceStore {
    fn put_event_source(&self, record: &EventSourceRecord) -> Result<(), StoreError> {
        validate_identifier(&record.source_id)?;
        validate_non_empty("event source kind", &record.kind)?;
        validate_non_empty("event source address", &record.address)?;
        validate_non_empty("event source auth_policy_json", &record.auth_policy_json)?;
        validate_non_empty(
            "event source default_route_policy_json",
            &record.default_route_policy_json,
        )?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO event_sources (
                    source_id, kind, address, display_name, owner_user_id,
                    auth_policy_json, default_route_policy_json, enabled, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT(source_id) DO UPDATE SET
                    kind = excluded.kind,
                    address = excluded.address,
                    display_name = excluded.display_name,
                    owner_user_id = excluded.owner_user_id,
                    auth_policy_json = excluded.auth_policy_json,
                    default_route_policy_json = excluded.default_route_policy_json,
                    enabled = excluded.enabled,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.source_id,
                    &record.kind,
                    &record.address,
                    &record.display_name,
                    &record.owner_user_id,
                    &record.auth_policy_json,
                    &record.default_route_policy_json,
                    &record.enabled,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_event_source(&self, source_id: &str) -> Result<Option<EventSourceRecord>, StoreError> {
        validate_identifier(source_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT source_id, kind, address, display_name, owner_user_id,
                            auth_policy_json, default_route_policy_json, enabled, created_at, updated_at
                     FROM event_sources
                     WHERE source_id = $1",
                    &[&source_id],
                )
                .map(|row| row.map(|row| event_source_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn put_inbound_event(
        &self,
        record: &InboundEventRecord,
    ) -> Result<InboundEventRecord, StoreError> {
        validate_identifier(&record.event_id)?;
        validate_non_empty("inbound event dedupe_key", &record.dedupe_key)?;
        validate_non_empty("inbound event source_kind", &record.source_kind)?;
        validate_identifier(&record.source_id)?;
        validate_non_empty("inbound event payload_json", &record.payload_json)?;
        validate_non_empty("inbound event metadata_json", &record.metadata_json)?;
        validate_non_empty("inbound event status", &record.status)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO inbound_events (
                    event_id, dedupe_key, source_kind, source_id, operator_id,
                    payload_json, metadata_json, status, received_at, published_at, error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(dedupe_key) DO NOTHING",
                &[
                    &record.event_id,
                    &record.dedupe_key,
                    &record.source_kind,
                    &record.source_id,
                    &record.operator_id,
                    &record.payload_json,
                    &record.metadata_json,
                    &record.status,
                    &record.received_at,
                    &record.published_at,
                    &record.error,
                ],
            )?;
            client
                .query_one(
                    "SELECT event_id, dedupe_key, source_kind, source_id, operator_id,
                            payload_json, metadata_json, status, received_at, published_at, error
                     FROM inbound_events
                     WHERE dedupe_key = $1",
                    &[&record.dedupe_key],
                )
                .map(|row| inbound_event_from_row(&row))
                .map_err(StoreError::from)
        })
    }

    fn get_inbound_event(&self, event_id: &str) -> Result<Option<InboundEventRecord>, StoreError> {
        validate_identifier(event_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT event_id, dedupe_key, source_kind, source_id, operator_id,
                            payload_json, metadata_json, status, received_at, published_at, error
                     FROM inbound_events
                     WHERE event_id = $1",
                    &[&event_id],
                )
                .map(|row| row.map(|row| inbound_event_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn mark_inbound_event_status(
        &self,
        event_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        validate_identifier(event_id)?;
        validate_non_empty("inbound event status", status)?;
        self.with_client(|client| {
            client.execute(
                "UPDATE inbound_events
                 SET status = $2,
                     error = $3
                 WHERE event_id = $1",
                &[&event_id, &status, &error],
            )?;
            Ok(())
        })
    }

    fn put_routed_event(&self, record: &RoutedEventRecord) -> Result<(), StoreError> {
        validate_identifier(&record.routed_event_id)?;
        validate_identifier(&record.inbound_event_id)?;
        validate_identifier(&record.session_id)?;
        validate_identifier(&record.agent_id)?;
        validate_non_empty("routed event queue_policy", &record.queue_policy)?;
        validate_non_empty("routed event payload_json", &record.payload_json)?;
        validate_non_empty("routed event metadata_json", &record.metadata_json)?;
        validate_non_empty("routed event status", &record.status)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO routed_events (
                    routed_event_id, inbound_event_id, rule_id, session_id, agent_id,
                    queue_policy, priority, payload_json, metadata_json, status,
                    routed_at, published_at, error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(routed_event_id) DO UPDATE SET
                    inbound_event_id = excluded.inbound_event_id,
                    rule_id = excluded.rule_id,
                    session_id = excluded.session_id,
                    agent_id = excluded.agent_id,
                    queue_policy = excluded.queue_policy,
                    priority = excluded.priority,
                    payload_json = excluded.payload_json,
                    metadata_json = excluded.metadata_json,
                    status = excluded.status,
                    routed_at = excluded.routed_at,
                    published_at = excluded.published_at,
                    error = excluded.error",
                &[
                    &record.routed_event_id,
                    &record.inbound_event_id,
                    &record.rule_id,
                    &record.session_id,
                    &record.agent_id,
                    &record.queue_policy,
                    &record.priority,
                    &record.payload_json,
                    &record.metadata_json,
                    &record.status,
                    &record.routed_at,
                    &record.published_at,
                    &record.error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_routed_event(
        &self,
        routed_event_id: &str,
    ) -> Result<Option<RoutedEventRecord>, StoreError> {
        validate_identifier(routed_event_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT routed_event_id, inbound_event_id, rule_id, session_id, agent_id,
                            queue_policy, priority, payload_json, metadata_json, status,
                            routed_at, published_at, error
                     FROM routed_events
                     WHERE routed_event_id = $1",
                    &[&routed_event_id],
                )
                .map(|row| row.map(|row| routed_event_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn put_event_outbox(&self, record: &EventOutboxRecord) -> Result<(), StoreError> {
        validate_identifier(&record.outbox_id)?;
        validate_non_empty("event outbox subject", &record.subject)?;
        validate_non_empty("event outbox payload_json", &record.payload_json)?;
        validate_non_empty("event outbox status", &record.status)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO event_outbox (
                    outbox_id, subject, payload_json, status, attempt_count,
                    next_attempt_at, created_at, published_at, last_error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(outbox_id) DO UPDATE SET
                    subject = excluded.subject,
                    payload_json = excluded.payload_json,
                    status = excluded.status,
                    attempt_count = excluded.attempt_count,
                    next_attempt_at = excluded.next_attempt_at,
                    created_at = excluded.created_at,
                    published_at = excluded.published_at,
                    last_error = excluded.last_error",
                &[
                    &record.outbox_id,
                    &record.subject,
                    &record.payload_json,
                    &record.status,
                    &record.attempt_count,
                    &record.next_attempt_at,
                    &record.created_at,
                    &record.published_at,
                    &record.last_error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_event_outbox(&self, outbox_id: &str) -> Result<Option<EventOutboxRecord>, StoreError> {
        validate_identifier(outbox_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT outbox_id, subject, payload_json, status, attempt_count,
                            next_attempt_at, created_at, published_at, last_error
                     FROM event_outbox
                     WHERE outbox_id = $1",
                    &[&outbox_id],
                )
                .map(|row| row.map(|row| event_outbox_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn claim_pending_event_outbox(
        &self,
        limit: i64,
        now: i64,
    ) -> Result<Vec<EventOutboxRecord>, StoreError> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        self.with_client(|client| {
            client
                .query(
                    "UPDATE event_outbox
                     SET status = 'publishing',
                         attempt_count = attempt_count + 1
                     WHERE outbox_id IN (
                         SELECT outbox_id
                         FROM event_outbox
                         WHERE status = 'pending' AND next_attempt_at <= $1
                         ORDER BY next_attempt_at ASC, created_at ASC, outbox_id ASC
                         LIMIT $2
                     )
                     RETURNING outbox_id, subject, payload_json, status, attempt_count,
                               next_attempt_at, created_at, published_at, last_error",
                    &[&now, &limit],
                )
                .map(|rows| rows.iter().map(event_outbox_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn mark_event_outbox_published(
        &self,
        outbox_id: &str,
        published_at: i64,
    ) -> Result<(), StoreError> {
        validate_identifier(outbox_id)?;
        self.with_client(|client| {
            client.execute(
                "UPDATE event_outbox
                 SET status = 'published',
                     published_at = $2,
                     last_error = NULL
                 WHERE outbox_id = $1",
                &[&outbox_id, &published_at],
            )?;
            Ok(())
        })
    }

    fn mark_event_outbox_pending_retry(
        &self,
        outbox_id: &str,
        next_attempt_at: i64,
        error: &str,
    ) -> Result<(), StoreError> {
        validate_identifier(outbox_id)?;
        self.with_client(|client| {
            client.execute(
                "UPDATE event_outbox
                 SET status = 'pending',
                     next_attempt_at = $2,
                     last_error = $3
                 WHERE outbox_id = $1",
                &[&outbox_id, &next_attempt_at, &error],
            )?;
            Ok(())
        })
    }

    fn mark_event_outbox_failed(&self, outbox_id: &str, error: &str) -> Result<(), StoreError> {
        validate_identifier(outbox_id)?;
        self.with_client(|client| {
            client.execute(
                "UPDATE event_outbox
                 SET status = 'failed',
                     last_error = $2
                 WHERE outbox_id = $1",
                &[&outbox_id, &error],
            )?;
            Ok(())
        })
    }

    fn put_event_delivery(&self, record: &EventDeliveryRecord) -> Result<(), StoreError> {
        validate_identifier(&record.delivery_event_id)?;
        validate_identifier(&record.source_event_id)?;
        validate_identifier(&record.target_id)?;
        validate_non_empty("event delivery status", &record.status)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO event_deliveries (
                    delivery_event_id, source_event_id, target_id, status, attempt_count,
                    created_at, updated_at, delivered_at, last_error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(delivery_event_id) DO UPDATE SET
                    source_event_id = excluded.source_event_id,
                    target_id = excluded.target_id,
                    status = excluded.status,
                    attempt_count = excluded.attempt_count,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    delivered_at = excluded.delivered_at,
                    last_error = excluded.last_error",
                &[
                    &record.delivery_event_id,
                    &record.source_event_id,
                    &record.target_id,
                    &record.status,
                    &record.attempt_count,
                    &record.created_at,
                    &record.updated_at,
                    &record.delivered_at,
                    &record.last_error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_event_delivery(
        &self,
        delivery_event_id: &str,
    ) -> Result<Option<EventDeliveryRecord>, StoreError> {
        validate_identifier(delivery_event_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT delivery_event_id, source_event_id, target_id, status, attempt_count,
                            created_at, updated_at, delivered_at, last_error
                     FROM event_deliveries
                     WHERE delivery_event_id = $1",
                    &[&delivery_event_id],
                )
                .map(|row| row.map(|row| event_delivery_from_row(&row)))
                .map_err(StoreError::from)
        })
    }
}

fn event_source_from_row(row: &Row) -> EventSourceRecord {
    EventSourceRecord {
        source_id: row.get(0),
        kind: row.get(1),
        address: row.get(2),
        display_name: row.get(3),
        owner_user_id: row.get(4),
        auth_policy_json: row.get(5),
        default_route_policy_json: row.get(6),
        enabled: row.get(7),
        created_at: row.get(8),
        updated_at: row.get(9),
    }
}

fn inbound_event_from_row(row: &Row) -> InboundEventRecord {
    InboundEventRecord {
        event_id: row.get(0),
        dedupe_key: row.get(1),
        source_kind: row.get(2),
        source_id: row.get(3),
        operator_id: row.get(4),
        payload_json: row.get(5),
        metadata_json: row.get(6),
        status: row.get(7),
        received_at: row.get(8),
        published_at: row.get(9),
        error: row.get(10),
    }
}

fn routed_event_from_row(row: &Row) -> RoutedEventRecord {
    RoutedEventRecord {
        routed_event_id: row.get(0),
        inbound_event_id: row.get(1),
        rule_id: row.get(2),
        session_id: row.get(3),
        agent_id: row.get(4),
        queue_policy: row.get(5),
        priority: row.get(6),
        payload_json: row.get(7),
        metadata_json: row.get(8),
        status: row.get(9),
        routed_at: row.get(10),
        published_at: row.get(11),
        error: row.get(12),
    }
}

fn event_outbox_from_row(row: &Row) -> EventOutboxRecord {
    EventOutboxRecord {
        outbox_id: row.get(0),
        subject: row.get(1),
        payload_json: row.get(2),
        status: row.get(3),
        attempt_count: row.get(4),
        next_attempt_at: row.get(5),
        created_at: row.get(6),
        published_at: row.get(7),
        last_error: row.get(8),
    }
}

fn event_delivery_from_row(row: &Row) -> EventDeliveryRecord {
    EventDeliveryRecord {
        delivery_event_id: row.get(0),
        source_event_id: row.get(1),
        target_id: row.get(2),
        status: row.get(3),
        attempt_count: row.get(4),
        created_at: row.get(5),
        updated_at: row.get(6),
        delivered_at: row.get(7),
        last_error: row.get(8),
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
