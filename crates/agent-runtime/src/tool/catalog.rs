use super::{ToolCall, ToolFamily, ToolName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPolicy {
    pub read_only: bool,
    pub destructive: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub family: ToolFamily,
    pub description: &'static str,
    pub policy: ToolPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCatalog {
    pub families: Vec<&'static str>,
    definitions: Vec<ToolDefinition>,
}

impl ToolCatalog {
    pub fn definition(&self, name: ToolName) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == name)
    }

    pub fn definition_for_call(&self, call: &ToolCall) -> Option<&ToolDefinition> {
        self.definition(call.name())
    }

    pub fn all_definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn automatic_model_definitions(&self) -> Vec<&ToolDefinition> {
        self.definitions
            .iter()
            .filter(|definition| {
                matches!(
                    definition.name,
                    ToolName::FsReadText
                        | ToolName::FsReadLines
                        | ToolName::FsList
                        | ToolName::FsGlob
                        | ToolName::FsSearchText
                        | ToolName::FsFindInFiles
                        | ToolName::FsWriteText
                        | ToolName::FsPatchText
                        | ToolName::FsReplaceLines
                        | ToolName::FsInsertText
                        | ToolName::FsMkdir
                        | ToolName::FsMove
                        | ToolName::FsTrash
                        | ToolName::ExecStart
                        | ToolName::ExecReadOutput
                        | ToolName::ExecWait
                        | ToolName::ExecKill
                        | ToolName::WebFetch
                        | ToolName::WebSearch
                        | ToolName::BrowserOpen
                        | ToolName::BrowserSnapshot
                        | ToolName::BrowserText
                        | ToolName::BrowserClick
                        | ToolName::BrowserFill
                        | ToolName::BrowserPress
                        | ToolName::BrowserWait
                        | ToolName::BrowserScroll
                        | ToolName::BrowserEval
                        | ToolName::BrowserScreenshot
                        | ToolName::BrowserPdf
                        | ToolName::BrowserStatus
                        | ToolName::BrowserClose
                        | ToolName::InitPlan
                        | ToolName::AddTask
                        | ToolName::SetTaskStatus
                        | ToolName::AddTaskNote
                        | ToolName::EditTask
                        | ToolName::PlanSnapshot
                        | ToolName::PlanLint
                        | ToolName::PromptBudgetRead
                        | ToolName::PromptBudgetUpdate
                        | ToolName::AutonomyStateRead
                        | ToolName::SkillList
                        | ToolName::SkillRead
                        | ToolName::SkillEnable
                        | ToolName::SkillDisable
                        | ToolName::ArtifactRead
                        | ToolName::ArtifactSearch
                        | ToolName::ArtifactPin
                        | ToolName::ArtifactUnpin
                        | ToolName::DeliverFile
                        | ToolName::MemoryAdd
                        | ToolName::MemorySearch
                        | ToolName::MemoryList
                        | ToolName::MemoryUpdate
                        | ToolName::MemoryDelete
                        | ToolName::KvGet
                        | ToolName::KvPut
                        | ToolName::KvList
                        | ToolName::KvDelete
                        | ToolName::KnowledgeSearch
                        | ToolName::KnowledgeRead
                        | ToolName::SessionSearch
                        | ToolName::SessionRead
                        | ToolName::SessionWait
                        | ToolName::McpSearchResources
                        | ToolName::McpReadResource
                        | ToolName::McpSearchPrompts
                        | ToolName::McpGetPrompt
                        | ToolName::AgentList
                        | ToolName::AgentRead
                        | ToolName::AgentCreate
                        | ToolName::ContinueLater
                        | ToolName::ScheduleList
                        | ToolName::ScheduleRead
                        | ToolName::ScheduleCreate
                        | ToolName::ScheduleUpdate
                        | ToolName::ScheduleDelete
                        | ToolName::MessageAgent
                        | ToolName::GrantAgentChainContinuation
                )
            })
            .collect()
    }

    fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: ToolName::FsRead,
                family: ToolFamily::Filesystem,
                description: "Read a UTF-8 text file from the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsWrite,
                family: ToolFamily::Filesystem,
                description: "Write a UTF-8 text file inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsPatch,
                family: ToolFamily::Filesystem,
                description: "Apply exact text edits to a UTF-8 text file inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsReadText,
                family: ToolFamily::Filesystem,
                description: "Read a UTF-8 text file from the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsReadLines,
                family: ToolFamily::Filesystem,
                description: "Read an inclusive line range from a UTF-8 text file and report file bounds",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsSearchText,
                family: ToolFamily::Filesystem,
                description: "Search for literal text within a single UTF-8 text file",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsFindInFiles,
                family: ToolFamily::Filesystem,
                description: "Search for literal text across regular workspace files, optionally constrained by glob; special files such as sockets are ignored",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsWriteText,
                family: ToolFamily::Filesystem,
                description: "Write full UTF-8 text to a file with explicit create, overwrite, or upsert semantics",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsPatchText,
                family: ToolFamily::Filesystem,
                description: "Replace one exact text fragment inside a UTF-8 text file. Use JSON fields `search` and `replace`; do not send `old`/`new` patch arrays",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsReplaceLines,
                family: ToolFamily::Filesystem,
                description: "Replace an explicit inclusive line range inside a UTF-8 text file",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsInsertText,
                family: ToolFamily::Filesystem,
                description: "Insert text before or after a line, or prepend or append to a file",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsMkdir,
                family: ToolFamily::Filesystem,
                description: "Create a directory inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsMove,
                family: ToolFamily::Filesystem,
                description: "Move or rename a file or directory inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsTrash,
                family: ToolFamily::Filesystem,
                description: "Move a file or directory into workspace trash instead of deleting it permanently",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsList,
                family: ToolFamily::Filesystem,
                description: "List files and directories inside the workspace with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsGlob,
                family: ToolFamily::Filesystem,
                description: "Match workspace paths with glob-style patterns",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsSearch,
                family: ToolFamily::Filesystem,
                description: "Search text content inside the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::WebFetch,
                family: ToolFamily::Web,
                description: "Fetch an exact URL and return readable response text; prefer web_search first unless the user supplied the URL, web_search returned it, or it is a known canonical source",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::WebSearch,
                family: ToolFamily::Web,
                description: "Search first for current or external information via the configured backend; deployments may use SearXNG",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserOpen,
                family: ToolFamily::Browser,
                description: "Open a URL through agent-browser using the session-scoped real browser backend",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserSnapshot,
                family: ToolFamily::Browser,
                description: "Read an agent-browser accessibility snapshot with fresh @eN refs for click/fill actions",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserText,
                family: ToolFamily::Browser,
                description: "Read visible text from the browser page or selector through agent-browser",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserClick,
                family: ToolFamily::Browser,
                description: "Click an agent-browser @eN ref or selector, then re-snapshot before using refs again",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserFill,
                family: ToolFamily::Browser,
                description: "Fill an input by agent-browser @eN ref or selector",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserPress,
                family: ToolFamily::Browser,
                description: "Press a key in the current browser page, for example Enter or Control+a",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserWait,
                family: ToolFamily::Browser,
                description: "Wait for browser state: selector, text, URL, load state, JS function, or duration",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserScroll,
                family: ToolFamily::Browser,
                description: "Scroll the current browser page up, down, left, or right",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserEval,
                family: ToolFamily::Browser,
                description: "Evaluate JavaScript in the current browser page and return bounded output",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserScreenshot,
                family: ToolFamily::Browser,
                description: "Capture a browser screenshot into a workspace file; use deliver_file with workspace_path to send it",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserPdf,
                family: ToolFamily::Browser,
                description: "Capture the current browser page as a PDF file inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserStatus,
                family: ToolFamily::Browser,
                description: "Report current browser session, URL, and title through agent-browser",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::BrowserClose,
                family: ToolFamily::Browser,
                description: "Close the current agent-browser session; all=true closes all agent-browser sessions",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecStart,
                family: ToolFamily::Exec,
                description: "Start a structured executable plus args process",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::ExecReadOutput,
                family: ToolFamily::Exec,
                description: "Read bounded live output from a structured exec process",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecWait,
                family: ToolFamily::Exec,
                description: "Wait for a structured exec process to finish",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecKill,
                family: ToolFamily::Exec,
                description: "Kill a structured exec process",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::PlanRead,
                family: ToolFamily::Planning,
                description: "Read the structured plan snapshot for the current session",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanWrite,
                family: ToolFamily::Planning,
                description: "Replace the structured plan snapshot for the current session",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::InitPlan,
                family: ToolFamily::Planning,
                description: "Initialize a structured session plan with a top-level goal",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AddTask,
                family: ToolFamily::Planning,
                description: "Add a task to the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SetTaskStatus,
                family: ToolFamily::Planning,
                description: "Update the status of a task in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AddTaskNote,
                family: ToolFamily::Planning,
                description: "Append a note to a task in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::EditTask,
                family: ToolFamily::Planning,
                description: "Edit a task description, dependencies, or parent relationship in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanSnapshot,
                family: ToolFamily::Planning,
                description: "Read the current structured session plan including goal and tasks",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanLint,
                family: ToolFamily::Planning,
                description: "Validate the current structured session plan and report issues",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PromptBudgetRead,
                family: ToolFamily::Planning,
                description: "Read the effective prompt budget policy for the next full prompt assembly, including usable context basis, layer percentages, target tokens, and pending one-shot override state",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PromptBudgetUpdate,
                family: ToolFamily::Planning,
                description: "Update or reset prompt budget policy. scope=session persists a session policy; scope=next_turn queues a one-shot override for the next full prompt assembly. Percentages must sum to 100 after merging",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AutonomyStateRead,
                family: ToolFamily::Memory,
                description: "Read a bounded aggregate autonomy state for the current session: related schedules, active jobs, delegated child sessions, inbox events, inter-agent chain metadata, and configured mesh/A2A peers.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillList,
                family: ToolFamily::Memory,
                description: "List skills available to the current session, including activation mode and source paths. Use before enabling, disabling, or reading a skill.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillRead,
                family: ToolFamily::Memory,
                description: "Read one skill's SKILL.md body by name with an optional max_bytes bound. Use this instead of guessing skill instructions.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillEnable,
                family: ToolFamily::Memory,
                description: "Manually enable one skill for the current session. This changes session settings only; use skill_list or skill_read first if unsure.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillDisable,
                family: ToolFamily::Memory,
                description: "Manually disable one skill for the current session. This changes session settings only; use when an automatic skill is distracting or wrong.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactRead,
                family: ToolFamily::Offload,
                description: "Read a bounded page of an offloaded context artifact by artifact_id; use next_offset to continue large payloads",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactSearch,
                family: ToolFamily::Offload,
                description: "Search across the current session's offloaded context references and payloads",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactPin,
                family: ToolFamily::Offload,
                description: "Manually pin an offloaded context artifact so its ref is prioritized in future prompt OffloadRefs",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactUnpin,
                family: ToolFamily::Offload,
                description: "Remove a manual pin from an offloaded context artifact; frequently read artifacts may still be auto-pinned",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::DeliverFile,
                family: ToolFamily::Offload,
                description: "Queue a file for delivery to the current operator surface. Prefer workspace_path for normal files created or found in the current workspace; use artifact_id only for an existing session artifact such as a user-uploaded file or offloaded output. Do not read file contents before delivery when the path is already known. Do not create or mention artifacts just to send a workspace file. A successful tool result means status=queued, not delivered yet; Telegram sends queued files as documents after the current turn and reports delivery failures to the chat. Do not invent Obsidian/vault fallback paths.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MemoryAdd,
                family: ToolFamily::Memory,
                description: "Store an explicit, inspectable long-term semantic memory through the configured Mem0 backend. Use only for confirmed durable preferences, decisions, project facts, or lessons; do not store secrets or raw transcripts by default.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MemorySearch,
                family: ToolFamily::Memory,
                description: "Search the configured Mem0 semantic memory backend for relevant durable preferences, decisions, facts, or lessons. Results are bounded and visible in the tool ledger.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MemoryList,
                family: ToolFamily::Memory,
                description: "List bounded long-term memories from the configured Mem0 backend for the current scope or explicit filters.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MemoryUpdate,
                family: ToolFamily::Memory,
                description: "Replace one existing long-term memory by exact memory_id. Search or list first; do not guess ids.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MemoryDelete,
                family: ToolFamily::Memory,
                description: "Delete one existing long-term memory by exact memory_id. Use only when the operator asks to remove or correct a memory.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::KvGet,
                family: ToolFamily::Memory,
                description: "Read one exact key from the scoped runtime KV store in state.sqlite. Use for deterministic state, not semantic recall.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KvPut,
                family: ToolFamily::Memory,
                description: "Store one exact JSON value in the scoped runtime KV store in state.sqlite. Supports optional expected_revision compare-and-set and ttl_seconds.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KvList,
                family: ToolFamily::Memory,
                description: "List exact keys from one scoped runtime KV namespace with optional prefix and pagination.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KvDelete,
                family: ToolFamily::Memory,
                description: "Delete one exact key from the scoped runtime KV store. Use only when stale state should be removed.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::KnowledgeSearch,
                family: ToolFamily::Memory,
                description: "Search project knowledge roots with bounded pagination and canonical source metadata",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KnowledgeRead,
                family: ToolFamily::Memory,
                description: "Read one project knowledge source in a bounded excerpt or full-text view; enum-like arguments such as mode must be quoted JSON strings",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionSearch,
                family: ToolFamily::Memory,
                description: "Search historical sessions with bounded pagination so you can find the exact session_id before reading it",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionRead,
                family: ToolFamily::Memory,
                description: "Read one session snapshot in bounded summary, timeline, transcript, or artifact views; this inspects current data and does not wait for new work",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionWait,
                family: ToolFamily::Agent,
                description: "Wait for queued or running work in a session to settle, then return a bounded session snapshot; use this after message_agent when you need the other agent's reply before concluding. If you set mode, send it as a quoted JSON string, for example {\"session_id\":\"...\",\"mode\":\"transcript\"}",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpCall,
                family: ToolFamily::Mcp,
                description: "Call one dynamically discovered MCP tool through the canonical runtime path",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::McpSearchResources,
                family: ToolFamily::Mcp,
                description: "Search discovered MCP resources with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpReadResource,
                family: ToolFamily::Mcp,
                description: "Read one MCP resource by connector id and URI",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpSearchPrompts,
                family: ToolFamily::Mcp,
                description: "Search discovered MCP prompts with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpGetPrompt,
                family: ToolFamily::Mcp,
                description: "Fetch one MCP prompt by connector id and prompt name",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentList,
                family: ToolFamily::Agent,
                description: "List available agent profiles with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentRead,
                family: ToolFamily::Agent,
                description: "Read one agent profile by id or name",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentCreate,
                family: ToolFamily::Agent,
                description: "Create a new agent profile from a built-in or existing template",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::ContinueLater,
                family: ToolFamily::Agent,
                description: "Create a self-addressed one-shot timer in the same session; use this when the user asks you to remind or message them later. If you set delivery_mode, send it as a quoted JSON string, for example {\"delay_seconds\":300,\"handoff_payload\":\"...\",\"delivery_mode\":\"existing_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleList,
                family: ToolFamily::Agent,
                description: "List agent schedules for the current workspace with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleRead,
                family: ToolFamily::Agent,
                description: "Read one agent schedule by id",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleCreate,
                family: ToolFamily::Agent,
                description: "Create an advanced or recurring agent schedule in the current workspace. For simple one-shot reminders, prefer continue_later. If you set mode or delivery_mode, send them as quoted JSON strings, for example {\"id\":\"nightly\",\"prompt\":\"...\",\"interval_seconds\":3600,\"mode\":\"once\",\"delivery_mode\":\"fresh_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleUpdate,
                family: ToolFamily::Agent,
                description: "Update an existing agent schedule in the current workspace. If you set mode or delivery_mode, send them as quoted JSON strings, for example {\"id\":\"nightly\",\"mode\":\"once\",\"delivery_mode\":\"existing_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleDelete,
                family: ToolFamily::Agent,
                description: "Delete an existing agent schedule in the current workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MessageAgent,
                family: ToolFamily::Agent,
                description: "Queue an asynchronous message to another agent by creating a fresh recipient session and background job; this returns recipient ids and does not wait for the reply",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::GrantAgentChainContinuation,
                family: ToolFamily::Agent,
                description: "Grant exactly one additional hop to a blocked inter-agent chain after you have confirmed the chain hit max_hops",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
        ]
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec![
                "fs", "web", "browser", "exec", "plan", "offload", "memory", "mcp", "agent",
            ],
            definitions: Self::definitions(),
        }
    }
}
