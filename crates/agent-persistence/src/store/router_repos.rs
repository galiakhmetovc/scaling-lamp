use super::*;

impl RouterRepository for PersistenceStore {
    fn put_router_rule(&self, record: &RouterRuleRecord) -> Result<(), StoreError> {
        validate_identifier(&record.rule_id)?;
        validate_non_empty("router rule source_filter_json", &record.source_filter_json)?;
        validate_non_empty(
            "router rule operator_filter_json",
            &record.operator_filter_json,
        )?;
        validate_non_empty("router rule condition_json", &record.condition_json)?;
        validate_non_empty("router rule route_policy_json", &record.route_policy_json)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO router_rules (
                    rule_id, priority, enabled, source_filter_json, operator_filter_json,
                    condition_json, route_policy_json, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(rule_id) DO UPDATE SET
                    priority = excluded.priority,
                    enabled = excluded.enabled,
                    source_filter_json = excluded.source_filter_json,
                    operator_filter_json = excluded.operator_filter_json,
                    condition_json = excluded.condition_json,
                    route_policy_json = excluded.route_policy_json,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.rule_id,
                    &record.priority,
                    &record.enabled,
                    &record.source_filter_json,
                    &record.operator_filter_json,
                    &record.condition_json,
                    &record.route_policy_json,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_router_rule(&self, rule_id: &str) -> Result<Option<RouterRuleRecord>, StoreError> {
        validate_identifier(rule_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT rule_id, priority, enabled, source_filter_json, operator_filter_json,
                            condition_json, route_policy_json, created_at, updated_at
                     FROM router_rules
                     WHERE rule_id = $1",
                    &[&rule_id],
                )
                .map(|row| row.map(|row| router_rule_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_enabled_router_rules(&self) -> Result<Vec<RouterRuleRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT rule_id, priority, enabled, source_filter_json, operator_filter_json,
                            condition_json, route_policy_json, created_at, updated_at
                     FROM router_rules
                     WHERE enabled = TRUE
                     ORDER BY priority ASC, updated_at DESC, rule_id ASC",
                    &[],
                )
                .map(|rows| rows.iter().map(router_rule_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn router_rule_from_row(row: &Row) -> RouterRuleRecord {
    RouterRuleRecord {
        rule_id: row.get(0),
        priority: row.get(1),
        enabled: row.get(2),
        source_filter_json: row.get(3),
        operator_filter_json: row.get(4),
        condition_json: row.get(5),
        route_policy_json: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
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
