use agent_runtime::agent::AgentTemplateKind;
use agent_runtime::tool::ToolCatalog;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_AGENT_ID: &str = "default";
pub const JUDGE_AGENT_ID: &str = "judge";

const DEFAULT_SYSTEM_MD: &str = r#"You are the default autonomous coding agent runtime profile.

Work directly, preserve the canonical runtime path, and keep outputs concise and operational.
"#;

const DEFAULT_AGENTS_MD: &str = r#"Default agent profile.

- Primary role: general-purpose coding agent
- Prefer direct execution over unnecessary planning
- Keep tool usage explicit and minimal
"#;

const JUDGE_SYSTEM_MD: &str = r#"You are the judge agent profile.

Your role is to inspect, verify, critique, and decide whether another agent's work should proceed.
You do not execute shell commands or mutate project files.
"#;

const JUDGE_AGENTS_MD: &str = r#"Judge agent profile.

- Primary role: review and adjudication
- Read-only behavior is enforced by the allowed tool surface
- Focus on correctness, risks, and explicit verdicts
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAgentTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub template_kind: AgentTemplateKind,
    pub system_md: &'static str,
    pub agents_md: &'static str,
}

const BUILTIN_TEMPLATES: [BuiltinAgentTemplate; 2] = [
    BuiltinAgentTemplate {
        id: DEFAULT_AGENT_ID,
        name: "Default",
        template_kind: AgentTemplateKind::Default,
        system_md: DEFAULT_SYSTEM_MD,
        agents_md: DEFAULT_AGENTS_MD,
    },
    BuiltinAgentTemplate {
        id: JUDGE_AGENT_ID,
        name: "Judge",
        template_kind: AgentTemplateKind::Judge,
        system_md: JUDGE_SYSTEM_MD,
        agents_md: JUDGE_AGENTS_MD,
    },
];

pub fn builtin_templates() -> &'static [BuiltinAgentTemplate] {
    &BUILTIN_TEMPLATES
}

pub fn builtin_template(id: &str) -> Option<BuiltinAgentTemplate> {
    BUILTIN_TEMPLATES
        .iter()
        .copied()
        .find(|template| template.id == id)
}

pub fn fallback_system_md(agent_id: &str) -> &'static str {
    builtin_template(agent_id)
        .map(|template| template.system_md)
        .unwrap_or(DEFAULT_SYSTEM_MD)
}

pub fn fallback_agents_md(agent_id: &str) -> &'static str {
    builtin_template(agent_id)
        .map(|template| template.agents_md)
        .unwrap_or(DEFAULT_AGENTS_MD)
}

pub fn agents_root(data_dir: &Path) -> PathBuf {
    data_dir.join("agents")
}

pub fn agent_home(data_dir: &Path, agent_id: &str) -> PathBuf {
    agents_root(data_dir).join(agent_id)
}

pub fn builtin_allowed_tools(template_kind: AgentTemplateKind) -> Vec<String> {
    match template_kind {
        AgentTemplateKind::Default => ToolCatalog::default()
            .all_definitions()
            .iter()
            .map(|definition| definition.name.as_str().to_string())
            .collect(),
        AgentTemplateKind::Judge => vec![
            "fs_read_text",
            "fs_read_lines",
            "fs_search_text",
            "fs_find_in_files",
            "fs_list",
            "fs_glob",
            "init_plan",
            "add_task",
            "set_task_status",
            "add_task_note",
            "edit_task",
            "plan_snapshot",
            "plan_lint",
            "artifact_read",
            "artifact_search",
            "message_agent",
            "grant_agent_chain_continuation",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        AgentTemplateKind::Custom => Vec::new(),
    }
}

pub fn ensure_agent_home_layout(
    agent_home: &Path,
    system_md: &str,
    agents_md: &str,
) -> io::Result<()> {
    fs::create_dir_all(agent_home.join("skills"))?;
    write_if_missing(&agent_home.join("SYSTEM.md"), system_md)?;
    write_if_missing(&agent_home.join("AGENTS.md"), agents_md)?;
    Ok(())
}

pub fn clone_agent_home(
    source_home: &Path,
    destination_home: &Path,
    fallback_system: &str,
    fallback_agents: &str,
) -> io::Result<()> {
    if destination_home.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("agent home {} already exists", destination_home.display()),
        ));
    }

    fs::create_dir_all(destination_home.join("skills"))?;
    copy_or_write(
        &source_home.join("SYSTEM.md"),
        &destination_home.join("SYSTEM.md"),
        fallback_system,
    )?;
    copy_or_write(
        &source_home.join("AGENTS.md"),
        &destination_home.join("AGENTS.md"),
        fallback_agents,
    )?;
    clone_directory_contents(
        &source_home.join("skills"),
        &destination_home.join("skills"),
    )?;
    Ok(())
}

pub fn normalize_agent_id(name: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_dash = false;

    for ch in name.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            normalized.push(lower);
            last_was_dash = false;
        } else if !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "agent".to_string()
    } else {
        normalized
    }
}

fn write_if_missing(path: &Path, content: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, content)
}

fn copy_or_write(source: &Path, destination: &Path, fallback: &str) -> io::Result<()> {
    if source.is_file() {
        fs::copy(source, destination)?;
    } else {
        fs::write(destination, fallback)?;
    }
    Ok(())
}

fn clone_directory_contents(source: &Path, destination: &Path) -> io::Result<()> {
    if !source.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = destination.join(entry.file_name());
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            fs::create_dir_all(&target_path)?;
            clone_directory_contents(&entry_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(&entry_path, &target_path)?;
        }
    }

    Ok(())
}
