use super::{App, BootstrapError, unix_timestamp};
use crate::agents;
use agent_persistence::{
    AgentProfileRecord, AgentRepository, AgentScheduleRecord, SessionRepository,
};
use agent_runtime::agent::{
    AgentProfile, AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
    AgentTemplateKind,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleCreateOptions {
    pub agent_identifier: Option<String>,
    pub prompt: String,
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleView {
    pub id: String,
    pub agent_profile_id: String,
    pub workspace_root: PathBuf,
    pub prompt: String,
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    pub next_fire_at: i64,
    pub enabled: bool,
    pub last_triggered_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_session_id: Option<String>,
    pub last_job_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleUpdatePatch {
    pub agent_identifier: Option<String>,
    pub prompt: Option<String>,
    pub mode: Option<AgentScheduleMode>,
    pub delivery_mode: Option<AgentScheduleDeliveryMode>,
    pub target_session_id: Option<String>,
    pub interval_seconds: Option<u64>,
    pub enabled: Option<bool>,
}

impl From<AgentSchedule> for AgentScheduleView {
    fn from(value: AgentSchedule) -> Self {
        Self {
            id: value.id,
            agent_profile_id: value.agent_profile_id,
            workspace_root: value.workspace_root,
            prompt: value.prompt,
            mode: value.mode,
            delivery_mode: value.delivery_mode,
            target_session_id: value.target_session_id,
            interval_seconds: value.interval_seconds,
            next_fire_at: value.next_fire_at,
            enabled: value.enabled,
            last_triggered_at: value.last_triggered_at,
            last_finished_at: value.last_finished_at,
            last_session_id: value.last_session_id,
            last_job_id: value.last_job_id,
            last_result: value.last_result,
            last_error: value.last_error,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl App {
    pub(crate) fn ensure_builtin_agents_bootstrapped(&self) -> Result<(), BootstrapError> {
        let store = self.store()?;
        let now = unix_timestamp()?;

        for template in agents::builtin_templates() {
            let home = agents::agent_home(&self.config.data_dir, template.id);
            agents::ensure_builtin_agent_home_layout(&home, *template).map_err(|source| {
                BootstrapError::Io {
                    path: home.clone(),
                    source,
                }
            })?;

            let created_at = store
                .get_agent_profile(template.id)?
                .map(|record| record.created_at)
                .unwrap_or(now);
            let profile = AgentProfile::new(
                template.id,
                template.name,
                template.template_kind,
                &home,
                agents::builtin_allowed_tools(template.template_kind),
                self.config.workspace.default_root.clone(),
                created_at,
                now,
            )
            .map_err(|error| BootstrapError::Usage {
                reason: error.to_string(),
            })?;
            store.put_agent_profile(
                &AgentProfileRecord::try_from(&profile)
                    .map_err(BootstrapError::RecordConversion)?,
            )?;
        }

        let current = store.get_current_agent_profile_id()?;
        let current_valid = match current {
            Some(ref id) => store.get_agent_profile(id)?.is_some(),
            None => false,
        };
        if !current_valid {
            store.set_current_agent_profile_id(Some(agents::DEFAULT_AGENT_ID))?;
        }

        Ok(())
    }

    pub fn list_agents(&self) -> Result<Vec<AgentProfile>, BootstrapError> {
        let store = self.store()?;
        store
            .list_agent_profiles()?
            .into_iter()
            .map(AgentProfile::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)
    }

    pub fn current_agent_profile_id(&self) -> Result<String, BootstrapError> {
        let store = self.store()?;
        store
            .get_current_agent_profile_id()?
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "agent",
                id: agents::DEFAULT_AGENT_ID.to_string(),
            })
    }

    pub fn current_agent_profile(&self) -> Result<AgentProfile, BootstrapError> {
        let agent_id = self.current_agent_profile_id()?;
        self.agent_profile(&agent_id)
    }

    pub fn agent_profile(&self, identifier: &str) -> Result<AgentProfile, BootstrapError> {
        let store = self.store()?;
        load_agent_profile(&store, identifier)?.ok_or_else(|| BootstrapError::MissingRecord {
            kind: "agent",
            id: identifier.to_string(),
        })
    }

    pub fn select_agent_profile(&self, identifier: &str) -> Result<AgentProfile, BootstrapError> {
        let store = self.store()?;
        let profile = load_agent_profile(&store, identifier)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "agent",
                id: identifier.to_string(),
            }
        })?;
        store.set_current_agent_profile_id(Some(&profile.id))?;
        Ok(profile)
    }

    pub fn create_agent_from_template(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<AgentProfile, BootstrapError> {
        let store = self.store()?;
        let template = load_agent_profile(
            &store,
            template_identifier.unwrap_or(agents::DEFAULT_AGENT_ID),
        )?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "agent",
            id: template_identifier
                .unwrap_or(agents::DEFAULT_AGENT_ID)
                .to_string(),
        })?;
        let template_fallback = agents::builtin_template(&template.id).unwrap_or(
            agents::builtin_template(agents::DEFAULT_AGENT_ID)
                .expect("built-in default agent template must exist"),
        );

        let base_id = agents::normalize_agent_id(name);
        let agent_id = next_available_agent_id(&store, &base_id)?;
        let agent_home = agents::agent_home(&self.config.data_dir, &agent_id);
        agents::clone_agent_home(
            &template.agent_home,
            &agent_home,
            template_fallback.system_md,
            template_fallback.agents_md,
        )
        .map_err(|source| BootstrapError::Io {
            path: agent_home.clone(),
            source,
        })?;

        let now = unix_timestamp()?;
        let profile = AgentProfile::new_with_provenance(
            &agent_id,
            name.trim(),
            AgentTemplateKind::Custom,
            &agent_home,
            template.allowed_tools.clone(),
            template.default_workspace_root.clone(),
            Some(template.id.clone()),
            None,
            None,
            now,
            now,
        )
        .map_err(|error| BootstrapError::Usage {
            reason: error.to_string(),
        })?;
        store.put_agent_profile(
            &AgentProfileRecord::try_from(&profile).map_err(BootstrapError::RecordConversion)?,
        )?;
        Ok(profile)
    }

    pub fn agent_home_path(&self, identifier: &str) -> Result<PathBuf, BootstrapError> {
        Ok(self.agent_profile(identifier)?.agent_home)
    }

    pub fn list_agent_schedules(&self) -> Result<Vec<AgentSchedule>, BootstrapError> {
        let store = self.store()?;
        let current_workspace = current_workspace_root(&self.runtime.workspace.root);
        let mut schedules = store
            .list_agent_schedules()?
            .into_iter()
            .map(AgentSchedule::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        schedules.retain(|schedule| schedule.workspace_root == current_workspace);
        Ok(schedules)
    }

    pub fn agent_schedule(&self, id: &str) -> Result<AgentSchedule, BootstrapError> {
        let store = self.store()?;
        let schedule =
            store
                .get_agent_schedule(id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "agent schedule",
                    id: id.to_string(),
                })?;
        let schedule =
            AgentSchedule::try_from(schedule).map_err(BootstrapError::RecordConversion)?;
        let current_workspace = current_workspace_root(&self.runtime.workspace.root);
        if schedule.workspace_root != current_workspace {
            return Err(BootstrapError::MissingRecord {
                kind: "agent schedule",
                id: id.to_string(),
            });
        }
        Ok(schedule)
    }

    pub fn agent_schedule_view(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
        self.agent_schedule(id).map(AgentScheduleView::from)
    }

    pub fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<AgentSchedule, BootstrapError> {
        self.create_agent_schedule_with_options(
            id,
            AgentScheduleCreateOptions {
                agent_identifier: agent_identifier.map(str::to_string),
                prompt: prompt.to_string(),
                mode: AgentScheduleMode::Interval,
                delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                interval_seconds,
                enabled: true,
            },
        )
    }

    pub fn create_agent_schedule_with_options(
        &self,
        id: &str,
        options: AgentScheduleCreateOptions,
    ) -> Result<AgentSchedule, BootstrapError> {
        let store = self.store()?;
        let agent =
            resolve_schedule_agent(self, &store, options.agent_identifier.as_deref(), None)?;
        let target_session_id = validate_schedule_target_session(
            &store,
            &agent.id,
            options.delivery_mode,
            options.target_session_id,
        )?;
        let now = unix_timestamp()?;
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: id.to_string(),
            agent_profile_id: agent.id.clone(),
            workspace_root: current_workspace_root(&self.runtime.workspace.root),
            prompt: options.prompt,
            mode: options.mode,
            delivery_mode: options.delivery_mode,
            target_session_id,
            interval_seconds: options.interval_seconds,
            next_fire_at: now,
            enabled: options.enabled,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        })
        .map_err(|error| BootstrapError::Usage {
            reason: error.to_string(),
        })?;
        store.put_agent_schedule(&AgentScheduleRecord::from(&schedule))?;
        Ok(schedule)
    }

    pub fn update_agent_schedule(
        &self,
        id: &str,
        patch: AgentScheduleUpdatePatch,
    ) -> Result<AgentSchedule, BootstrapError> {
        let store = self.store()?;
        let current = self.agent_schedule(id)?;
        let agent = resolve_schedule_agent(
            self,
            &store,
            patch.agent_identifier.as_deref(),
            Some(current.agent_profile_id.as_str()),
        )?;
        let mode = patch.mode.unwrap_or(current.mode);
        let delivery_mode = patch.delivery_mode.unwrap_or(current.delivery_mode);
        let target_session_id = validate_schedule_target_session(
            &store,
            &agent.id,
            delivery_mode,
            patch
                .target_session_id
                .or(current.target_session_id.clone()),
        )?;
        let now = unix_timestamp()?;
        let updated = AgentSchedule::new(AgentScheduleInit {
            id: current.id.clone(),
            agent_profile_id: agent.id,
            workspace_root: current.workspace_root.clone(),
            prompt: patch.prompt.unwrap_or(current.prompt.clone()),
            mode,
            delivery_mode,
            target_session_id,
            interval_seconds: patch.interval_seconds.unwrap_or(current.interval_seconds),
            next_fire_at: current.next_fire_at,
            enabled: patch.enabled.unwrap_or(current.enabled),
            last_triggered_at: current.last_triggered_at,
            last_finished_at: current.last_finished_at,
            last_session_id: current.last_session_id.clone(),
            last_job_id: current.last_job_id.clone(),
            last_result: current.last_result.clone(),
            last_error: current.last_error.clone(),
            created_at: current.created_at,
            updated_at: now,
        })
        .map_err(|error| BootstrapError::Usage {
            reason: error.to_string(),
        })?;
        store.put_agent_schedule(&AgentScheduleRecord::from(&updated))?;
        Ok(updated)
    }

    pub fn set_agent_schedule_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<AgentSchedule, BootstrapError> {
        self.update_agent_schedule(
            id,
            AgentScheduleUpdatePatch {
                enabled: Some(enabled),
                ..AgentScheduleUpdatePatch::default()
            },
        )
    }

    pub fn delete_agent_schedule(&self, id: &str) -> Result<bool, BootstrapError> {
        let store = self.store()?;
        store
            .delete_agent_schedule(id)
            .map_err(BootstrapError::Store)
    }

    pub fn render_agents(&self) -> Result<String, BootstrapError> {
        let current_agent_id = self.current_agent_profile_id()?;
        let mut agents = self.list_agents()?;
        agents.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));

        let mut lines = vec![format!("Агенты: текущий={current_agent_id}")];
        for agent in agents {
            let marker = if agent.id == current_agent_id {
                "*"
            } else {
                "-"
            };
            lines.push(format!(
                "{marker} {} ({}) template={} tools={} home={}",
                agent.name,
                agent.id,
                agent.template_kind.as_str(),
                agent.allowed_tools.len(),
                agent.agent_home.display()
            ));
        }
        Ok(lines.join("\n"))
    }

    pub fn render_agent_profile(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        let profile = match identifier.map(str::trim).filter(|value| !value.is_empty()) {
            Some(identifier) => self.agent_profile(identifier)?,
            None => self.current_agent_profile()?,
        };
        Ok(render_agent_profile(&profile))
    }

    pub fn render_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        let profile = match identifier.map(str::trim).filter(|value| !value.is_empty()) {
            Some(identifier) => self.agent_profile(identifier)?,
            None => self.current_agent_profile()?,
        };
        Ok(profile.agent_home.display().to_string())
    }

    pub fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        let schedules = self.list_agent_schedules()?;
        let workspace = current_workspace_root(&self.runtime.workspace.root);
        if schedules.is_empty() {
            return Ok(format!(
                "Расписания: для workspace {} ничего не настроено",
                workspace.display()
            ));
        }

        let mut lines = vec![format!("Расписания: workspace={}", workspace.display())];
        for schedule in schedules {
            lines.push(format!(
                "- {} agent={} mode={} delivery={} enabled={} interval={} next_fire_at={}",
                schedule.id,
                schedule.agent_profile_id,
                schedule.mode.as_str(),
                schedule.delivery_mode.as_str(),
                if schedule.enabled { "yes" } else { "no" },
                schedule.interval_seconds,
                schedule.next_fire_at
            ));
            if let Some(last_triggered_at) = schedule.last_triggered_at {
                lines.push(format!("  last_triggered_at={last_triggered_at}"));
            }
            if let Some(last_finished_at) = schedule.last_finished_at {
                lines.push(format!("  last_finished_at={last_finished_at}"));
            }
            if let Some(last_session_id) = schedule.last_session_id.as_deref() {
                lines.push(format!("  last_session_id={last_session_id}"));
            }
            if let Some(last_job_id) = schedule.last_job_id.as_deref() {
                lines.push(format!("  last_job_id={last_job_id}"));
            }
            if let Some(target_session_id) = schedule.target_session_id.as_deref() {
                lines.push(format!("  target_session_id={target_session_id}"));
            }
            if let Some(last_result) = schedule.last_result.as_deref() {
                lines.push(format!("  last_result={last_result}"));
            }
            if let Some(last_error) = schedule.last_error.as_deref() {
                lines.push(format!("  last_error={last_error}"));
            }
            lines.push(format!("  prompt={}", schedule.prompt));
        }
        Ok(lines.join("\n"))
    }

    pub fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        let schedule = self.agent_schedule(id)?;
        let mut lines = vec![
            format!("id={}", schedule.id),
            format!("agent_profile_id={}", schedule.agent_profile_id),
            format!("workspace_root={}", schedule.workspace_root.display()),
            format!("mode={}", schedule.mode.as_str()),
            format!("delivery_mode={}", schedule.delivery_mode.as_str()),
            format!("enabled={}", schedule.enabled),
            format!("interval_seconds={}", schedule.interval_seconds),
            format!("next_fire_at={}", schedule.next_fire_at),
            format!("created_at={}", schedule.created_at),
            format!("updated_at={}", schedule.updated_at),
        ];
        match schedule.last_triggered_at {
            Some(value) => lines.push(format!("last_triggered_at={value}")),
            None => lines.push("last_triggered_at=<none>".to_string()),
        }
        match schedule.last_finished_at {
            Some(value) => lines.push(format!("last_finished_at={value}")),
            None => lines.push("last_finished_at=<none>".to_string()),
        }
        match schedule.target_session_id.as_deref() {
            Some(value) => lines.push(format!("target_session_id={value}")),
            None => lines.push("target_session_id=<none>".to_string()),
        }
        match schedule.last_session_id.as_deref() {
            Some(value) => lines.push(format!("last_session_id={value}")),
            None => lines.push("last_session_id=<none>".to_string()),
        }
        match schedule.last_job_id.as_deref() {
            Some(value) => lines.push(format!("last_job_id={value}")),
            None => lines.push("last_job_id=<none>".to_string()),
        }
        match schedule.last_result.as_deref() {
            Some(value) => lines.push(format!("last_result={value}")),
            None => lines.push("last_result=<none>".to_string()),
        }
        match schedule.last_error.as_deref() {
            Some(value) => lines.push(format!("last_error={value}")),
            None => lines.push("last_error=<none>".to_string()),
        }
        lines.push("prompt:".to_string());
        lines.push(schedule.prompt);
        Ok(lines.join("\n"))
    }
}

fn render_agent_profile(profile: &AgentProfile) -> String {
    let mut lines = vec![
        format!("id={}", profile.id),
        format!("name={}", profile.name),
        format!("template={}", profile.template_kind.as_str()),
        format!("home={}", profile.agent_home.display()),
        format!(
            "created_from_template={}",
            profile
                .created_from_template_id
                .as_deref()
                .unwrap_or("<none>")
        ),
        format!(
            "created_by_session={}",
            profile.created_by_session_id.as_deref().unwrap_or("<none>")
        ),
        format!(
            "created_by_agent={}",
            profile
                .created_by_agent_profile_id
                .as_deref()
                .unwrap_or("<none>")
        ),
        format!(
            "system_md={}",
            profile.agent_home.join("SYSTEM.md").display()
        ),
        format!(
            "agents_md={}",
            profile.agent_home.join("AGENTS.md").display()
        ),
        format!("skills_dir={}", profile.agent_home.join("skills").display()),
        "allowed_tools:".to_string(),
    ];
    if profile.allowed_tools.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(profile.allowed_tools.iter().map(|tool| format!("- {tool}")));
    }
    lines.join("\n")
}

fn load_agent_profile(
    store: &agent_persistence::PersistenceStore,
    identifier: &str,
) -> Result<Option<AgentProfile>, BootstrapError> {
    if let Some(record) = store.get_agent_profile(identifier)? {
        return AgentProfile::try_from(record)
            .map(Some)
            .map_err(BootstrapError::RecordConversion);
    }

    store
        .list_agent_profiles()?
        .into_iter()
        .find(|record| record.name.eq_ignore_ascii_case(identifier))
        .map(AgentProfile::try_from)
        .transpose()
        .map_err(BootstrapError::RecordConversion)
}

fn resolve_schedule_agent(
    app: &App,
    store: &agent_persistence::PersistenceStore,
    agent_identifier: Option<&str>,
    fallback_agent_id: Option<&str>,
) -> Result<AgentProfile, BootstrapError> {
    match agent_identifier
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(identifier) => {
            load_agent_profile(store, identifier)?.ok_or_else(|| BootstrapError::MissingRecord {
                kind: "agent",
                id: identifier.to_string(),
            })
        }
        None => match fallback_agent_id {
            Some(agent_id) => {
                load_agent_profile(store, agent_id)?.ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "agent",
                    id: agent_id.to_string(),
                })
            }
            None => app.current_agent_profile(),
        },
    }
}

fn validate_schedule_target_session(
    store: &agent_persistence::PersistenceStore,
    agent_profile_id: &str,
    delivery_mode: AgentScheduleDeliveryMode,
    target_session_id: Option<String>,
) -> Result<Option<String>, BootstrapError> {
    match delivery_mode {
        AgentScheduleDeliveryMode::FreshSession => Ok(None),
        AgentScheduleDeliveryMode::ExistingSession => {
            let target_session_id = target_session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| BootstrapError::Usage {
                    reason: "existing_session расписание требует target_session_id".to_string(),
                })?;
            let session = store.get_session(&target_session_id)?.ok_or_else(|| {
                BootstrapError::MissingRecord {
                    kind: "session",
                    id: target_session_id.clone(),
                }
            })?;
            if session.agent_profile_id != agent_profile_id {
                return Err(BootstrapError::Usage {
                    reason: format!(
                        "target_session_id {} привязан к агенту {}, а не {}",
                        target_session_id, session.agent_profile_id, agent_profile_id
                    ),
                });
            }
            Ok(Some(target_session_id))
        }
    }
}

fn next_available_agent_id(
    store: &agent_persistence::PersistenceStore,
    base_id: &str,
) -> Result<String, BootstrapError> {
    let mut candidate = base_id.to_string();
    let mut suffix = 2usize;

    while store.get_agent_profile(&candidate)?.is_some() {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    Ok(candidate)
}

fn current_workspace_root(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
