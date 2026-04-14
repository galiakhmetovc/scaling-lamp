package contracts

type ResolvedContracts struct {
	ProviderRequest     ProviderRequestContract
	Memory              MemoryContract
	PromptAssets        PromptAssetsContract
	PromptAssembly      PromptAssemblyContract
	Tools               ToolContract
	FilesystemTools     FilesystemToolContract
	FilesystemExecution FilesystemExecutionContract
	ShellTools          ShellToolContract
	ShellExecution      ShellExecutionContract
	PlanTools           PlanToolContract
	ToolExecution       ToolExecutionContract
	Chat                ChatContract
	ProviderTrace       ProviderTraceContract
}

type ProviderRequestContract struct {
	Transport    TransportContract
	RequestShape RequestShapeContract
}

type TransportContract struct {
	ID       string
	Endpoint EndpointPolicy
	Auth     AuthPolicy
	Retry    RetryPolicy
	Timeout  TimeoutPolicy
}

type EndpointPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   EndpointParams
}

type EndpointParams struct {
	BaseURL      string            `yaml:"base_url"`
	Path         string            `yaml:"path"`
	Method       string            `yaml:"method"`
	ExtraHeaders map[string]string `yaml:"extra_headers"`
}

type AuthPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   AuthParams
}

type AuthParams struct {
	Header      string `yaml:"header"`
	Prefix      string `yaml:"prefix"`
	ValueEnvVar string `yaml:"value_env_var"`
}

type RetryPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   RetryParams
}

type RetryParams struct {
	MaxAttempts        int      `yaml:"max_attempts"`
	BaseDelay          string   `yaml:"base_delay"`
	MaxDelay           string   `yaml:"max_delay"`
	RetryOnStatuses    []int    `yaml:"retry_on_statuses"`
	RetryOnErrors      []string `yaml:"retry_on_errors"`
	EarlyFailureWindow string   `yaml:"early_failure_window"`
}

type TimeoutPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   TimeoutParams
}

type TimeoutParams struct {
	Total           string `yaml:"total"`
	Connect         string `yaml:"connect"`
	Idle            string `yaml:"idle"`
	OperationBudget string `yaml:"operation_budget"`
	AttemptTimeout  string `yaml:"attempt_timeout"`
}

type RequestShapeContract struct {
	ID             string
	Model          ModelPolicy
	Messages       MessagePolicy
	Tools          ToolPolicy
	ResponseFormat ResponseFormatPolicy
	Streaming      StreamingPolicy
	Sampling       SamplingPolicy
}

type PromptAssetsContract struct {
	ID          string
	PromptAsset PromptAssetPolicy
}

type PromptAssemblyContract struct {
	ID           string
	SystemPrompt SystemPromptPolicy
	SessionHead  SessionHeadPolicy
}

type ToolContract struct {
	ID            string
	Catalog       ToolCatalogPolicy
	Serialization ToolSerializationPolicy
}

type FilesystemToolContract struct {
	ID          string
	Catalog     FilesystemCatalogPolicy
	Description FilesystemDescriptionPolicy
}

type FilesystemExecutionContract struct {
	ID       string
	Scope    FilesystemScopePolicy
	Mutation FilesystemMutationPolicy
	IO       FilesystemIOPolicy
}

type ShellToolContract struct {
	ID          string
	Catalog     ShellCatalogPolicy
	Description ShellDescriptionPolicy
}

type ShellExecutionContract struct {
	ID       string
	Command  ShellCommandPolicy
	Approval ShellApprovalPolicy
	Runtime  ShellRuntimePolicy
}

type PlanToolContract struct {
	ID       string
	PlanTool PlanToolPolicy
}

type ToolExecutionContract struct {
	ID       string
	Access   ToolAccessPolicy
	Approval ToolApprovalPolicy
	Sandbox  ToolSandboxPolicy
}

type ProviderTraceContract struct {
	ID      string
	Request ProviderTracePolicy
}

type ChatContract struct {
	ID      string
	Input   ChatInputPolicy
	Submit  ChatSubmitPolicy
	Output  ChatOutputPolicy
	Status  ChatStatusPolicy
	Command ChatCommandPolicy
	Resume  ChatResumePolicy
}

type ChatInputPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatInputParams
}

type ChatSubmitPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatSubmitParams
}

type ChatOutputPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatOutputParams
}

type ChatStatusPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatStatusParams
}

type ChatCommandPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatCommandParams
}

type ChatResumePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatResumeParams
}

type ChatInputParams struct {
	PrimaryPrompt      string `yaml:"primary_prompt"`
	ContinuationPrompt string `yaml:"continuation_prompt"`
}

type ChatSubmitParams struct {
	EmptyLineThreshold int `yaml:"empty_line_threshold"`
}

type ChatOutputParams struct {
	ShowFinalNewline bool   `yaml:"show_final_newline"`
	RenderMarkdown   bool   `yaml:"render_markdown"`
	MarkdownStyle    string `yaml:"markdown_style"`
}

type ChatStatusParams struct {
	ShowHeader             bool `yaml:"show_header"`
	ShowUsage              bool `yaml:"show_usage"`
	ShowToolCalls          bool `yaml:"show_tool_calls"`
	ShowToolResults        bool `yaml:"show_tool_results"`
	ShowPlanAfterPlanTools bool `yaml:"show_plan_after_plan_tools"`
}

type ChatCommandParams struct {
	ExitCommand    string `yaml:"exit_command"`
	HelpCommand    string `yaml:"help_command"`
	SessionCommand string `yaml:"session_command"`
}

type ChatResumeParams struct {
	RequireExplicitID bool `yaml:"require_explicit_id"`
}

type PromptAssetPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   PromptAssetParams
}

type SystemPromptPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   SystemPromptParams
}

type SessionHeadPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   SessionHeadParams
}

type ToolCatalogPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ToolCatalogParams
}

type ToolSerializationPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ToolSerializationParams
}

type PlanToolPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   PlanToolParams
}

type FilesystemCatalogPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   FilesystemCatalogParams
}

type FilesystemDescriptionPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   FilesystemDescriptionParams
}

type FilesystemScopePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   FilesystemScopeParams
}

type FilesystemMutationPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   FilesystemMutationParams
}

type FilesystemIOPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   FilesystemIOParams
}

type ShellCatalogPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ShellCatalogParams
}

type ShellDescriptionPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ShellDescriptionParams
}

type ShellCommandPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ShellCommandParams
}

type ShellApprovalPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ShellApprovalParams
}

type ShellRuntimePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ShellRuntimeParams
}

type ToolAccessPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ToolAccessParams
}

type ToolApprovalPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ToolApprovalParams
}

type ToolSandboxPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ToolSandboxParams
}

type ProviderTracePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ProviderTraceParams
}

type SystemPromptParams struct {
	Path                   string `yaml:"path"`
	Role                   string `yaml:"role"`
	Required               bool   `yaml:"required"`
	TrimTrailingWhitespace bool   `yaml:"trim_trailing_whitespace"`
}

type SessionHeadParams struct {
	Placement                   string `yaml:"placement"`
	Title                       string `yaml:"title"`
	MaxItems                    int    `yaml:"max_items"`
	IncludeSessionID            bool   `yaml:"include_session_id"`
	IncludeOpenLoops            bool   `yaml:"include_open_loops"`
	IncludeLastUserMessage      bool   `yaml:"include_last_user_message"`
	IncludeLastAssistantMessage bool   `yaml:"include_last_assistant_message"`
}

type ToolCatalogParams struct {
	ToolIDs    []string `yaml:"tool_ids"`
	AllowEmpty bool     `yaml:"allow_empty"`
	Dedupe     bool     `yaml:"dedupe"`
}

type ToolSerializationParams struct {
	StrictJSONSchema    bool `yaml:"strict_json_schema"`
	IncludeDescriptions bool `yaml:"include_descriptions"`
}

type PlanToolParams struct {
	ToolIDs []string `yaml:"tool_ids"`
}

type FilesystemCatalogParams struct {
	ToolIDs    []string `yaml:"tool_ids"`
	AllowEmpty bool     `yaml:"allow_empty"`
	Dedupe     bool     `yaml:"dedupe"`
}

type FilesystemDescriptionParams struct {
	IncludeExamples  bool `yaml:"include_examples"`
	IncludeScopeHint bool `yaml:"include_scope_hint"`
}

type FilesystemScopeParams struct {
	RootPath      string   `yaml:"root_path"`
	ReadSubpaths  []string `yaml:"read_subpaths"`
	WriteSubpaths []string `yaml:"write_subpaths"`
	AllowedPaths  []string `yaml:"allowed_paths"`
	ReadOnlyPaths []string `yaml:"read_only_paths"`
	WritePaths    []string `yaml:"write_paths"`
}

type FilesystemMutationParams struct {
	AllowWrite              bool   `yaml:"allow_write"`
	AllowMove               bool   `yaml:"allow_move"`
	AllowMkdir              bool   `yaml:"allow_mkdir"`
	ApprovalMessageTemplate string `yaml:"approval_message_template"`
	TrashDir                string `yaml:"trash_dir"`
}

type FilesystemIOParams struct {
	MaxReadBytes  int    `yaml:"max_read_bytes"`
	MaxWriteBytes int    `yaml:"max_write_bytes"`
	Encoding      string `yaml:"encoding"`
}

type ShellCatalogParams struct {
	ToolIDs    []string `yaml:"tool_ids"`
	AllowEmpty bool     `yaml:"allow_empty"`
}

type ShellDescriptionParams struct {
	IncludeExamples      bool `yaml:"include_examples"`
	IncludeRuntimeLimits bool `yaml:"include_runtime_limits"`
}

type ShellCommandParams struct {
	AllowedCommands []string `yaml:"allowed_commands"`
	AllowedPrefixes []string `yaml:"allowed_prefixes"`
	DenyPatterns    []string `yaml:"deny_patterns"`
}

type ShellApprovalParams struct {
	Patterns                []string `yaml:"patterns"`
	ApprovalMessageTemplate string   `yaml:"approval_message_template"`
}

type ShellRuntimeParams struct {
	Cwd            string `yaml:"cwd"`
	Timeout        string `yaml:"timeout"`
	MaxOutputBytes int    `yaml:"max_output_bytes"`
	AllowNetwork   bool   `yaml:"allow_network"`
}

type ToolAccessParams struct {
	ToolIDs []string `yaml:"tool_ids"`
}

type ToolApprovalParams struct {
	DestructiveToolIDs      []string `yaml:"destructive_tool_ids"`
	ApprovalMessageTemplate string   `yaml:"approval_message_template"`
}

type ToolSandboxParams struct {
	AllowNetwork    bool     `yaml:"allow_network"`
	AllowWritePaths []string `yaml:"allow_write_paths"`
	DenyWritePaths  []string `yaml:"deny_write_paths"`
	Timeout         string   `yaml:"timeout"`
	MaxOutputBytes  int      `yaml:"max_output_bytes"`
}

type ProviderTraceParams struct {
	IncludeRawBody        bool `yaml:"include_raw_body"`
	IncludeDecodedPayload bool `yaml:"include_decoded_payload"`
}

type PromptAssetParams struct {
	Assets []PromptAsset `yaml:"assets"`
}

type PromptAsset struct {
	ID        string `yaml:"id" json:"id"`
	Role      string `yaml:"role" json:"role"`
	Content   string `yaml:"content" json:"content"`
	Placement string `yaml:"placement" json:"placement"`
}

type ModelPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ModelParams
}

type ModelParams struct {
	Model string `yaml:"model"`
}

type MessagePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
}

type ToolPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
}

type ResponseFormatPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ResponseFormatParams
}

type ResponseFormatParams struct {
	Type string `yaml:"type"`
}

type StreamingPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   StreamingParams
}

type StreamingParams struct {
	Stream bool `yaml:"stream"`
}

type SamplingPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   SamplingParams
}

type SamplingParams struct {
	Temperature     *float64 `yaml:"temperature"`
	TopP            *float64 `yaml:"top_p"`
	MaxOutputTokens *int     `yaml:"max_output_tokens"`
}

type Message struct {
	Role       string            `json:"role"`
	Content    string            `json:"content"`
	Name       string            `json:"name,omitempty"`
	ToolCallID string            `json:"tool_call_id,omitempty"`
	ToolCalls  []MessageToolCall `json:"tool_calls,omitempty"`
}

type MessageToolCall struct {
	ID       string              `json:"id"`
	Type     string              `json:"type,omitempty"`
	Function MessageToolFunction `json:"function"`
}

type MessageToolFunction struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}

type MemoryContract struct {
	ID      string
	Offload OffloadPolicy
}

type OffloadPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   OffloadParams
}

type OffloadParams struct {
	MaxChars int `yaml:"max_chars"`
}
