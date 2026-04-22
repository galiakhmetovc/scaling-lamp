use super::{App, BootstrapError, unix_timestamp};
use crate::agents;
use agent_persistence::{AgentProfileRecord, AgentRepository, AgentScheduleRecord};
use agent_runtime::agent::{
    AgentProfile, AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
    AgentTemplateKind,
};
use std::path::PathBuf;

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
        let profile = AgentProfile::new(
            &agent_id,
            name.trim(),
            AgentTemplateKind::Custom,
            &agent_home,
            template.allowed_tools.clone(),
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

    pub fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<AgentSchedule, BootstrapError> {
        let store = self.store()?;
        let agent = match agent_identifier
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(identifier) => load_agent_profile(&store, identifier)?.ok_or_else(|| {
                BootstrapError::MissingRecord {
                    kind: "agent",
                    id: identifier.to_string(),
                }
            })?,
            None => self.current_agent_profile()?,
        };
        let now = unix_timestamp()?;
        let schedule = AgentSchedule::new(AgentScheduleInit {
            id: id.to_string(),
            agent_profile_id: agent.id.clone(),
            workspace_root: current_workspace_root(&self.runtime.workspace.root),
            prompt: prompt.to_string(),
            mode: AgentScheduleMode::Interval,
            delivery_mode: AgentScheduleDeliveryMode::FreshSession,
            target_session_id: None,
            interval_seconds,
            next_fire_at: now,
            enabled: true,
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
