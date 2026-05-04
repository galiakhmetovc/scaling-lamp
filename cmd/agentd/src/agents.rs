use agent_runtime::agent::AgentTemplateKind;
use agent_runtime::tool::ToolCatalog;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_AGENT_ID: &str = "default";
pub const JUDGE_AGENT_ID: &str = "judge";

const DEFAULT_SYSTEM_MD: &str = include_str!("../../../agent-templates/default/SYSTEM.md");
const DEFAULT_AGENTS_MD: &str = include_str!("../../../agent-templates/default/AGENTS.md");
const JUDGE_SYSTEM_MD: &str = include_str!("../../../agent-templates/judge/SYSTEM.md");
const JUDGE_AGENTS_MD: &str = include_str!("../../../agent-templates/judge/AGENTS.md");
const MEMORY_CURATOR_SYSTEM_MD: &str =
    include_str!("../../../agent-templates/system/memory-curator/SYSTEM.md");

const LEGACY_DEFAULT_SYSTEM_MD: &str =
    include_str!("../../../agent-templates/default/legacy/default-legacy.SYSTEM.md");
const PRE_SELF_LEARNING_DEFAULT_SYSTEM_MD: &str =
    include_str!("../../../agent-templates/default/legacy/default-pre-self-learning.SYSTEM.md");
const LEGACY_DEFAULT_AGENTS_MD: &str =
    include_str!("../../../agent-templates/default/legacy/default-legacy.AGENTS.md");
const PRE_INTERAGENT_GUIDANCE_DEFAULT_AGENTS_MD: &str =
    include_str!("../../../agent-templates/default/legacy/default-pre-interagent.AGENTS.md");
const PRE_REMINDER_GUIDANCE_DEFAULT_AGENTS_MD: &str =
    include_str!("../../../agent-templates/default/legacy/default-pre-reminder.AGENTS.md");
const PRE_SELF_LEARNING_JUDGE_SYSTEM_MD: &str =
    include_str!("../../../agent-templates/judge/legacy/judge-pre-self-learning.SYSTEM.md");

const DEFAULT_SILVERBULLET_SPACE_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/silverbullet-space/SKILL.md");
const DEFAULT_MEM0_MEMORY_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/mem0-memory/SKILL.md");
const DEFAULT_SCOPED_KV_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/scoped-kv/SKILL.md");
const DEFAULT_TELEGRAM_OPERATOR_WORKFLOW_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/telegram-operator-workflow/SKILL.md");
const DEFAULT_BROWSER_SEARCH_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/browser-search/SKILL.md");
const DEFAULT_FILE_ARTIFACT_WORKFLOW_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/file-artifact-workflow/SKILL.md");
const DEFAULT_PLANNING_SESSION_LIFECYCLE_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/planning-session-lifecycle/SKILL.md");
const DEFAULT_AGENT_BROWSER_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/skills/agent-browser/SKILL.md");

const DEFAULT_LIGHTPANDA_BROWSER_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/deprecated-skills/lightpanda-browser/SKILL.md");
const DEPRECATED_LOGSEQ_GRAPH_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/deprecated-skills/logseq-graph/SKILL.md");
const DEPRECATED_OBSIDIAN_VAULT_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/deprecated-skills/obsidian-vault/SKILL.md");
const DEFAULT_OBSIDIAN_VAULT_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/legacy/obsidian-vault-default.SKILL.md");
const PRE_WORKING_KNOWLEDGE_OBSIDIAN_VAULT_SKILL_MD: &str = include_str!(
    "../../../agent-templates/default/legacy/obsidian-vault-pre-working-knowledge.SKILL.md"
);
const PRE_PARA_OBSIDIAN_VAULT_SKILL_MD: &str =
    include_str!("../../../agent-templates/default/legacy/obsidian-vault-pre-para.SKILL.md");

#[derive(Debug, Clone, Copy)]
struct BundledTemplateFile {
    relative_path: &'static str,
    content: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinSkillTemplate {
    name: &'static str,
    relative_path: &'static str,
    bundled_content: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct DeprecatedBuiltinSkillTemplate {
    name: &'static str,
    relative_path: &'static str,
    bundled_content: &'static str,
    legacy_variants: &'static [&'static str],
    legacy_markers: &'static [&'static str],
}

const BUNDLED_TEMPLATE_FILES: &[BundledTemplateFile] = &[
    BundledTemplateFile {
        relative_path: "default/SYSTEM.md",
        content: DEFAULT_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "default/AGENTS.md",
        content: DEFAULT_AGENTS_MD,
    },
    BundledTemplateFile {
        relative_path: "judge/SYSTEM.md",
        content: JUDGE_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "judge/AGENTS.md",
        content: JUDGE_AGENTS_MD,
    },
    BundledTemplateFile {
        relative_path: "system/memory-curator/SYSTEM.md",
        content: MEMORY_CURATOR_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/default-legacy.SYSTEM.md",
        content: LEGACY_DEFAULT_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/default-pre-self-learning.SYSTEM.md",
        content: PRE_SELF_LEARNING_DEFAULT_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/default-legacy.AGENTS.md",
        content: LEGACY_DEFAULT_AGENTS_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/default-pre-interagent.AGENTS.md",
        content: PRE_INTERAGENT_GUIDANCE_DEFAULT_AGENTS_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/default-pre-reminder.AGENTS.md",
        content: PRE_REMINDER_GUIDANCE_DEFAULT_AGENTS_MD,
    },
    BundledTemplateFile {
        relative_path: "judge/legacy/judge-pre-self-learning.SYSTEM.md",
        content: PRE_SELF_LEARNING_JUDGE_SYSTEM_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/silverbullet-space/SKILL.md",
        content: DEFAULT_SILVERBULLET_SPACE_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/mem0-memory/SKILL.md",
        content: DEFAULT_MEM0_MEMORY_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/scoped-kv/SKILL.md",
        content: DEFAULT_SCOPED_KV_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/telegram-operator-workflow/SKILL.md",
        content: DEFAULT_TELEGRAM_OPERATOR_WORKFLOW_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/browser-search/SKILL.md",
        content: DEFAULT_BROWSER_SEARCH_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/file-artifact-workflow/SKILL.md",
        content: DEFAULT_FILE_ARTIFACT_WORKFLOW_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/planning-session-lifecycle/SKILL.md",
        content: DEFAULT_PLANNING_SESSION_LIFECYCLE_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/skills/agent-browser/SKILL.md",
        content: DEFAULT_AGENT_BROWSER_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/deprecated-skills/lightpanda-browser/SKILL.md",
        content: DEFAULT_LIGHTPANDA_BROWSER_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/deprecated-skills/logseq-graph/SKILL.md",
        content: DEPRECATED_LOGSEQ_GRAPH_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/deprecated-skills/obsidian-vault/SKILL.md",
        content: DEPRECATED_OBSIDIAN_VAULT_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/obsidian-vault-default.SKILL.md",
        content: DEFAULT_OBSIDIAN_VAULT_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/obsidian-vault-pre-working-knowledge.SKILL.md",
        content: PRE_WORKING_KNOWLEDGE_OBSIDIAN_VAULT_SKILL_MD,
    },
    BundledTemplateFile {
        relative_path: "default/legacy/obsidian-vault-pre-para.SKILL.md",
        content: PRE_PARA_OBSIDIAN_VAULT_SKILL_MD,
    },
];

const DEFAULT_ACTIVE_SKILL_TEMPLATES: &[BuiltinSkillTemplate] = &[
    BuiltinSkillTemplate {
        name: "silverbullet-space",
        relative_path: "default/skills/silverbullet-space/SKILL.md",
        bundled_content: DEFAULT_SILVERBULLET_SPACE_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "mem0-memory",
        relative_path: "default/skills/mem0-memory/SKILL.md",
        bundled_content: DEFAULT_MEM0_MEMORY_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "scoped-kv",
        relative_path: "default/skills/scoped-kv/SKILL.md",
        bundled_content: DEFAULT_SCOPED_KV_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "telegram-operator-workflow",
        relative_path: "default/skills/telegram-operator-workflow/SKILL.md",
        bundled_content: DEFAULT_TELEGRAM_OPERATOR_WORKFLOW_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "browser-search",
        relative_path: "default/skills/browser-search/SKILL.md",
        bundled_content: DEFAULT_BROWSER_SEARCH_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "file-artifact-workflow",
        relative_path: "default/skills/file-artifact-workflow/SKILL.md",
        bundled_content: DEFAULT_FILE_ARTIFACT_WORKFLOW_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "planning-session-lifecycle",
        relative_path: "default/skills/planning-session-lifecycle/SKILL.md",
        bundled_content: DEFAULT_PLANNING_SESSION_LIFECYCLE_SKILL_MD,
    },
    BuiltinSkillTemplate {
        name: "agent-browser",
        relative_path: "default/skills/agent-browser/SKILL.md",
        bundled_content: DEFAULT_AGENT_BROWSER_SKILL_MD,
    },
];

const DEFAULT_DEPRECATED_SKILL_TEMPLATES: &[DeprecatedBuiltinSkillTemplate] = &[
    DeprecatedBuiltinSkillTemplate {
        name: "lightpanda-browser",
        relative_path: "default/deprecated-skills/lightpanda-browser/SKILL.md",
        bundled_content: DEFAULT_LIGHTPANDA_BROWSER_SKILL_MD,
        legacy_variants: &[],
        legacy_markers: &[
            "# Lightpanda Browser",
            "mcp__lightpanda__",
            "Lightpanda is exposed",
        ],
    },
    DeprecatedBuiltinSkillTemplate {
        name: "logseq-graph",
        relative_path: "default/deprecated-skills/logseq-graph/SKILL.md",
        bundled_content: DEPRECATED_LOGSEQ_GRAPH_SKILL_MD,
        legacy_variants: &[],
        legacy_markers: &[
            "# Logseq Graph",
            "/var/lib/teamd/knowledge/logseq/teamd",
            "Logseq Publish",
        ],
    },
    DeprecatedBuiltinSkillTemplate {
        name: "obsidian-vault",
        relative_path: "default/deprecated-skills/obsidian-vault/SKILL.md",
        bundled_content: DEPRECATED_OBSIDIAN_VAULT_SKILL_MD,
        legacy_variants: &[
            DEFAULT_OBSIDIAN_VAULT_SKILL_MD,
            PRE_WORKING_KNOWLEDGE_OBSIDIAN_VAULT_SKILL_MD,
            PRE_PARA_OBSIDIAN_VAULT_SKILL_MD,
        ],
        legacy_markers: &[
            "# Obsidian Vault",
            "/var/lib/teamd/vaults/teamd",
            "mcp__obsidian__",
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAgentTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub template_kind: AgentTemplateKind,
    pub system_md: &'static str,
    pub agents_md: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinAgentTemplateContent {
    pub system_md: String,
    pub agents_md: String,
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

pub fn agent_templates_root(data_dir: &Path) -> PathBuf {
    data_dir.join("agent-templates")
}

pub fn ensure_runtime_agent_templates_layout(data_dir: &Path) -> io::Result<()> {
    let root = agent_templates_root(data_dir);
    for template_file in BUNDLED_TEMPLATE_FILES {
        let path = root.join(template_file.relative_path);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, normalize_prompt_contents(template_file.content))?;
    }
    Ok(())
}

pub fn load_builtin_template_content(
    data_dir: &Path,
    template: BuiltinAgentTemplate,
) -> io::Result<BuiltinAgentTemplateContent> {
    Ok(BuiltinAgentTemplateContent {
        system_md: read_runtime_template_or_bundled(
            data_dir,
            &format!("{}/SYSTEM.md", template.id),
            template.system_md,
        )?,
        agents_md: read_runtime_template_or_bundled(
            data_dir,
            &format!("{}/AGENTS.md", template.id),
            template.agents_md,
        )?,
    })
}

pub fn fallback_system_md(data_dir: &Path, agent_id: &str) -> String {
    let template = builtin_template(agent_id).unwrap_or(
        builtin_template(DEFAULT_AGENT_ID).expect("built-in default agent template must exist"),
    );
    load_builtin_template_content(data_dir, template)
        .map(|content| content.system_md)
        .unwrap_or_else(|_| normalize_prompt_contents(template.system_md))
}

pub fn fallback_agents_md(data_dir: &Path, agent_id: &str) -> String {
    let template = builtin_template(agent_id).unwrap_or(
        builtin_template(DEFAULT_AGENT_ID).expect("built-in default agent template must exist"),
    );
    load_builtin_template_content(data_dir, template)
        .map(|content| content.agents_md)
        .unwrap_or_else(|_| normalize_prompt_contents(template.agents_md))
}

pub fn agents_root(data_dir: &Path) -> PathBuf {
    data_dir.join("agents")
}

pub fn agent_home(data_dir: &Path, agent_id: &str) -> PathBuf {
    agents_root(data_dir).join(agent_id)
}

pub fn agent_workspace(data_dir: &Path, agent_id: &str) -> PathBuf {
    data_dir
        .parent()
        .unwrap_or(data_dir)
        .join("workspaces")
        .join("agents")
        .join(agent_id)
}

pub fn ensure_agent_workspace_layout(agent_workspace: &Path) -> io::Result<()> {
    fs::create_dir_all(agent_workspace)
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
            "prompt_budget_read",
            "autonomy_state_read",
            "skill_list",
            "skill_read",
            "artifact_read",
            "artifact_search",
            "knowledge_search",
            "knowledge_read",
            "session_search",
            "session_read",
            "session_wait",
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
    data_dir: &Path,
    agent_home: &Path,
    template: BuiltinAgentTemplate,
) -> io::Result<()> {
    ensure_runtime_agent_templates_layout(data_dir)?;
    let content = load_builtin_template_content(data_dir, template)?;
    fs::create_dir_all(agent_home.join("skills"))?;
    sync_builtin_prompt_file(
        &agent_home.join("SYSTEM.md"),
        &content.system_md,
        builtin_legacy_system_variants(template.id),
    )?;
    if template.id == DEFAULT_AGENT_ID {
        sync_builtin_default_agents_prompt_file(
            &agent_home.join("AGENTS.md"),
            &content.agents_md,
            builtin_legacy_agents_variants(template.id),
        )?;
    } else {
        sync_builtin_prompt_file(
            &agent_home.join("AGENTS.md"),
            &content.agents_md,
            builtin_legacy_agents_variants(template.id),
        )?;
    }
    if template.id == DEFAULT_AGENT_ID {
        for skill in DEFAULT_ACTIVE_SKILL_TEMPLATES {
            let skill_content = read_runtime_template_or_bundled(
                data_dir,
                skill.relative_path,
                skill.bundled_content,
            )?;
            sync_builtin_default_skill(agent_home, skill.name, &skill_content, &[])?;
        }
        for skill in DEFAULT_DEPRECATED_SKILL_TEMPLATES {
            let skill_content = read_runtime_template_or_bundled(
                data_dir,
                skill.relative_path,
                skill.bundled_content,
            )?;
            remove_builtin_default_skill_with_legacy_markers(
                agent_home,
                skill.name,
                &skill_content,
                skill.legacy_variants,
                skill.legacy_markers,
            )?;
        }
    }
    Ok(())
}

fn sync_builtin_default_skill(
    agent_home: &Path,
    skill_name: &str,
    content: &str,
    legacy_variants: &[&str],
) -> io::Result<()> {
    let skill_dir = agent_home.join("skills").join(skill_name);
    fs::create_dir_all(&skill_dir)?;
    sync_builtin_prompt_file(&skill_dir.join("SKILL.md"), content, legacy_variants)
}

fn remove_builtin_default_skill_with_legacy_markers(
    agent_home: &Path,
    skill_name: &str,
    current_generated_content: &str,
    legacy_variants: &[&str],
    legacy_markers: &[&str],
) -> io::Result<()> {
    let skill_dir = agent_home.join("skills").join(skill_name);
    let path = skill_dir.join("SKILL.md");
    let existing = match fs::read_to_string(&path) {
        Ok(existing) => existing,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(source),
    };

    let existing_normalized = normalize_prompt_contents(&existing);
    let current = normalize_prompt_contents(current_generated_content);
    let is_generated = existing_normalized == current
        || legacy_variants
            .iter()
            .any(|candidate| existing_normalized == normalize_prompt_contents(candidate))
        || legacy_markers
            .iter()
            .all(|marker| existing.contains(marker));

    if is_generated {
        fs::remove_dir_all(skill_dir)?;
    }
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

fn sync_builtin_default_agents_prompt_file(
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

fn read_runtime_template_or_bundled(
    data_dir: &Path,
    relative_path: &str,
    bundled: &str,
) -> io::Result<String> {
    let path = agent_templates_root(data_dir).join(relative_path);
    match fs::read_to_string(&path) {
        Ok(content) => Ok(normalize_prompt_contents(&content)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            Ok(normalize_prompt_contents(bundled))
        }
        Err(source) => Err(source),
    }
}

fn builtin_legacy_system_variants(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        DEFAULT_AGENT_ID => &[
            LEGACY_DEFAULT_SYSTEM_MD,
            PRE_SELF_LEARNING_DEFAULT_SYSTEM_MD,
        ],
        JUDGE_AGENT_ID => &[PRE_SELF_LEARNING_JUDGE_SYSTEM_MD],
        _ => &[],
    }
}

fn builtin_legacy_agents_variants(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        DEFAULT_AGENT_ID => &[
            LEGACY_DEFAULT_AGENTS_MD,
            PRE_INTERAGENT_GUIDANCE_DEFAULT_AGENTS_MD,
            PRE_REMINDER_GUIDANCE_DEFAULT_AGENTS_MD,
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agent_home_uses_runtime_agent_template_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state");
        let default_home = agent_home(&data_dir, DEFAULT_AGENT_ID);
        let templates_root = agent_templates_root(&data_dir);

        fs::create_dir_all(templates_root.join("default/skills/mem0-memory"))
            .expect("create runtime template override dirs");
        fs::write(
            templates_root.join("default/SYSTEM.md"),
            "runtime default system\n",
        )
        .expect("write system override");
        fs::write(
            templates_root.join("default/AGENTS.md"),
            "runtime default agents\n",
        )
        .expect("write agents override");
        fs::write(
            templates_root.join("default/skills/mem0-memory/SKILL.md"),
            "---\nname: mem0-memory\n---\nruntime memory skill\n",
        )
        .expect("write skill override");

        ensure_builtin_agent_home_layout(
            &data_dir,
            &default_home,
            builtin_template(DEFAULT_AGENT_ID).expect("default template"),
        )
        .expect("refresh builtin prompt");

        assert_eq!(
            fs::read_to_string(default_home.join("SYSTEM.md")).expect("read system"),
            "runtime default system\n"
        );
        assert_eq!(
            fs::read_to_string(default_home.join("AGENTS.md")).expect("read agents"),
            "runtime default agents\n"
        );
        assert_eq!(
            fs::read_to_string(default_home.join("skills/mem0-memory/SKILL.md"))
                .expect("read skill"),
            "---\nname: mem0-memory\n---\nruntime memory skill\n"
        );

        assert!(
            templates_root
                .join("default/skills/silverbullet-space/SKILL.md")
                .exists(),
            "bootstrap must seed missing runtime-editable template files"
        );
    }

    #[test]
    fn builtin_agent_home_preserves_operator_modified_agent_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state");
        let default_home = agent_home(&data_dir, DEFAULT_AGENT_ID);
        fs::create_dir_all(&default_home).expect("create default home");
        fs::write(default_home.join("AGENTS.md"), "operator custom agents\n")
            .expect("write custom agents");

        ensure_builtin_agent_home_layout(
            &data_dir,
            &default_home,
            builtin_template(DEFAULT_AGENT_ID).expect("default template"),
        )
        .expect("refresh builtin prompt");

        assert_eq!(
            fs::read_to_string(default_home.join("AGENTS.md")).expect("read agents"),
            "operator custom agents\n"
        );
    }

    #[test]
    fn builtin_agent_home_seeds_current_stack_skills_and_removes_legacy_skills() {
        let temp = tempfile::tempdir().expect("tempdir");
        let data_dir = temp.path().join("state");
        let default_home = agent_home(&data_dir, DEFAULT_AGENT_ID);
        fs::create_dir_all(default_home.join("skills/lightpanda-browser"))
            .expect("create default home");
        fs::write(
            default_home.join("skills/lightpanda-browser/SKILL.md"),
            DEFAULT_LIGHTPANDA_BROWSER_SKILL_MD,
        )
        .expect("write generated lightpanda skill");
        fs::create_dir_all(default_home.join("skills/logseq-graph")).expect("create logseq skill");
        fs::write(
            default_home.join("skills/logseq-graph/SKILL.md"),
            DEPRECATED_LOGSEQ_GRAPH_SKILL_MD,
        )
        .expect("write generated logseq skill");
        fs::create_dir_all(default_home.join("skills/obsidian-vault"))
            .expect("create obsidian skill");
        fs::write(
            default_home.join("skills/obsidian-vault/SKILL.md"),
            DEFAULT_OBSIDIAN_VAULT_SKILL_MD,
        )
        .expect("write generated obsidian skill");

        ensure_builtin_agent_home_layout(
            &data_dir,
            &default_home,
            builtin_template(DEFAULT_AGENT_ID).expect("default template"),
        )
        .expect("refresh builtin prompt");

        let silverbullet_skill =
            fs::read_to_string(default_home.join("skills/silverbullet-space/SKILL.md"))
                .expect("read silverbullet skill");
        assert!(silverbullet_skill.contains("name: silverbullet-space"));
        assert!(silverbullet_skill.contains("Space Lua / Lua Integrated Query"));
        assert!(silverbullet_skill.contains("r/system-guide"));

        let current_stack_skills = [
            ("mem0-memory", "memory_search"),
            ("scoped-kv", "kv_get"),
            ("telegram-operator-workflow", "/status"),
            ("browser-search", "web_search"),
            ("file-artifact-workflow", "deliver_file"),
            ("planning-session-lifecycle", "continue_later"),
            ("agent-browser", "browser_open"),
        ];
        for (skill_name, expected_fragment) in current_stack_skills {
            let skill =
                fs::read_to_string(default_home.join(format!("skills/{skill_name}/SKILL.md")))
                    .unwrap_or_else(|source| panic!("read {skill_name} skill: {source}"));
            assert!(skill.contains(&format!("name: {skill_name}")));
            assert!(skill.contains(expected_fragment));
        }

        assert!(
            !default_home
                .join("skills/lightpanda-browser/SKILL.md")
                .exists()
        );
        assert!(!default_home.join("skills/logseq-graph/SKILL.md").exists());
        assert!(!default_home.join("skills/obsidian-vault/SKILL.md").exists());
    }
}
