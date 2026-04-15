export type BootstrapPayload = {
  agent_id: string;
  config_path: string;
  listen_addr: string;
  generated_at: string;
  transport: {
    endpoint_path: string;
    websocket_path: string;
  };
  assets: {
    mode: string;
  };
  settings: SettingsSnapshot;
  sessions: SessionSummary[];
};

export type SessionSummary = {
  session_id: string;
  created_at: string;
  last_activity: string;
  message_count: number;
};

export type QueuedDraft = {
  id: string;
  text: string;
  queued_at: string;
};

export type ChatMessage = {
  role: string;
  content: string;
};

export type ChatTimelineItem = {
  kind: "message" | "tool" | "plan";
  role?: string;
  content: string;
};

export type PlanTaskView = {
  id: string;
  plan_id: string;
  description: string;
  status: string;
  order: number;
  parent_task_id?: string;
  depends_on?: string[];
  blocked_reason?: string;
};

export type PlanView = {
  id: string;
  goal: string;
  status: string;
  created_at: string;
};

export type PlanHeadSnapshot = {
  plan: PlanView;
  tasks: Record<string, PlanTaskView>;
  ready: Record<string, boolean>;
  waiting_on_dependencies: Record<string, boolean>;
  blocked: Record<string, string>;
  notes: Record<string, string[]>;
};

export type PendingApprovalView = {
  approval_id: string;
  command_id: string;
  tool_name: string;
  message: string;
  command: string;
  args: string[];
  cwd: string;
};

export type ShellCommandView = {
  command_id: string;
  session_id: string;
  run_id: string;
  approval_id?: string;
  tool_name?: string;
  message?: string;
  command: string;
  args?: string[];
  cwd?: string;
  status: string;
  next_offset: number;
  last_chunk: string;
  exit_code?: number;
  error?: string;
  kill_pending?: boolean;
};

export type DelegateView = {
  delegate_id: string;
  session_id: string;
  status: string;
  task?: string;
  mode?: string;
};

export type SessionSnapshot = {
  session_id: string;
  created_at: string;
  last_activity: string;
  message_count: number;
  main_run_active: boolean;
  queued_drafts: QueuedDraft[];
  transcript: ChatMessage[];
  timeline: ChatTimelineItem[];
  plan: PlanHeadSnapshot;
  pending_approvals: PendingApprovalView[];
  running_commands: ShellCommandView[];
  delegates: DelegateView[];
};

export type SettingsFieldState = {
  key: string;
  label: string;
  type: string;
  value: unknown;
  file_path: string;
  revision: string;
};

export type SettingsRawFileState = {
  path: string;
  revision: string;
  size: number;
};

export type SettingsSnapshot = {
  revision: string;
  form_fields: SettingsFieldState[];
  raw_files: SettingsRawFileState[];
};

export type SettingsRawFileContent = {
  path: string;
  revision: string;
  content: string;
};

export type UIEvent = {
  kind: string;
  session_id: string;
  run_id: string;
  text: string;
  status: string;
  tool?: {
    phase: string;
    name: string;
    arguments: Record<string, unknown>;
    result_text?: string;
    error_text?: string;
  };
};

export type WebsocketEnvelope = {
  type: string;
  id?: string;
  command?: string;
  payload?: unknown;
  error?: string;
  event?: UIEvent;
  generated_at?: string;
};

export type ProviderResultPayload = {
  provider: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  content: string;
};

export type CommandPayloadMap = {
  "session.create": { session: SessionSnapshot };
  "session.get": { session: SessionSnapshot };
  "chat.send": { session: SessionSnapshot; queued: boolean; draft?: QueuedDraft; result?: ProviderResultPayload };
  "chat.btw": { session_id: string; prompt: string; result: ProviderResultPayload };
  "draft.enqueue": { session: SessionSnapshot; draft: QueuedDraft };
  "draft.list": { session: SessionSnapshot };
  "draft.recall": { session: SessionSnapshot; draft: QueuedDraft };
  "plan.create": { session: SessionSnapshot };
  "plan.add_task": { session: SessionSnapshot };
  "plan.edit_task": { session: SessionSnapshot };
  "plan.set_task_status": { session: SessionSnapshot };
  "plan.add_task_note": { session: SessionSnapshot };
  "shell.approve": { session: SessionSnapshot; command_id?: string };
  "shell.deny": { session: SessionSnapshot };
  "shell.kill": { session: SessionSnapshot };
  "settings.get": { settings: SettingsSnapshot };
  "settings.form.apply": { settings: SettingsSnapshot };
  "settings.raw.get": { file: SettingsRawFileContent };
  "settings.raw.apply": { settings: SettingsSnapshot };
};

export type CommandName = keyof CommandPayloadMap;
