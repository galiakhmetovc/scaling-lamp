use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanItemStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlanItem {
    pub id: String,
    pub content: String,
    pub status: PlanItemStatus,
    pub depends_on: Vec<String>,
    pub notes: Vec<String>,
    pub blocked_reason: Option<String>,
    pub parent_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PlanSnapshot {
    pub session_id: String,
    pub goal: Option<String>,
    pub items: Vec<PlanItem>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanMutationError {
    AlreadyInitialized,
    EmptyGoal,
    EmptyDescription,
    EmptyNote,
    MissingTask { task_id: String },
    MissingDependency { task_id: String },
    MissingParentTask { task_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLintIssue {
    pub severity: String,
    pub task_id: Option<String>,
    pub message: String,
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
            Self::Blocked => "blocked",
            Self::Cancelled => "cancelled",
        }
    }
}

impl TryFrom<&str> for PlanItemStatus {
    type Error = PlanItemStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" | "todo" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" | "done" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "cancelled" => Ok(Self::Cancelled),
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
        self.goal.is_none() && self.items.is_empty()
    }

    pub fn system_message_text(&self) -> String {
        if self.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Plan:".to_string()];
        if let Some(goal) = &self.goal {
            lines.push(format!("Goal: {goal}"));
        }
        lines.extend(self.items.iter().flat_map(|item| {
            let mut item_lines = vec![format!(
                "- [{}] {}: {}",
                item.status.as_str(),
                item.id,
                item.content
            )];
            if !item.depends_on.is_empty() {
                item_lines.push(format!("  depends_on: {}", item.depends_on.join(", ")));
            }
            if let Some(blocked_reason) = &item.blocked_reason {
                item_lines.push(format!("  blocked_reason: {blocked_reason}"));
            }
            item_lines
        }));
        lines.join("\n")
    }

    pub fn plan_exists(&self) -> bool {
        self.goal.is_some() || !self.items.is_empty()
    }

    pub fn next_task_id(&self, description: &str) -> String {
        let base = description
            .to_ascii_lowercase()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join("-");
        let base = if base.is_empty() {
            "task".to_string()
        } else {
            base
        };
        let mut candidate = base.clone();
        let mut suffix = 2usize;
        while self.items.iter().any(|item| item.id == candidate) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        candidate
    }

    pub fn task(&self, task_id: &str) -> Option<&PlanItem> {
        self.items.iter().find(|item| item.id == task_id)
    }

    pub fn task_mut(&mut self, task_id: &str) -> Option<&mut PlanItem> {
        self.items.iter_mut().find(|item| item.id == task_id)
    }

    pub fn task_ready(&self, task_id: &str) -> bool {
        let Some(task) = self.task(task_id) else {
            return false;
        };
        if !matches!(task.status, PlanItemStatus::Pending) {
            return false;
        }
        task.depends_on.iter().all(|dependency| {
            self.task(dependency)
                .map(|item| item.status == PlanItemStatus::Completed)
                .unwrap_or(false)
        })
    }

    pub fn initialize(&mut self, goal: &str, now: i64) -> Result<(), PlanMutationError> {
        if self.plan_exists() {
            return Err(PlanMutationError::AlreadyInitialized);
        }
        let goal = goal.trim();
        if goal.is_empty() {
            return Err(PlanMutationError::EmptyGoal);
        }
        self.goal = Some(goal.to_string());
        self.updated_at = now;
        Ok(())
    }

    pub fn add_task(
        &mut self,
        description: &str,
        depends_on: Vec<String>,
        parent_task_id: Option<String>,
        now: i64,
    ) -> Result<PlanItem, PlanMutationError> {
        let description = description.trim();
        if description.is_empty() {
            return Err(PlanMutationError::EmptyDescription);
        }
        self.ensure_dependencies_exist(&depends_on)?;
        self.ensure_parent_exists(parent_task_id.as_deref())?;

        let task = PlanItem {
            id: self.next_task_id(description),
            content: description.to_string(),
            status: PlanItemStatus::Pending,
            depends_on,
            notes: Vec::new(),
            blocked_reason: None,
            parent_task_id,
        };
        self.items.push(task.clone());
        self.updated_at = now;
        Ok(task)
    }

    pub fn set_task_status(
        &mut self,
        task_id: &str,
        status: PlanItemStatus,
        blocked_reason: Option<String>,
        now: i64,
    ) -> Result<PlanItem, PlanMutationError> {
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| PlanMutationError::MissingTask {
                task_id: task_id.to_string(),
            })?;
        task.status = status;
        task.blocked_reason = match status {
            PlanItemStatus::Blocked => blocked_reason
                .map(|reason| reason.trim().to_string())
                .filter(|reason| !reason.is_empty()),
            _ => None,
        };
        let updated = task.clone();
        self.updated_at = now;
        Ok(updated)
    }

    pub fn add_task_note(
        &mut self,
        task_id: &str,
        note: &str,
        now: i64,
    ) -> Result<PlanItem, PlanMutationError> {
        let note = note.trim();
        if note.is_empty() {
            return Err(PlanMutationError::EmptyNote);
        }
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| PlanMutationError::MissingTask {
                task_id: task_id.to_string(),
            })?;
        task.notes.push(note.to_string());
        let updated = task.clone();
        self.updated_at = now;
        Ok(updated)
    }

    pub fn edit_task(
        &mut self,
        task_id: &str,
        description: Option<String>,
        depends_on: Option<Vec<String>>,
        parent_task_id: Option<String>,
        clear_parent_task: bool,
        now: i64,
    ) -> Result<PlanItem, PlanMutationError> {
        if let Some(depends_on) = depends_on.as_ref() {
            self.ensure_dependencies_exist(depends_on)?;
        }
        if let Some(parent_task_id) = parent_task_id.as_deref() {
            self.ensure_parent_exists(Some(parent_task_id))?;
        }
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| PlanMutationError::MissingTask {
                task_id: task_id.to_string(),
            })?;
        if let Some(description) = description {
            let description = description.trim();
            if description.is_empty() {
                return Err(PlanMutationError::EmptyDescription);
            }
            task.content = description.to_string();
        }
        if let Some(depends_on) = depends_on {
            task.depends_on = depends_on;
        }
        if clear_parent_task {
            task.parent_task_id = None;
        } else if parent_task_id.is_some() {
            task.parent_task_id = parent_task_id;
        }
        let updated = task.clone();
        self.updated_at = now;
        Ok(updated)
    }

    pub fn lint(&self) -> Vec<PlanLintIssue> {
        let mut issues = Vec::new();
        if !self.items.is_empty() && self.goal.is_none() {
            issues.push(PlanLintIssue {
                severity: "warning".to_string(),
                task_id: None,
                message: "plan has tasks but no goal".to_string(),
            });
        }

        let ids = self
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();

        for item in &self.items {
            if item
                .depends_on
                .iter()
                .any(|dependency| dependency == &item.id)
            {
                issues.push(PlanLintIssue {
                    severity: "error".to_string(),
                    task_id: Some(item.id.clone()),
                    message: "task depends on itself".to_string(),
                });
            }

            for dependency in &item.depends_on {
                if !ids.contains(dependency.as_str()) {
                    issues.push(PlanLintIssue {
                        severity: "error".to_string(),
                        task_id: Some(item.id.clone()),
                        message: format!("missing dependency {dependency}"),
                    });
                }
            }

            if matches!(item.status, PlanItemStatus::Blocked)
                && item
                    .blocked_reason
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
            {
                issues.push(PlanLintIssue {
                    severity: "warning".to_string(),
                    task_id: Some(item.id.clone()),
                    message: "blocked task is missing blocked_reason".to_string(),
                });
            }
        }

        issues.extend(self.detect_dependency_cycles());
        issues
    }

    fn ensure_dependencies_exist(&self, depends_on: &[String]) -> Result<(), PlanMutationError> {
        for dependency in depends_on {
            if self.task(dependency).is_none() {
                return Err(PlanMutationError::MissingDependency {
                    task_id: dependency.clone(),
                });
            }
        }
        Ok(())
    }

    fn ensure_parent_exists(&self, parent_task_id: Option<&str>) -> Result<(), PlanMutationError> {
        if let Some(parent_task_id) = parent_task_id
            && self.task(parent_task_id).is_none()
        {
            return Err(PlanMutationError::MissingParentTask {
                task_id: parent_task_id.to_string(),
            });
        }
        Ok(())
    }

    fn detect_dependency_cycles(&self) -> Vec<PlanLintIssue> {
        fn visit<'a>(
            task_id: &'a str,
            graph: &BTreeMap<&'a str, Vec<&'a str>>,
            visiting: &mut BTreeSet<&'a str>,
            visited: &mut BTreeSet<&'a str>,
            issues: &mut Vec<PlanLintIssue>,
        ) {
            if visited.contains(task_id) {
                return;
            }
            if !visiting.insert(task_id) {
                issues.push(PlanLintIssue {
                    severity: "error".to_string(),
                    task_id: Some(task_id.to_string()),
                    message: "dependency cycle detected".to_string(),
                });
                return;
            }
            if let Some(edges) = graph.get(task_id) {
                for dependency in edges {
                    visit(dependency, graph, visiting, visited, issues);
                }
            }
            visiting.remove(task_id);
            visited.insert(task_id);
        }

        let graph = self
            .items
            .iter()
            .map(|item| {
                (
                    item.id.as_str(),
                    item.depends_on
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut issues = Vec::new();
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();

        for task_id in graph.keys().copied().collect::<Vec<_>>() {
            visit(task_id, &graph, &mut visiting, &mut visited, &mut issues);
        }

        issues
    }
}

impl fmt::Display for PlanMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(formatter, "plan already exists"),
            Self::EmptyGoal => write!(formatter, "plan goal must not be empty"),
            Self::EmptyDescription => write!(formatter, "task description must not be empty"),
            Self::EmptyNote => write!(formatter, "task note must not be empty"),
            Self::MissingTask { task_id } => write!(formatter, "unknown task {task_id}"),
            Self::MissingDependency { task_id } => {
                write!(formatter, "unknown dependency {task_id}")
            }
            Self::MissingParentTask { task_id } => {
                write!(formatter, "unknown parent task {task_id}")
            }
        }
    }
}

impl std::error::Error for PlanMutationError {}

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
            goal: Some("Ship planning tools".to_string()),
            items: vec![
                PlanItem {
                    id: "inspect".to_string(),
                    content: "Inspect planning seams".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
                PlanItem {
                    id: "persist".to_string(),
                    content: "Persist plan snapshot".to_string(),
                    status: PlanItemStatus::InProgress,
                    depends_on: vec!["inspect".to_string()],
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
                PlanItem {
                    id: "wire".to_string(),
                    content: "Wire prompt assembly".to_string(),
                    status: PlanItemStatus::Completed,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
            ],
            updated_at: 10,
        };

        let rendered = snapshot.system_message_text();

        assert!(rendered.contains("Plan:"));
        assert!(rendered.contains("Goal: Ship planning tools"));
        assert!(rendered.contains("- [pending] inspect: Inspect planning seams"));
        assert!(rendered.contains("- [in_progress] persist: Persist plan snapshot"));
        assert!(rendered.contains("depends_on: inspect"));
        assert!(rendered.contains("- [completed] wire: Wire prompt assembly"));
    }

    #[test]
    fn next_task_id_slugs_description_and_avoids_collisions() {
        let snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            goal: None,
            items: vec![PlanItem {
                id: "inspect-auth-module".to_string(),
                content: "Inspect auth module".to_string(),
                status: PlanItemStatus::Pending,
                depends_on: Vec::new(),
                notes: Vec::new(),
                blocked_reason: None,
                parent_task_id: None,
            }],
            updated_at: 0,
        };

        assert_eq!(
            snapshot.next_task_id("Inspect auth module"),
            "inspect-auth-module-2"
        );
    }

    #[test]
    fn task_ready_requires_all_dependencies_to_be_completed() {
        let snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            goal: None,
            items: vec![
                PlanItem {
                    id: "inspect".to_string(),
                    content: "Inspect auth module".to_string(),
                    status: PlanItemStatus::Completed,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
                PlanItem {
                    id: "wire".to_string(),
                    content: "Wire prompt assembly".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: vec!["inspect".to_string()],
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
            ],
            updated_at: 0,
        };

        assert!(snapshot.task_ready("wire"));
    }

    #[test]
    fn granular_plan_mutations_update_goal_notes_and_status() {
        let mut snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            goal: None,
            items: Vec::new(),
            updated_at: 0,
        };

        snapshot.initialize("Refactor auth", 1).expect("init plan");
        let task = snapshot
            .add_task("Inspect auth module", Vec::new(), None, 2)
            .expect("add task");
        let noted = snapshot
            .add_task_note(task.id.as_str(), "Look at login and session paths", 3)
            .expect("add task note");
        let edited = snapshot
            .edit_task(
                task.id.as_str(),
                Some("Inspect auth and session modules".to_string()),
                Some(Vec::new()),
                None,
                false,
                4,
            )
            .expect("edit task");
        let status = snapshot
            .set_task_status(
                task.id.as_str(),
                PlanItemStatus::Blocked,
                Some("Need production logs".to_string()),
                5,
            )
            .expect("set status");

        assert_eq!(snapshot.goal.as_deref(), Some("Refactor auth"));
        assert_eq!(noted.notes.len(), 1);
        assert_eq!(edited.content, "Inspect auth and session modules");
        assert_eq!(status.status, PlanItemStatus::Blocked);
        assert_eq!(
            status.blocked_reason.as_deref(),
            Some("Need production logs")
        );
    }

    #[test]
    fn plan_lint_reports_missing_goal_cycles_and_blocked_reason_gaps() {
        let snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            goal: None,
            items: vec![
                PlanItem {
                    id: "inspect".to_string(),
                    content: "Inspect auth module".to_string(),
                    status: PlanItemStatus::Blocked,
                    depends_on: vec!["wire".to_string()],
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
                PlanItem {
                    id: "wire".to_string(),
                    content: "Wire prompt assembly".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: vec!["inspect".to_string()],
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
            ],
            updated_at: 0,
        };

        let issues = snapshot.lint();

        assert!(
            issues
                .iter()
                .any(|issue| issue.message == "plan has tasks but no goal")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message == "blocked task is missing blocked_reason")
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message == "dependency cycle detected")
        );
    }
}
