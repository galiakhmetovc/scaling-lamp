use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanItemStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub content: String,
    pub status: PlanItemStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlanSnapshot {
    pub session_id: String,
    pub items: Vec<PlanItem>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItemStatusParseError {
    value: String,
}

impl PlanItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

impl TryFrom<&str> for PlanItemStatus {
    type Error = PlanItemStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            _ => Err(PlanItemStatusParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl fmt::Display for PlanItemStatusParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid plan item status {}", self.value)
    }
}

impl std::error::Error for PlanItemStatusParseError {}

impl PlanSnapshot {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn system_message_text(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Plan:".to_string()];
        lines.extend(
            self.items
                .iter()
                .map(|item| format!("- [{}] {}: {}", item.status.as_str(), item.id, item.content)),
        );
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{PlanItem, PlanItemStatus, PlanSnapshot};

    #[test]
    fn empty_plan_snapshot_renders_no_system_message() {
        let snapshot = PlanSnapshot::default();

        assert!(snapshot.is_empty());
        assert!(snapshot.system_message_text().is_empty());
    }

    #[test]
    fn plan_snapshot_renders_stable_compact_system_text() {
        let snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            items: vec![
                PlanItem {
                    id: "inspect".to_string(),
                    content: "Inspect planning seams".to_string(),
                    status: PlanItemStatus::Pending,
                },
                PlanItem {
                    id: "persist".to_string(),
                    content: "Persist plan snapshot".to_string(),
                    status: PlanItemStatus::InProgress,
                },
                PlanItem {
                    id: "wire".to_string(),
                    content: "Wire prompt assembly".to_string(),
                    status: PlanItemStatus::Completed,
                },
            ],
            updated_at: 10,
        };

        let rendered = snapshot.system_message_text();

        assert!(rendered.contains("Plan:"));
        assert!(rendered.contains("- [pending] inspect: Inspect planning seams"));
        assert!(rendered.contains("- [in_progress] persist: Persist plan snapshot"));
        assert!(rendered.contains("- [completed] wire: Wire prompt assembly"));
    }
}
