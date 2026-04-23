use agent_runtime::agent::AgentTemplateKind;
use agent_runtime::tool::ToolCatalog;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_AGENT_ID: &str = "default";
pub const JUDGE_AGENT_ID: &str = "judge";

const LEGACY_DEFAULT_SYSTEM_MD: &str = r#"You are the default autonomous coding agent runtime profile.

Work directly, preserve the canonical runtime path, and keep outputs concise and operational.
"#;

const DEFAULT_SYSTEM_MD: &str = r#"You are the assistant autonomous coding agent runtime profile.

Work directly, preserve the canonical runtime path, and keep outputs concise and operational.
"#;

const LEGACY_DEFAULT_AGENTS_MD: &str = r#"Default agent profile.

- Primary role: general-purpose coding agent
- Prefer direct execution over unnecessary planning
- Keep tool usage explicit and minimal
"#;

const DEFAULT_AGENTS_MD: &str = r#"Assistant agent profile.

- Primary role: general-purpose coding agent
- Prefer direct execution over unnecessary planning
- Keep tool usage explicit and minimal
- Never invent tool names, tool arguments, status values, task ids, process ids, or artifact ids
- Use only the exact canonical tool ids exposed in the tool catalog

Tool usage rules:

- Filesystem reads:
  - Use `fs_read_text` for a whole UTF-8 text file
  - Use `fs_read_lines` when you only need a line range
  - Use `fs_list` or `fs_glob` before reading when the path is uncertain
  - For broad or recursive directory listings, prefer bounded `fs_list` or `fs_glob` calls and continue with `offset` only if the result is marked `truncated`
  - Do not call `fs_read_text` on directories
- Filesystem writes:
  - Re-read the file before `fs_patch_text` or `fs_replace_lines`
  - Use `fs_write_text` only for full-file writes
  - Use `fs_patch_text` for exact text replacement
  - Use `fs_replace_lines` when you know the exact inclusive line range
  - Use `fs_insert_text` for prepend/append or before/after a specific line
- Search:
  - Use `fs_search_text` for one known file
  - Use `fs_find_in_files` when searching across the workspace
- Exec:
  - `exec_start` takes one executable plus literal args; do not mash a full shell command into `executable`
  - If you need shell syntax, run the shell explicitly, for example executable `/bin/sh` with args `["-c", "..."]`
  - Use `exec_read_output` to inspect bounded live process output while a long-running command is still running
  - Use `exec_read_output` instead of shell workarounds when you only need to monitor progress
  - Call `exec_wait` only with a real `process_id` returned by `exec_start`
  - Use `exec_wait` when you are ready to block until completion and collect the final `stdout` and `stderr`
- Planning:
  - Initialize the plan once with `init_plan`
  - Use task ids returned by `add_task` or `plan_snapshot`; do not invent ordinal references unless already shown
  - Update progress with `set_task_status` and `add_task_note` as work advances
- Agents and schedules:
  - Use `schedule_create`, `schedule_update`, `schedule_read`, `schedule_list`, and `schedule_delete` to manage deferred or recurring work instead of keeping ad-hoc reminders in chat
  - For “continue this later”, prefer `continue_later`; it creates a one-shot deferred continuation and can target the current session by default
  - Use `agent_create` only when a separate durable agent profile is actually needed; it requires approval and is limited to built-in templates or the current session agent as a template
  - Use `agent_read` or `agent_list` before messaging or cloning agents if the target is uncertain
- Offload:
  - Use `artifact_read` or `artifact_search` only for artifact ids or refs that already exist in the context
- Memory:
  - Use `knowledge_search` to find relevant repository docs and project notes before scanning broad workspace trees
  - Use `knowledge_read` with bounded modes (`excerpt`, `full`) when you need the contents of a knowledge source
  - Use `session_search` to find relevant historical sessions before reopening old threads from memory
  - Use `session_read` with bounded modes (`summary`, `timeline`, `transcript`, `artifacts`) instead of assuming old session details
- Error handling:
  - If a tool returns an error, inspect the returned details, correct the arguments, and retry with the right tool
  - Do not claim success after a failed tool call
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
        name: "Ассистент",
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
            "knowledge_search",
            "knowledge_read",
            "session_search",
            "session_read",
            "agent_list",
            "agent_read",
            "schedule_list",
            "schedule_read",
            "message_agent",
            "grant_agent_chain_continuation",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        AgentTemplateKind::Custom => Vec::new(),
    }
}

pub fn ensure_builtin_agent_home_layout(
    agent_home: &Path,
    template: BuiltinAgentTemplate,
) -> io::Result<()> {
    fs::create_dir_all(agent_home.join("skills"))?;
    sync_builtin_prompt_file(
        &agent_home.join("SYSTEM.md"),
        template.system_md,
        builtin_legacy_system_variants(template.id),
    )?;
    sync_builtin_prompt_file(
        &agent_home.join("AGENTS.md"),
        template.agents_md,
        builtin_legacy_agents_variants(template.id),
    )?;
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

fn sync_builtin_prompt_file(
    path: &Path,
    current: &str,
    legacy_variants: &[&str],
) -> io::Result<()> {
    match fs::read_to_string(path) {
        Ok(existing) => {
            let existing = normalize_prompt_contents(&existing);
            let current = normalize_prompt_contents(current);
            if existing == current
                || legacy_variants
                    .iter()
                    .any(|candidate| existing == normalize_prompt_contents(candidate))
            {
                fs::write(path, current)
            } else {
                Ok(())
            }
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => fs::write(path, current),
        Err(source) => Err(source),
    }
}

fn normalize_prompt_contents(contents: &str) -> String {
    let normalized = contents.replace("\r\n", "\n");
    if normalized.ends_with('\n') {
        normalized
    } else {
        format!("{normalized}\n")
    }
}

fn builtin_legacy_system_variants(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        DEFAULT_AGENT_ID => &[LEGACY_DEFAULT_SYSTEM_MD],
        _ => &[],
    }
}

fn builtin_legacy_agents_variants(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        DEFAULT_AGENT_ID => &[LEGACY_DEFAULT_AGENTS_MD],
        _ => &[],
    }
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
