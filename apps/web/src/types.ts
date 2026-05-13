export type RuntimeStatus = {
  ok: boolean;
  version?: string | null;
  commit?: string | null;
  tree_state?: string | null;
  build_id?: string | null;
  bind_host?: string;
  bind_port?: number;
  permission_mode: string;
  session_count: number;
  mission_count: number;
  run_count: number;
  job_count: number;
  components?: number;
  data_dir: string;
  database?: string | null;
  telegram_mode?: string;
  event_bus_required?: boolean;
  event_bus_backend?: string;
  event_bus_nats_configured?: boolean;
};

export type WebEventBus = {
  backend: string;
  required: boolean;
  nats_configured: boolean;
  input_stream: string;
  session_stream: string;
  delivery_stream: string;
  task_stream: string;
  dlq_stream: string;
};

export type AgentSummary = {
  id: string;
  name: string;
  template_kind: string;
  default_workspace_root?: string | null;
  updated_at: number;
};

export type AgentDetail = {
  id: string;
  name: string;
  template_kind: string;
  agent_home: string;
  allowed_tools: string[];
  default_workspace_root?: string | null;
  created_from_template_id?: string | null;
  created_by_session_id?: string | null;
  created_by_agent_profile_id?: string | null;
  created_at: number;
  updated_at: number;
};

export type AgentUpdatePatch = {
  name?: string;
  allowed_tools?: string[];
  default_workspace_root?: string | null;
};

export type AgentFileEntry = {
  path: string;
  kind: string;
  byte_len: number;
};

export type AgentFiles = {
  agent_id: string;
  agent_name: string;
  agent_home: string;
  files: AgentFileEntry[];
};

export type AgentFile = {
  agent_id: string;
  agent_home: string;
  path: string;
  kind: string;
  byte_len: number;
  content: string;
};

export type AgentFileWriteResult = {
  agent_id: string;
  agent_home: string;
  path: string;
  kind: string;
  bytes_written: number;
  created: boolean;
  overwritten: boolean;
};

export type SessionSummary = {
  id: string;
  title: string;
  agent_profile_id: string;
  agent_name: string;
  model?: string | null;
  reasoning_visible: boolean;
  think_level?: string | null;
  compactifications: number;
  auto_approve: boolean;
  context_tokens: number;
  usage_input_tokens?: number | null;
  usage_output_tokens?: number | null;
  usage_total_tokens?: number | null;
  has_pending_approval: boolean;
  last_message_preview?: string | null;
  message_count: number;
  background_job_count: number;
  running_background_job_count: number;
  queued_background_job_count: number;
  created_at: number;
  updated_at: number;
};

export type RunSummary = {
  id: string;
  session_id: string;
  status: string;
  error?: string | null;
  started_at: number;
  updated_at: number;
  finished_at?: number | null;
};

export type ToolCallSummary = {
  id: string;
  session_id: string;
  run_id: string;
  tool_name: string;
  status: string;
  summary: string;
  error?: string | null;
  result_summary?: string | null;
  result_artifact_id?: string | null;
  requested_at: number;
  updated_at: number;
};

export type ToolCatalogItem = {
  id: string;
  family: string;
  origin: string;
  connector_id?: string | null;
  remote_name?: string | null;
  title?: string | null;
  description: string;
  read_only: boolean;
  destructive: boolean;
  requires_approval: boolean;
  automatic: boolean;
  available: boolean;
  availability_note?: string | null;
  input_schema: unknown;
};

export type ToolCatalog = {
  generated_at: number;
  tools: ToolCatalogItem[];
};

export type McpConnectorRuntime = {
  state: string;
  pid?: number | null;
  started_at?: number | null;
  stopped_at?: number | null;
  last_error?: string | null;
  restart_count: number;
};

export type McpConnector = {
  id: string;
  transport: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd?: string | null;
  enabled: boolean;
  created_at: number;
  updated_at: number;
  runtime: McpConnectorRuntime;
};

export type DeliveryTarget = {
  target_id: string;
  kind: string;
  scope: string;
  format_policy: string;
  updated_at: number;
};

export type TelegramChat = {
  telegram_chat_id: number;
  scope: string;
  selected_session_id?: string | null;
  default_agent_profile_id?: string | null;
  inbound_queue_mode: string;
  inbound_coalesce_window_ms?: number | null;
  updated_at: number;
};

export type TraceLink = {
  trace_id: string;
  span_id: string;
  entity_kind: string;
  entity_id: string;
  surface?: string | null;
  entrypoint?: string | null;
  created_at: number;
};

export type WebSnapshot = {
  generated_at: number;
  status: RuntimeStatus;
  event_bus: WebEventBus;
  agents: AgentSummary[];
  sessions: SessionSummary[];
  recent_runs: RunSummary[];
  recent_tasks: SessionTask[];
  recent_tool_calls: ToolCallSummary[];
  delivery_targets: DeliveryTarget[];
  telegram_chats: TelegramChat[];
  recent_traces: TraceLink[];
};

export type TranscriptLine = {
  role: string;
  content: string;
  run_id?: string | null;
  created_at: number;
  tool_name?: string | null;
  tool_status?: string | null;
  approval_id?: string | null;
};

export type PendingChatMessage = {
  id: string;
  session_id: string;
  role: "user";
  content: string;
  created_at: number;
  status: "sending" | "failed";
  error?: string | null;
};

export type SessionTranscript = {
  session_id: string;
  entries: TranscriptLine[];
};

export type DebugEntry = {
  id: string;
  kind: string;
  label: string;
  detail_title: string;
  detail: string;
  created_at: number;
  run_id?: string | null;
  artifact_id?: string | null;
};

export type SessionDebug = {
  session_id: string;
  entries: DebugEntry[];
};

export type SessionTask = {
  id: string;
  kind: string;
  status: string;
  source_session_id?: string | null;
  owner_agent_id?: string | null;
  executor_agent_id?: string | null;
  parent_task_id?: string | null;
  context_ref_json: string;
  result_ref_json?: string | null;
  attempt_count: number;
  max_attempts: number;
  timeout_at?: number | null;
  chain_id?: string | null;
  trace_id?: string | null;
  created_at: number;
  updated_at: number;
  started_at?: number | null;
  finished_at?: number | null;
  error?: string | null;
};

export type PendingApproval = {
  run_id: string;
  approval_id: string;
  reason: string;
  requested_at: number;
};

export type SessionSkillStatus = {
  name: string;
  description: string;
  mode: string;
};

export type SessionPreferencesPatch = {
  title?: string;
  model?: string | null;
  reasoning_visible?: boolean;
  think_level?: string | null;
  compactifications?: number;
  completion_nudges?: number | null;
  auto_approve?: boolean;
};

export type WorkspaceEntry = {
  path: string;
  kind: "file" | "directory" | string;
  bytes?: number | null;
};

export type WorkspaceList = {
  workspace_root: string;
  path: string;
  entries: WorkspaceEntry[];
  total: number;
  limit: number;
  offset: number;
  next_offset?: number | null;
};

export type WorkspaceFile = {
  workspace_root: string;
  path: string;
  byte_len: number;
  content?: string | null;
  content_truncated: boolean;
  text: boolean;
};

export type WorkspaceWriteResult = {
  workspace_root: string;
  path: string;
  bytes_written: number;
  created: boolean;
  overwritten: boolean;
};

export type WorkspaceMkdirResult = {
  workspace_root: string;
  path: string;
};

export type WorkspaceTrashResult = {
  workspace_root: string;
  path: string;
  trash_path: string;
};

export type ArtifactFileSummary = {
  id: string;
  session_id: string;
  kind: string;
  metadata_json: string;
  path: string;
  byte_len: number;
  created_at: number;
};

export type ArtifactFile = ArtifactFileSummary & {
  content?: string | null;
  content_truncated: boolean;
  text: boolean;
};

export type WorkerOutcome =
  | { kind: "chat_completed"; report: { session_id: string; run_id: string; output_text?: string | null } }
  | { kind: "approval_completed"; report: { run_id: string; output_text?: string | null; approval_id?: string | null } }
  | { kind: "approval_required"; approval_id: string; reason: string }
  | { kind: "interrupted_by_queued_input" }
  | { kind: "failed"; reason: string }
  | { kind: string; [key: string]: unknown };
