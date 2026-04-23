use super::*;

pub(super) fn merge_connect_options(
    global: DaemonConnectOptions,
    host: Option<String>,
    port: Option<u16>,
) -> DaemonConnectOptions {
    DaemonConnectOptions {
        host: host.or(global.host),
        port: port.or(global.port),
    }
}

pub(super) fn daemon_supports_command(command: &Command) -> bool {
    matches!(
        command,
        Command::Status
            | Command::ChatShow { .. }
            | Command::ChatSend { .. }
            | Command::ChatRepl { .. }
            | Command::SessionCreate { .. }
            | Command::SessionShow { .. }
            | Command::SessionSkills { .. }
            | Command::SessionEnableSkill { .. }
            | Command::SessionDisableSkill { .. }
    )
}

pub(super) fn daemon_connection_for_process(
    app: &App,
    connect: &DaemonConnectOptions,
) -> Result<crate::http::client::DaemonConnection, BootstrapError> {
    crate::http::client::connect_or_autospawn_detailed(&app.config, connect, || {
        daemon::spawn_local_process().map_err(BootstrapError::Stream)
    })
}

pub(super) fn execute_command(app: &App, command: Command) -> Result<String, BootstrapError> {
    match command {
        Command::Status => render::render_status(app),
        Command::Version => app.render_version_info(),
        Command::Update { tag } => app.update_runtime_binary(tag.as_deref()),
        Command::ProviderSmoke { prompt } => render::run_provider_smoke(app, &prompt),
        Command::ChatShow { session_id } => render::show_chat(app, &session_id),
        Command::ChatSend {
            session_id,
            message,
        } => render::send_chat(app, &session_id, &message),
        Command::ChatRepl { .. } => Err(BootstrapError::Usage {
            reason: "chat repl requires interactive I/O".to_string(),
        }),
        Command::Tui { .. } => Err(BootstrapError::Usage {
            reason: "tui requires interactive terminal I/O".to_string(),
        }),
        Command::Daemon => Err(BootstrapError::Usage {
            reason: "daemon requires server mode I/O".to_string(),
        }),
        Command::DaemonStop => Err(BootstrapError::Usage {
            reason: "daemon stop requires process I/O path".to_string(),
        }),
        Command::MissionTick { now } => render::run_mission_tick(app, now),
        Command::SessionCreate { id, title } => render::create_session(&app.store()?, &id, &title),
        Command::SessionShow { id } => render::show_session(&app.store()?, &id),
        Command::SessionSkills { id } => app.render_session_skills(&id),
        Command::SessionEnableSkill { id, skill_name } => {
            app.enable_session_skill(&id, &skill_name)?;
            app.render_session_skills(&id)
        }
        Command::SessionDisableSkill { id, skill_name } => {
            app.disable_session_skill(&id, &skill_name)?;
            app.render_session_skills(&id)
        }
        Command::MissionCreate {
            id,
            session_id,
            objective,
        } => render::create_mission(&app.store()?, &id, &session_id, &objective),
        Command::MissionShow { id } => render::show_mission(&app.store()?, &id),
        Command::RunShow { id } => render::show_run(&app.store()?, &id),
        Command::JobShow { id } => render::show_job(&app.store()?, &id),
        Command::JobExecute { id, now } => render::execute_job(app, &id, now),
        Command::ApprovalList { run_id } => render::list_approvals(&app.store()?, &run_id),
        Command::ApprovalApprove {
            run_id,
            approval_id,
        } => render::approve_run(app, &run_id, &approval_id),
        Command::DelegateList { run_id } => render::list_delegates(&app.store()?, &run_id),
        Command::VerificationShow { run_id } => render::show_verification(&app.store()?, &run_id),
    }
}

pub(super) fn execute_daemon_command(
    client: &DaemonClient,
    command: Command,
) -> Result<String, BootstrapError> {
    match command {
        Command::Status => render::render_daemon_status(&client.status()?),
        Command::Version => client.about(),
        Command::Update { tag } => client.update_runtime(tag.as_deref()),
        Command::ChatShow { session_id } => render::show_chat_via_client(client, &session_id),
        Command::ChatSend {
            session_id,
            message,
        } => render::send_chat_via_client(client, &session_id, &message),
        Command::ChatRepl { .. } | Command::Tui { .. } | Command::Daemon => {
            Err(BootstrapError::Usage {
                reason: "interactive command requires process I/O path".to_string(),
            })
        }
        Command::SessionCreate { id, title } => {
            let summary = client.create_session(Some(&id), Some(&title))?;
            Ok(format!(
                "created session {} title={}",
                summary.id, summary.title
            ))
        }
        Command::SessionShow { id } => render::show_session_via_client(client, &id),
        Command::SessionSkills { id } => {
            render::render_session_skills_list(client.session_skills(&id)?)
        }
        Command::SessionEnableSkill { id, skill_name } => {
            let skills = client.enable_session_skill(&id, &skill_name)?;
            render::render_session_skills_list(skills)
        }
        Command::SessionDisableSkill { id, skill_name } => {
            let skills = client.disable_session_skill(&id, &skill_name)?;
            render::render_session_skills_list(skills)
        }
        _ => Err(BootstrapError::Usage {
            reason: "this command is not available over daemon transport yet".to_string(),
        }),
    }
}
