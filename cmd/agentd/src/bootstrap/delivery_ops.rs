use super::{App, BootstrapError, unix_timestamp};
use agent_persistence::{
    DeliveryRepository, DeliveryTargetRecord, SessionOutputRouteRecord, TranscriptRepository,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryTargetCreateOptions {
    pub kind: String,
    pub address: String,
    pub scope: String,
    pub owner_user_id: Option<String>,
    pub allowed_agent_ids: Vec<String>,
    pub allowed_session_ids: Vec<String>,
    pub send_policy_json: String,
    pub format_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryTargetView {
    pub id: String,
    pub kind: String,
    pub address: String,
    pub scope: String,
    pub owner_user_id: Option<String>,
    pub allowed_agent_ids: Vec<String>,
    pub allowed_session_ids: Vec<String>,
    pub send_policy_json: String,
    pub format_policy: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutputRouteCreateOptions {
    pub route_id: Option<String>,
    pub filter_json: String,
    pub format_policy: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutputRouteView {
    pub id: String,
    pub session_id: String,
    pub target_id: String,
    pub filter_json: String,
    pub format_policy: String,
    pub enabled: bool,
    pub last_delivered_transcript_created_at: Option<i64>,
    pub last_delivered_transcript_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl App {
    pub fn create_delivery_target(
        &self,
        target_id: &str,
        options: DeliveryTargetCreateOptions,
    ) -> Result<DeliveryTargetView, BootstrapError> {
        validate_non_blank("delivery target id", target_id)?;
        validate_non_blank("delivery target kind", &options.kind)?;
        validate_non_blank("delivery target address", &options.address)?;
        validate_non_blank("delivery target scope", &options.scope)?;
        validate_format_policy(&options.format_policy)?;
        validate_json_value(
            "delivery target send_policy_json",
            &options.send_policy_json,
        )?;
        let store = self.store()?;
        let now = unix_timestamp()?;
        let record = DeliveryTargetRecord {
            target_id: target_id.trim().to_string(),
            kind: options.kind.trim().to_string(),
            address: options.address.trim().to_string(),
            scope: options.scope.trim().to_string(),
            owner_user_id: options
                .owner_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            allowed_agent_ids_json: serde_json::to_string(&options.allowed_agent_ids)
                .map_err(usage_json_error)?,
            allowed_session_ids_json: serde_json::to_string(&options.allowed_session_ids)
                .map_err(usage_json_error)?,
            send_policy_json: options.send_policy_json,
            format_policy: options.format_policy.trim().to_string(),
            created_at: now,
            updated_at: now,
        };
        store.put_delivery_target(&record)?;
        self.delivery_target(target_id)
    }

    pub fn delivery_target(&self, target_id: &str) -> Result<DeliveryTargetView, BootstrapError> {
        let store = self.store()?;
        let record =
            store
                .get_delivery_target(target_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "delivery target",
                    id: target_id.to_string(),
                })?;
        delivery_target_view(record)
    }

    pub fn list_delivery_targets(&self) -> Result<Vec<DeliveryTargetView>, BootstrapError> {
        let store = self.store()?;
        let mut targets = store
            .list_delivery_targets()?
            .into_iter()
            .map(delivery_target_view)
            .collect::<Result<Vec<_>, _>>()?;
        targets.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(targets)
    }

    pub fn attach_session_output_route(
        &self,
        session_id: &str,
        target_id: &str,
        options: SessionOutputRouteCreateOptions,
    ) -> Result<SessionOutputRouteView, BootstrapError> {
        validate_non_blank("session id", session_id)?;
        validate_non_blank("delivery target id", target_id)?;
        validate_format_policy(&options.format_policy)?;
        validate_json_value("session output route filter_json", &options.filter_json)?;
        let store = self.store()?;
        if !store.session_exists(session_id)? {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        if store.get_delivery_target(target_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "delivery target",
                id: target_id.to_string(),
            });
        }
        let latest_transcript = store.get_latest_transcript_for_session(session_id)?;
        let now = unix_timestamp()?;
        let route_id = options
            .route_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("route-{session_id}-{target_id}"));
        let record = SessionOutputRouteRecord {
            route_id: route_id.clone(),
            session_id: session_id.to_string(),
            target_id: target_id.to_string(),
            filter_json: options.filter_json,
            format_policy: options.format_policy.trim().to_string(),
            enabled: options.enabled,
            last_delivered_transcript_created_at: latest_transcript
                .as_ref()
                .map(|transcript| transcript.created_at)
                .or(Some(0)),
            last_delivered_transcript_id: latest_transcript
                .map(|transcript| transcript.id)
                .or_else(|| Some(String::new())),
            created_at: now,
            updated_at: now,
        };
        store.put_session_output_route(&record)?;
        self.session_output_route(&route_id)
    }

    pub fn session_output_route(
        &self,
        route_id: &str,
    ) -> Result<SessionOutputRouteView, BootstrapError> {
        let store = self.store()?;
        let record = store.get_session_output_route(route_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session output route",
                id: route_id.to_string(),
            }
        })?;
        Ok(session_output_route_view(record))
    }

    pub fn list_enabled_session_output_routes(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionOutputRouteView>, BootstrapError> {
        let store = self.store()?;
        let mut routes = store
            .list_enabled_session_output_routes(session_id)?
            .into_iter()
            .map(session_output_route_view)
            .collect::<Vec<_>>();
        routes.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(routes)
    }
}

fn delivery_target_view(
    record: DeliveryTargetRecord,
) -> Result<DeliveryTargetView, BootstrapError> {
    Ok(DeliveryTargetView {
        id: record.target_id,
        kind: record.kind,
        address: record.address,
        scope: record.scope,
        owner_user_id: record.owner_user_id,
        allowed_agent_ids: serde_json::from_str(&record.allowed_agent_ids_json)
            .map_err(usage_json_error)?,
        allowed_session_ids: serde_json::from_str(&record.allowed_session_ids_json)
            .map_err(usage_json_error)?,
        send_policy_json: record.send_policy_json,
        format_policy: record.format_policy,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn session_output_route_view(record: SessionOutputRouteRecord) -> SessionOutputRouteView {
    SessionOutputRouteView {
        id: record.route_id,
        session_id: record.session_id,
        target_id: record.target_id,
        filter_json: record.filter_json,
        format_policy: record.format_policy,
        enabled: record.enabled,
        last_delivered_transcript_created_at: record.last_delivered_transcript_created_at,
        last_delivered_transcript_id: record.last_delivered_transcript_id,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn validate_non_blank(label: &'static str, value: &str) -> Result<(), BootstrapError> {
    if value.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: format!("{label} must not be blank"),
        });
    }
    Ok(())
}

fn validate_json_value(label: &'static str, value: &str) -> Result<(), BootstrapError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|source| BootstrapError::Usage {
            reason: format!("{label} must be valid JSON: {source}"),
        })
}

fn validate_format_policy(value: &str) -> Result<(), BootstrapError> {
    match value.trim() {
        "full_text" | "summary" | "status_only" | "errors_only" => Ok(()),
        other => Err(BootstrapError::Usage {
            reason: format!(
                "delivery format_policy {other:?} is unsupported; expected full_text|summary|status_only|errors_only"
            ),
        }),
    }
}

fn usage_json_error(source: serde_json::Error) -> BootstrapError {
    BootstrapError::Usage {
        reason: format!("delivery JSON conversion failed: {source}"),
    }
}
