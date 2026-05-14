use super::{ToolDefinition, ToolName};
use serde_json::{Value, json};

impl ToolDefinition {
    pub fn openai_function_schema(&self) -> Value {
        json!({
            "type": "function",
            "name": self.name.as_str(),
            "description": self.description,
            "parameters": self.name.input_schema(),
        })
    }
}
impl ToolName {
    pub fn example_arguments_json(self) -> Option<&'static str> {
        Some(match self {
            Self::FsReadText => r#"{"path":"README.md"}"#,
            Self::FsReadLines => r#"{"path":"README.md","start_line":1,"end_line":5}"#,
            Self::FsSearchText => r#"{"path":"README.md","query":"teamD"}"#,
            Self::FsFindInFiles => r#"{"query":"teamD","glob":"*.md","limit":5}"#,
            Self::FsWriteText => {
                r#"{"path":"notes/example.md","content":"hello\n","mode":"create"}"#
            }
            Self::FsPatchText => {
                r#"{"path":"notes/example.md","search":"old text","replace":"new text"}"#
            }
            Self::FsReplaceLines => {
                r#"{"path":"notes/example.md","start_line":1,"end_line":1,"content":"new line\n"}"#
            }
            Self::FsInsertText => {
                r#"{"path":"notes/example.md","position":"append","content":"tail\n"}"#
            }
            Self::FsMkdir => r#"{"path":"notes"}"#,
            Self::FsMove => r#"{"src":"notes/old.md","dest":"notes/new.md"}"#,
            Self::FsTrash => r#"{"path":"notes/old.md"}"#,
            Self::FsList => r#"{"path":".","recursive":false,"limit":20}"#,
            Self::FsGlob => r#"{"path":".","pattern":"*.md","limit":20}"#,
            Self::WebFetch => r#"{"url":"https://example.com"}"#,
            Self::WebSearch => r#"{"query":"teamD agent runtime","limit":3}"#,
            Self::BrowserOpen => r#"{"url":"https://example.com","wait_until":"load"}"#,
            Self::BrowserSnapshot => r#"{"interactive":true,"compact":true,"max_chars":12000}"#,
            Self::BrowserText => r#"{"selector":"body","max_chars":12000}"#,
            Self::BrowserClick => r#"{"selector":"@e1","wait_until":"networkidle"}"#,
            Self::BrowserFill => r#"{"selector":"input[name=q]","text":"teamD"}"#,
            Self::BrowserPress => r#"{"key":"Enter"}"#,
            Self::BrowserWait => r#"{"kind":"selector","value":"main","state":"visible"}"#,
            Self::BrowserScroll => r#"{"direction":"down","pixels":800}"#,
            Self::BrowserEval => r#"{"script":"document.title","max_chars":4000}"#,
            Self::BrowserScreenshot => {
                r#"{"path":"scratch/browser/page.png","full":true,"annotate":false}"#
            }
            Self::BrowserPdf => r#"{"path":"scratch/browser/page.pdf"}"#,
            Self::BrowserStatus => r#"{}"#,
            Self::BrowserClose => r#"{"all":false}"#,
            Self::ExecStart => r#"{"executable":"echo","args":["hello"],"cwd":"."}"#,
            Self::ExecReadOutput => r#"{"process_id":"exec-1","stream":"merged","max_lines":20}"#,
            Self::ExecWait => r#"{"process_id":"exec-1","timeout_ms":60000}"#,
            Self::ExecKill => r#"{"process_id":"exec-1"}"#,
            Self::InitPlan => r#"{"goal":"Ship the requested change"}"#,
            Self::AddTask => r#"{"description":"Implement the runtime change","depends_on":[]}"#,
            Self::SetTaskStatus => r#"{"task_id":"task-1","new_status":"in_progress"}"#,
            Self::AddTaskNote => r#"{"task_id":"task-1","note":"Found the parser seam"}"#,
            Self::EditTask => r#"{"task_id":"task-1","description":"Updated task text"}"#,
            Self::PlanSnapshot => r#"{}"#,
            Self::PlanLint => r#"{}"#,
            Self::PromptBudgetRead => r#"{}"#,
            Self::PromptBudgetUpdate => {
                r#"{"scope":"next_turn","reset":true,"reason":"clear temporary override"}"#
            }
            Self::AutonomyStateRead => r#"{"max_items":5,"include_inactive_schedules":false}"#,
            Self::SkillList => r#"{"include_inactive":true,"limit":20}"#,
            Self::SkillRead => r#"{"name":"silverbullet","path":"SKILL.md","max_bytes":8000}"#,
            Self::SkillEnable => r#"{"name":"silverbullet"}"#,
            Self::SkillDisable => r#"{"name":"silverbullet"}"#,
            Self::SkillInstall => {
                r#"{"source_dir":"skills/mail-workflow","name":"mail-workflow","enable":true}"#
            }
            Self::ArtifactRead => r#"{"artifact_id":"artifact-1","max_bytes":8000}"#,
            Self::ArtifactSearch => r#"{"query":"report","limit":5}"#,
            Self::ArtifactPin => r#"{"artifact_id":"artifact-1"}"#,
            Self::ArtifactUnpin => r#"{"artifact_id":"artifact-1"}"#,
            Self::DeliverFile => r#"{"workspace_path":"reports/result.pdf","caption":"Done"}"#,
            Self::MemoryAdd => {
                r#"{"text":"User prefers concise answers.","scope":"operator","infer":false}"#
            }
            Self::MemorySearch => {
                r#"{"query":"preferred response style","scope":"operator","limit":5}"#
            }
            Self::MemoryList => r#"{"scope":"workspace","limit":10}"#,
            Self::MemoryUpdate => {
                r#"{"memory_id":"mem-1","text":"User prefers concise direct answers."}"#
            }
            Self::MemoryDelete => r#"{"memory_id":"mem-1"}"#,
            Self::KvGet => r#"{"key":"state/current_task","scope":"agent_shared"}"#,
            Self::KvPut => {
                r#"{"key":"state/current_task","value":{"id":"task-1"},"scope":"agent_shared"}"#
            }
            Self::KvList => r#"{"scope":"agent_shared","prefix":"state/","limit":10}"#,
            Self::KvDelete => r#"{"key":"state/current_task","scope":"agent_shared"}"#,
            Self::KnowledgeSearch => r#"{"query":"prompt contract","limit":5,"roots":["docs"]}"#,
            Self::KnowledgeRead => {
                r#"{"path":"docs/current/00-overview.md","mode":"excerpt","max_lines":20}"#
            }
            Self::SessionSearch => r#"{"query":"telegram routing","limit":5}"#,
            Self::SessionRead => r#"{"session_id":"session-1","mode":"transcript","max_items":10}"#,
            Self::SessionWait => {
                r#"{"session_id":"session-agentmsg-1","wait_timeout_ms":1000,"mode":"transcript","max_items":10}"#
            }
            Self::McpSearchResources => r#"{"query":"project","limit":5}"#,
            Self::McpReadResource => r#"{"connector_id":"silverbullet","uri":"sb://README.md"}"#,
            Self::McpSearchPrompts => r#"{"query":"summary","limit":5}"#,
            Self::McpGetPrompt => {
                r#"{"connector_id":"silverbullet","name":"summarize","arguments":{"topic":"teamD"}}"#
            }
            Self::AgentList => r#"{"limit":10}"#,
            Self::AgentRead => r#"{"identifier":"default"}"#,
            Self::AgentCreate => r#"{"name":"Monitoring Agent","template_identifier":"default"}"#,
            Self::ContinueLater => {
                r#"{"delay_seconds":120,"handoff_payload":"Send the reminder now.","delivery_mode":"existing_session"}"#
            }
            Self::ScheduleList => r#"{"limit":10}"#,
            Self::ScheduleRead => r#"{"id":"daily-status"}"#,
            Self::ScheduleCreate => {
                r#"{"id":"daily-status","prompt":"Send daily status.","mode":"interval","delivery_mode":"fresh_session","interval_seconds":86400}"#
            }
            Self::ScheduleUpdate => r#"{"id":"daily-status","enabled":false}"#,
            Self::ScheduleDelete => r#"{"id":"daily-status"}"#,
            Self::MessageAgent => {
                r#"{"target_agent_id":"judge","message":"Review the current plan and return risks."}"#
            }
            Self::GrantAgentChainContinuation => {
                r#"{"chain_id":"chain-session-1-1","reason":"Need one final review response."}"#
            }
            _ => return None,
        })
    }

    pub fn input_schema(self) -> Value {
        match self {
            Self::FsRead => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsReadText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsReadLines => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 }
                },
                "required": ["path", "start_line", "end_line"],
                "additionalProperties": false,
            }),
            Self::FsWrite => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to write" },
                    "content": { "type": "string", "description": "UTF-8 file content to write" }
                },
                "required": ["path", "content"],
                "additionalProperties": false,
            }),
            Self::FsWriteText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to write" },
                    "content": { "type": "string", "description": "UTF-8 file content to write" },
                    "mode": { "type": "string", "enum": ["create", "overwrite", "upsert"] }
                },
                "required": ["path", "content", "mode"],
                "additionalProperties": false,
            }),
            Self::FsPatch => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": { "type": "string" },
                                "new": { "type": "string" },
                                "replace_all": { "type": "boolean" }
                            },
                            "required": ["old", "new", "replace_all"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["path", "edits"],
                "additionalProperties": false,
            }),
            Self::FsPatchText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "search": { "type": "string", "description": "Exact existing text fragment to replace. Do not send old/new patch arrays; use this `search` field directly." },
                    "replace": { "type": "string", "description": "Replacement text for the first matching `search` fragment." }
                },
                "required": ["path", "search", "replace"],
                "additionalProperties": false,
            }),
            Self::FsReplaceLines => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "content": { "type": "string" }
                },
                "required": ["path", "start_line", "end_line", "content"],
                "additionalProperties": false,
            }),
            Self::FsInsertText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to modify" },
                    "line": { "type": ["integer", "null"], "minimum": 1 },
                    "position": { "type": "string", "enum": ["before", "after", "prepend", "append"] },
                    "content": { "type": "string" }
                },
                "required": ["path", "position", "content"],
                "additionalProperties": false,
            }),
            Self::FsList => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to list" },
                    "recursive": { "type": "boolean", "description": "Whether to recurse into subdirectories" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of entries to return; defaults to a safe bounded page size" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based offset for continuing a previous paginated listing" }
                },
                "required": ["path", "recursive"],
                "additionalProperties": false,
            }),
            Self::FsGlob => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace root to search from" },
                    "pattern": { "type": "string", "description": "Glob-style path pattern" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of matches to return; defaults to a safe bounded page size" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based offset for continuing a previous paginated glob result" }
                },
                "required": ["path", "pattern"],
                "additionalProperties": false,
            }),
            Self::FsSearch => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace root to search from" },
                    "query": { "type": "string", "description": "Literal text to search for" }
                },
                "required": ["path", "query"],
                "additionalProperties": false,
            }),
            Self::FsSearchText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace file to search" },
                    "query": { "type": "string", "description": "Literal text to search for" }
                },
                "required": ["path", "query"],
                "additionalProperties": false,
            }),
            Self::FsFindInFiles => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Literal text to search for" },
                    "glob": { "type": ["string", "null"], "description": "Optional glob used to filter matching paths" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of matches" }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::FsMkdir => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative directory path to create" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsMove => json!({
                "type": "object",
                "properties": {
                    "src": { "type": "string", "description": "Relative source path" },
                    "dest": { "type": "string", "description": "Relative destination path" }
                },
                "required": ["src", "dest"],
                "additionalProperties": false,
            }),
            Self::FsTrash => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to move into trash" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::WebFetch => json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Absolute exact URL to fetch and convert into readable text. Prefer web_search first unless the user supplied this URL, web_search returned it, or it is a known canonical source." }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
            Self::WebSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query text. Use this first for current or external information; configured deployments may use SearXNG." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of results" }
                },
                "required": ["query", "limit"],
                "additionalProperties": false,
            }),
            Self::BrowserOpen => json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Absolute URL to open in the session-scoped agent-browser instance" },
                    "wait_until": { "type": ["string", "null"], "enum": ["load", "domcontentloaded", "networkidle", null], "description": "Optional load state to wait for after navigation; use networkidle for SPAs" }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
            Self::BrowserSnapshot => json!({
                "type": "object",
                "properties": {
                    "interactive": { "type": ["boolean", "null"], "description": "When true, return interactive elements only. Prefer true for @eN ref workflows." },
                    "compact": { "type": ["boolean", "null"], "description": "When true, remove empty structural nodes to reduce tokens." },
                    "depth": { "type": ["integer", "null"], "minimum": 1, "description": "Optional tree depth cap." },
                    "selector": { "type": ["string", "null"], "description": "Optional CSS selector scope for the snapshot." },
                    "max_chars": { "type": ["integer", "null"], "minimum": 1000, "description": "Optional output cap. Refs like @eN are fresh per snapshot and become stale after page changes." }
                },
                "additionalProperties": false,
            }),
            Self::BrowserText => json!({
                "type": "object",
                "properties": {
                    "selector": { "type": ["string", "null"], "description": "Optional selector or @eN ref; defaults to body." },
                    "max_chars": { "type": ["integer", "null"], "minimum": 1000, "description": "Optional output cap." }
                },
                "additionalProperties": false,
            }),
            Self::BrowserClick => json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "agent-browser @eN ref from the latest snapshot, CSS selector, or supported locator." },
                    "wait_until": { "type": ["string", "null"], "enum": ["load", "domcontentloaded", "networkidle", null], "description": "Optional load state to wait for after the click. Re-snapshot before using @eN refs again." }
                },
                "required": ["selector"],
                "additionalProperties": false,
            }),
            Self::BrowserFill => json!({
                "type": "object",
                "properties": {
                    "selector": { "type": "string", "description": "agent-browser @eN ref from the latest snapshot, CSS selector, or supported locator." },
                    "text": { "type": "string", "description": "Text to fill into the target input." }
                },
                "required": ["selector", "text"],
                "additionalProperties": false,
            }),
            Self::BrowserPress => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Key or key chord, for example Enter, Tab, or Control+a." }
                },
                "required": ["key"],
                "additionalProperties": false,
            }),
            Self::BrowserWait => json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["selector", "text", "url", "load", "function", "duration_ms"], "description": "What to wait for." },
                    "value": { "type": ["string", "null"], "description": "Selector/text/url glob/load state/JS expression/duration milliseconds depending on kind." },
                    "state": { "type": ["string", "null"], "enum": ["visible", "hidden", "attached", "detached", null], "description": "Optional selector state." }
                },
                "required": ["kind"],
                "additionalProperties": false,
            }),
            Self::BrowserScroll => json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
                    "pixels": { "type": ["integer", "null"], "minimum": 1 }
                },
                "required": ["direction"],
                "additionalProperties": false,
            }),
            Self::BrowserEval => json!({
                "type": "object",
                "properties": {
                    "script": { "type": "string", "description": "JavaScript expression or script to evaluate in the current page." },
                    "max_chars": { "type": ["integer", "null"], "minimum": 1000, "description": "Optional output cap." }
                },
                "required": ["script"],
                "additionalProperties": false,
            }),
            Self::BrowserScreenshot => json!({
                "type": "object",
                "properties": {
                    "path": { "type": ["string", "null"], "description": "Optional workspace path for the PNG/JPEG screenshot; defaults under scratch/browser/." },
                    "full": { "type": ["boolean", "null"], "description": "Capture full page when true." },
                    "annotate": { "type": ["boolean", "null"], "description": "Annotate screenshot with numbered element labels." }
                },
                "additionalProperties": false,
            }),
            Self::BrowserPdf => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Workspace path for the generated PDF." }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::BrowserStatus | Self::BrowserClose => match self {
                Self::BrowserStatus => json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false,
                }),
                Self::BrowserClose => json!({
                    "type": "object",
                    "properties": {
                        "all": { "type": ["boolean", "null"], "description": "When true, close all agent-browser sessions. Prefer false for normal cleanup." }
                    },
                    "additionalProperties": false,
                }),
                _ => unreachable!(),
            },
            Self::ExecStart => json!({
                "type": "object",
                "properties": {
                    "executable": { "type": "string" },
                    "args": { "type": "array", "items": { "type": "string" } },
                    "cwd": { "type": ["string", "null"] }
                },
                "required": ["executable", "args"],
                "additionalProperties": false,
            }),
            Self::ExecReadOutput => json!({
                "type": "object",
                "properties": {
                    "process_id": { "type": "string", "description": "Process id returned by exec_start" },
                    "stream": { "type": ["string", "null"], "enum": ["merged", "stdout", "stderr", null], "description": "Which stream view to read; defaults to merged" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional cursor returned by a previous exec_read_output call" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of UTF-8 bytes to return" },
                    "max_lines": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of lines to return" }
                },
                "required": ["process_id"],
                "additionalProperties": false,
            }),
            Self::ExecWait => json!({
                "type": "object",
                "properties": {
                    "process_id": { "type": "string" },
                    "timeout_ms": {
                        "type": ["integer", "null"],
                        "minimum": 1,
                        "description": "Optional hard wait timeout in milliseconds. Defaults to 10 minutes and is capped by the runtime."
                    }
                },
                "required": ["process_id"],
                "additionalProperties": false,
            }),
            Self::ExecKill => json!({
                "type": "object",
                "properties": {
                    "process_id": { "type": "string" }
                },
                "required": ["process_id"],
                "additionalProperties": false,
            }),
            Self::PlanRead => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PlanWrite => json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "content": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"]
                                }
                            },
                            "required": ["id", "content", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["items"],
                "additionalProperties": false,
            }),
            Self::InitPlan => json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "Top-level goal for the structured session plan" }
                },
                "required": ["goal"],
                "additionalProperties": false,
            }),
            Self::AddTask => json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "Human-readable task description" },
                    "depends_on": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional task ids that must complete first"
                    },
                    "parent_task_id": { "type": ["string", "null"], "description": "Optional parent task id" }
                },
                "required": ["description"],
                "additionalProperties": false,
            }),
            Self::SetTaskStatus => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "new_status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed", "blocked", "cancelled"]
                    },
                    "blocked_reason": { "type": ["string", "null"] }
                },
                "required": ["task_id", "new_status"],
                "additionalProperties": false,
            }),
            Self::AddTaskNote => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["task_id", "note"],
                "additionalProperties": false,
            }),
            Self::EditTask => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "description": { "type": ["string", "null"] },
                    "depends_on": {
                        "type": ["array", "null"],
                        "items": { "type": "string" }
                    },
                    "parent_task_id": { "type": ["string", "null"] },
                    "clear_parent_task": { "type": "boolean" }
                },
                "required": ["task_id"],
                "additionalProperties": false,
            }),
            Self::PlanSnapshot | Self::PlanLint => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PromptBudgetRead => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PromptBudgetUpdate => json!({
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "enum": ["session", "next_turn"],
                        "description": "Update scope. Use session for persistent session policy. Use next_turn for a one-shot override applied to the next full prompt assembly only; provider continuation rounds do not rebuild the current prompt."
                    },
                    "reset": { "type": "boolean", "description": "Reset the selected prompt budget scope back to runtime defaults before applying any supplied percentages. With scope=next_turn and no percentages, this clears the queued one-shot override." },
                    "percentages": {
                        "type": ["object", "null"],
                        "description": "Optional layer percentage overrides. Supplied values merge into the selected scope's base policy; after merging, all percentages must sum to 100.",
                        "properties": {
                            "system": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "agents": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "active_skills": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "session_head": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "autonomy_state": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "memory_recall": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "plan": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "context_summary": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "offload_refs": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "recent_tool_activity": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "transcript_tail": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 }
                        },
                        "additionalProperties": false
                    },
                    "reason": { "type": ["string", "null"], "description": "Short explanation for audit/debug views" }
                },
                "additionalProperties": false,
            }),
            Self::SkillList => json!({
                "type": "object",
                "properties": {
                    "include_inactive": { "type": ["boolean", "null"], "description": "Whether to include inactive skills. Defaults to true so the model can discover the full skill catalog before activation." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of skills to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::AutonomyStateRead => json!({
                "type": "object",
                "properties": {
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of schedules, jobs, child sessions, inbox events, and mesh peers per section" },
                    "include_inactive_schedules": { "type": ["boolean", "null"], "description": "Whether to include disabled schedules in the related schedules section. Defaults to false." }
                },
                "additionalProperties": false,
            }),
            Self::SkillRead => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list" },
                    "path": { "type": ["string", "null"], "description": "Optional relative path inside the skill directory. Defaults to SKILL.md. Use this to read examples, templates, scripts, or other files shipped with the skill. Absolute paths and .. are rejected." },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum UTF-8 bytes from the requested skill file to return" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::SkillEnable => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list to manually enable for the current session" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::SkillInstall => json!({
                "type": "object",
                "properties": {
                    "source_dir": { "type": "string", "description": "Relative workspace directory containing SKILL.md plus optional scripts/references/data. Do not pass a SKILL.md file path; pass its parent directory." },
                    "name": { "type": ["string", "null"], "description": "Optional expected skill name. If provided, it must match SKILL.md frontmatter name." },
                    "enable": { "type": ["boolean", "null"], "description": "Whether to enable the installed skill for the current session. Defaults to true." },
                    "overwrite": { "type": ["boolean", "null"], "description": "Whether to replace an existing agent-local skill directory with the same name. Defaults to false." }
                },
                "required": ["source_dir"],
                "additionalProperties": false,
            }),
            Self::SkillDisable => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list to manually disable for the current session" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::ArtifactRead => json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact id from the offloaded context references block" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional byte offset returned as next_offset by a previous artifact_read call" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum UTF-8 bytes to return; defaults to a safe page, use next_offset to continue reading" }
                },
                "required": ["artifact_id"],
                "additionalProperties": false,
            }),
            Self::ArtifactSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query against offloaded context labels, summaries, and payloads" },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of matching artifacts to return" }
                },
                "required": ["query", "limit"],
                "additionalProperties": false,
            }),
            Self::ArtifactPin | Self::ArtifactUnpin => json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact id from the offloaded context references block" }
                },
                "required": ["artifact_id"],
                "additionalProperties": false,
            }),
            Self::DeliverFile => json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": ["string", "null"], "description": "Existing artifact id from the current session to send. Use this only when the file is already a session artifact, for example a Telegram upload or offloaded tool output. Do not create an artifact just to send a workspace file. Use either artifact_id or workspace_path, not both. Do not pass host paths or files from another session." },
                    "workspace_path": { "type": ["string", "null"], "description": "Preferred source for ordinary files: a relative path inside the current workspace. Use this for files you created with filesystem tools or found in the workspace. Do not read file contents before delivery when the path is already known; call deliver_file directly. The runtime stores bytes internally for durable delivery; do not mention that storage to the user. Use either workspace_path or artifact_id, not both." },
                    "file_name": { "type": ["string", "null"], "description": "Optional outward filename. Defaults to artifact metadata file_name or the workspace file name." },
                    "caption": { "type": ["string", "null"], "description": "Optional short caption shown with the document." },
                    "target": { "type": ["string", "null"], "enum": ["current_chat", null], "description": "Delivery target. Defaults to current_chat." }
                },
                "additionalProperties": false,
            }),
            Self::MemoryAdd => json!({
                "type": "object",
                "properties": {
                    "text": { "type": ["string", "null"], "description": "Explicit durable memory text to store. Use this for confirmed facts, preferences, durable decisions, or lessons. Either text or messages must be non-empty." },
                    "messages": {
                        "type": ["array", "null"],
                        "description": "Optional conversation turns for Mem0 to infer memories from. Use only when the conversation itself is the source; otherwise prefer explicit text.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "role": { "type": "string", "enum": ["user", "assistant", "system", "tool"] },
                                "content": { "type": "string" }
                            },
                            "required": ["role", "content"],
                            "additionalProperties": false
                        }
                    },
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "Memory scope. Defaults to workspace. Use operator for human preferences, agent for this profile, agent_shared for lessons all agents may reuse, workspace for project decisions, session for short-lived session facts." },
                    "infer": { "type": ["boolean", "null"], "description": "When false, store the supplied text as-is. When true or omitted, Mem0 may extract concise facts from messages/text." },
                    "metadata": { "type": ["object", "null"], "description": "Optional provenance/filter metadata. Do not put secrets here.", "additionalProperties": true }
                },
                "additionalProperties": false,
            }),
            Self::MemorySearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Semantic memory search query. Use this when a durable preference, prior decision, or lesson may help the current task." },
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "Optional scope restriction; defaults to workspace. Search one entity scope at a time; use repeated searches for multiple scopes." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of memories to return" },
                    "filters": { "type": ["object", "null"], "description": "Optional backend filter object. Prefer scope unless you know the exact metadata key.", "additionalProperties": true }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::MemoryList => json!({
                "type": "object",
                "properties": {
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "Optional scope restriction; defaults to workspace. List one entity scope at a time." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of memories to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset handled by teamD if the backend returns enough rows" },
                    "filters": { "type": ["object", "null"], "description": "Optional backend filter object. Prefer scope unless you know the exact metadata key.", "additionalProperties": true }
                },
                "additionalProperties": false,
            }),
            Self::MemoryUpdate => json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Exact memory id returned by memory_search or memory_list" },
                    "text": { "type": "string", "description": "Replacement memory text" },
                    "metadata": { "type": ["object", "null"], "description": "Optional metadata merge/update object", "additionalProperties": true }
                },
                "required": ["memory_id", "text"],
                "additionalProperties": false,
            }),
            Self::MemoryDelete => json!({
                "type": "object",
                "properties": {
                    "memory_id": { "type": "string", "description": "Exact memory id returned by memory_search or memory_list. Never delete broad memory without explicit user intent." }
                },
                "required": ["memory_id"],
                "additionalProperties": false,
            }),
            Self::KvGet => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Exact key inside the selected KV scope, for example state/current_task. Must not be empty." },
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "KV scope. Defaults to workspace. Uses the same logical scopes as semantic memory but stores exact state in the PostgreSQL-backed runtime store." }
                },
                "required": ["key"],
                "additionalProperties": false,
            }),
            Self::KvPut => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Exact key inside the selected KV scope, for example state/current_task. Use slash-separated namespaces; do not use secrets." },
                    "value": { "description": "JSON value to store exactly. Use this for deterministic state, counters, flags, cursors, or compact structured facts that should not be semantic-search memories." },
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "KV scope. Defaults to workspace. Use agent_shared for shared agent state, session for temporary per-session state." },
                    "metadata": { "type": ["object", "null"], "description": "Optional JSON metadata for audit/debug. Do not put secrets here.", "additionalProperties": true },
                    "expected_revision": { "type": ["integer", "null"], "minimum": 0, "description": "Optional compare-and-set guard. 0 means create only; current revision means update only if unchanged." },
                    "ttl_seconds": { "type": ["integer", "null"], "minimum": 1, "description": "Optional expiry in seconds. Expired entries are hidden from kv_get/kv_list." }
                },
                "required": ["key", "value"],
                "additionalProperties": false,
            }),
            Self::KvList => json!({
                "type": "object",
                "properties": {
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "KV scope. Defaults to workspace." },
                    "prefix": { "type": ["string", "null"], "description": "Optional key prefix, for example state/." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Maximum number of entries to return." },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Pagination offset." }
                },
                "additionalProperties": false,
            }),
            Self::KvDelete => json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Exact key to delete from the selected KV scope." },
                    "scope": { "type": ["string", "null"], "enum": ["operator", "agent", "agent_shared", "workspace", "session", null], "description": "KV scope. Defaults to workspace." },
                    "expected_revision": { "type": ["integer", "null"], "minimum": 0, "description": "Optional compare-and-set guard. If set, delete only when current revision matches." }
                },
                "required": ["key"],
                "additionalProperties": false,
            }),
            Self::KnowledgeSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Plain search terms for the PostgreSQL full-text canonical teamD docs/workspace knowledge index. Sources are only [knowledge].source_files/source_dirs inside the agent workspace; this is not arbitrary filesystem search, not SilverBullet, not Mem0, and not KV." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of search results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "kinds": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["root_doc", "project_doc", "project_note", "extra_doc"] },
                        "description": "Optional knowledge source kind filters"
                    },
                    "roots": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["root_docs", "docs", "projects", "notes", "extra"] },
                        "description": "Optional canonical knowledge root filters"
                    }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::KnowledgeRead => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative indexed knowledge source path returned by knowledge_search. Do not pass arbitrary filesystem or SilverBullet paths here; use fs_read_text/fs_read_lines or SilverBullet tools for those." },
                    "mode": { "type": ["string", "null"], "enum": ["excerpt", "full", null], "description": "Optional view mode as a quoted JSON string; use \"excerpt\" or \"full\". Defaults to excerpt" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based line cursor returned by a previous knowledge_read call" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum UTF-8 bytes to return" },
                    "max_lines": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of lines to return" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::SessionSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query across historical sessions so you can find the exact session_id before reading or waiting on a session" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of search results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "tiers": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["active", "warm", "cold"] },
                        "description": "Optional retention tier filters"
                    },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name filter" },
                    "updated_after": { "type": ["integer", "null"], "description": "Optional inclusive lower updated_at bound" },
                    "updated_before": { "type": ["integer", "null"], "description": "Optional inclusive upper updated_at bound" }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::SessionRead => json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session id to inspect without waiting for new work" },
                    "mode": { "type": ["string", "null"], "enum": ["summary", "timeline", "transcript", "artifacts", null], "description": "Optional bounded view mode as a quoted JSON string; use \"summary\", \"timeline\", \"transcript\", or \"artifacts\". Defaults to summary" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional item cursor returned by a previous session_read call" },
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of messages or artifacts to return" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum content bytes to return across message bodies" },
                    "include_tools": { "type": ["boolean", "null"], "description": "Whether transcript and timeline modes should include tool-role entries; defaults to true" }
                },
                "required": ["session_id"],
                "additionalProperties": false,
            }),
            Self::SessionWait => json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session id to wait on; after message_agent this should normally be the returned recipient_session_id. Use only when you explicitly need a bounded snapshot now." },
                    "wait_timeout_ms": { "type": ["integer", "null"], "minimum": 0, "description": "Optional wait timeout in milliseconds; defaults to the runtime request timeout and 0 means read immediately without waiting. Keep waits bounded; otherwise continue and inspect /tasks/task registry later." },
                    "mode": { "type": ["string", "null"], "enum": ["summary", "timeline", "transcript", "artifacts", null], "description": "Optional bounded view mode for the returned snapshot as a quoted JSON string; use \"summary\", \"timeline\", \"transcript\", or \"artifacts\". Defaults to transcript" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional item cursor returned by a previous session_wait or session_read call" },
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of messages or artifacts to return" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum content bytes to return across message bodies" },
                    "include_tools": { "type": ["boolean", "null"], "description": "Whether transcript and timeline modes should include tool-role entries; defaults to true" }
                },
                "required": ["session_id"],
                "additionalProperties": false,
            }),
            Self::McpCall => json!({
                "type": "object",
                "properties": {
                    "arguments": {
                        "type": "object",
                        "description": "Dynamic MCP tool arguments; this schema is replaced at runtime with the discovered MCP tool schema"
                    }
                },
                "additionalProperties": true,
            }),
            Self::McpSearchResources => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": ["string", "null"], "description": "Optional MCP connector id filter" },
                    "query": { "type": ["string", "null"], "description": "Optional search query against resource metadata" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::McpReadResource => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": "string", "description": "MCP connector id" },
                    "uri": { "type": "string", "description": "Resource URI to read" }
                },
                "required": ["connector_id", "uri"],
                "additionalProperties": false,
            }),
            Self::McpSearchPrompts => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": ["string", "null"], "description": "Optional MCP connector id filter" },
                    "query": { "type": ["string", "null"], "description": "Optional search query against prompt metadata" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::McpGetPrompt => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": "string", "description": "MCP connector id" },
                    "name": { "type": "string", "description": "Prompt name to retrieve" },
                    "arguments": {
                        "type": ["object", "null"],
                        "description": "Optional prompt arguments",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["connector_id", "name"],
                "additionalProperties": false,
            }),
            Self::AgentList => json!({
                "type": "object",
                "properties": {
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of agents to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::AgentRead => json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string", "description": "Agent id or name to resolve" }
                },
                "required": ["identifier"],
                "additionalProperties": false,
            }),
            Self::AgentCreate => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Display name for the new agent" },
                    "template_identifier": { "type": ["string", "null"], "description": "Optional template agent id or name; defaults to the current session agent" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::ContinueLater => json!({
                "type": "object",
                "properties": {
                    "delay_seconds": { "type": "integer", "minimum": 1, "description": "How many seconds to wait before the same session wakes up" },
                    "handoff_payload": { "type": "string", "description": "what to say or do when the timer fires; include the user's requested reminder text and any relevant context" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\". Defaults to existing_session, which resumes the same session and is the right default for reminders" }
                },
                "required": ["delay_seconds", "handoff_payload"],
                "additionalProperties": false,
            }),
            Self::ScheduleList => json!({
                "type": "object",
                "properties": {
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of schedules to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name filter" }
                },
                "additionalProperties": false,
            }),
            Self::ScheduleRead => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to read" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::ScheduleCreate => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Stable schedule id" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name; defaults to the current session agent" },
                    "prompt": { "type": "string", "description": "Prompt delivered when the advanced or recurring schedule fires" },
                    "mode": { "type": ["string", "null"], "enum": ["interval", "after_completion", "once", null], "description": "Optional schedule mode as a quoted JSON string; use \"interval\", \"after_completion\", or \"once\". Defaults to interval. Use once only for explicit one-shot schedules; for simple reminders use continue_later instead" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\". Defaults to fresh_session. Use existing_session when the result must appear in the current chat/session" },
                    "target_session_id": { "type": ["string", "null"], "description": "Optional target session id; for existing_session defaults to the current session" },
                    "interval_seconds": { "type": "integer", "minimum": 1, "description": "Positive schedule cadence in seconds; this is not the preferred field for a simple user reminder" },
                    "enabled": { "type": ["boolean", "null"], "description": "Optional enabled state; defaults to true" }
                },
                "required": ["id", "prompt", "interval_seconds"],
                "additionalProperties": false,
            }),
            Self::ScheduleUpdate => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to update" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name override" },
                    "prompt": { "type": ["string", "null"], "description": "Optional replacement prompt" },
                    "mode": { "type": ["string", "null"], "enum": ["interval", "after_completion", "once", null], "description": "Optional replacement mode as a quoted JSON string; use \"interval\", \"after_completion\", or \"once\"" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional replacement delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\"" },
                    "target_session_id": { "type": ["string", "null"], "description": "Optional replacement target session id" },
                    "interval_seconds": { "type": ["integer", "null"], "minimum": 1, "description": "Optional replacement interval in seconds" },
                    "enabled": { "type": ["boolean", "null"], "description": "Optional replacement enabled state" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::ScheduleDelete => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to delete" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::MessageAgent => json!({
                "type": "object",
                "properties": {
                    "target_agent_id": { "type": "string", "description": "Global agent profile id to message" },
                    "message": { "type": "string", "description": "The user-like message to deliver into a fresh recipient session; this tool queues async work, returns recipient ids/task refs, and does not wait for the reply" }
                },
                "required": ["target_agent_id", "message"],
                "additionalProperties": false,
            }),
            Self::GrantAgentChainContinuation => json!({
                "type": "object",
                "properties": {
                    "chain_id": { "type": "string", "description": "Blocked inter-agent chain id to extend once after it hit max_hops" },
                    "reason": { "type": "string", "description": "Why one more hop should be allowed" }
                },
                "required": ["chain_id", "reason"],
                "additionalProperties": false,
            }),
        }
    }
}
