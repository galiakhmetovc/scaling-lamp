use agent_runtime::mcp::McpConnectorConfig;
use agent_runtime::tool::{
    McpCallOutput, McpGetPromptOutput, McpPromptMessageOutput, McpReadResourceOutput,
    McpResourceContentOutput,
};
use rmcp::{
    ServiceExt,
    model::{
        CallToolRequestParams, Content, GetPromptRequestParams, PromptMessageContent,
        PromptMessageRole, ReadResourceRequestParams, ResourceContents, ToolAnnotations,
    },
    service::{RoleClient, RunningService},
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpConnectorState {
    Starting,
    Running,
    Stopped,
    Failed,
}

impl McpConnectorState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpConnectorRuntimeStatus {
    pub state: McpConnectorState,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub last_error: Option<String>,
    pub restart_count: u32,
}

impl Default for McpConnectorRuntimeStatus {
    fn default() -> Self {
        Self {
            state: McpConnectorState::Stopped,
            pid: None,
            started_at: None,
            stopped_at: None,
            last_error: None,
            restart_count: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpWorkerControl {
    command_tx: Sender<McpWorkerCommand>,
}

impl McpWorkerControl {
    fn new(command_tx: Sender<McpWorkerCommand>) -> Self {
        Self { command_tx }
    }

    pub fn noop() -> Self {
        let (command_tx, _command_rx) = mpsc::channel();
        Self { command_tx }
    }

    fn stop(&self) {
        let _ = self.command_tx.send(McpWorkerCommand::Stop);
    }

    fn call_tool(
        &self,
        connector_id: &str,
        exposed_name: &str,
        remote_name: &str,
        arguments_json: &str,
    ) -> Result<McpCallOutput, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(McpWorkerCommand::CallTool {
                connector_id: connector_id.to_string(),
                exposed_name: exposed_name.to_string(),
                remote_name: remote_name.to_string(),
                arguments_json: arguments_json.to_string(),
                response_tx,
            })
            .map_err(|error| error.to_string())?;
        response_rx.recv().map_err(|error| error.to_string())?
    }

    fn read_resource(
        &self,
        connector_id: &str,
        uri: &str,
    ) -> Result<McpReadResourceOutput, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(McpWorkerCommand::ReadResource {
                connector_id: connector_id.to_string(),
                uri: uri.to_string(),
                response_tx,
            })
            .map_err(|error| error.to_string())?;
        response_rx.recv().map_err(|error| error.to_string())?
    }

    fn get_prompt(
        &self,
        connector_id: &str,
        name: &str,
        arguments: Option<BTreeMap<String, String>>,
    ) -> Result<McpGetPromptOutput, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(McpWorkerCommand::GetPrompt {
                connector_id: connector_id.to_string(),
                name: name.to_string(),
                arguments,
                response_tx,
            })
            .map_err(|error| error.to_string())?;
        response_rx.recv().map_err(|error| error.to_string())?
    }
}

enum McpWorkerCommand {
    Stop,
    CallTool {
        connector_id: String,
        exposed_name: String,
        remote_name: String,
        arguments_json: String,
        response_tx: Sender<Result<McpCallOutput, String>>,
    },
    ReadResource {
        connector_id: String,
        uri: String,
        response_tx: Sender<Result<McpReadResourceOutput, String>>,
    },
    GetPrompt {
        connector_id: String,
        name: String,
        arguments: Option<BTreeMap<String, String>>,
        response_tx: Sender<Result<McpGetPromptOutput, String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredTool {
    pub exposed_name: String,
    pub remote_name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub read_only: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredResource {
    pub connector_id: String,
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredPrompt {
    pub connector_id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<McpDiscoveredPromptArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct McpConnectorDiscovery {
    tools: Vec<McpDiscoveredTool>,
    resources: Vec<McpDiscoveredResource>,
    prompts: Vec<McpDiscoveredPrompt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockMcpToolResult {
    pub content_text: String,
    pub structured_content: Option<serde_json::Value>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockMcpResourceRead {
    pub text: String,
    pub contents: Vec<McpResourceContentOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockMcpPromptGet {
    pub description: Option<String>,
    pub text: String,
    pub messages: Vec<McpPromptMessageOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockMcpConnectorRuntime {
    pub id: String,
    pub tools: Vec<McpDiscoveredTool>,
    pub resources: Vec<McpDiscoveredResource>,
    pub prompts: Vec<McpDiscoveredPrompt>,
    pub tool_results: BTreeMap<String, MockMcpToolResult>,
    pub resource_reads: BTreeMap<String, MockMcpResourceRead>,
    pub prompt_gets: BTreeMap<String, MockMcpPromptGet>,
}

type McpWorkerStarter = dyn Fn(&McpConnectorConfig, SharedMcpRegistry, i64) -> Result<McpWorkerControl, String>
    + Send
    + Sync;

#[derive(Debug, Default)]
struct McpRegistryState {
    statuses: BTreeMap<String, McpConnectorRuntimeStatus>,
    workers: BTreeMap<String, McpWorkerControl>,
    discoveries: BTreeMap<String, McpConnectorDiscovery>,
}

#[derive(Clone)]
pub struct SharedMcpRegistry {
    inner: Arc<Mutex<McpRegistryState>>,
    starter: Arc<McpWorkerStarter>,
}

impl std::fmt::Debug for SharedMcpRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedMcpRegistry")
            .finish_non_exhaustive()
    }
}

impl Default for SharedMcpRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(McpRegistryState::default())),
            starter: Arc::new(default_stdio_starter),
        }
    }
}

impl SharedMcpRegistry {
    pub fn with_starter<F>(starter: F) -> Self
    where
        F: Fn(&McpConnectorConfig, SharedMcpRegistry, i64) -> Result<McpWorkerControl, String>
            + Send
            + Sync
            + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(McpRegistryState::default())),
            starter: Arc::new(starter),
        }
    }

    pub fn with_mock_connectors(connectors: Vec<MockMcpConnectorRuntime>) -> Self {
        let registry = Self::default();
        let now = unix_timestamp();
        {
            let mut state = registry.lock();
            for connector in connectors {
                let connector_id = connector.id.clone();
                state.statuses.insert(
                    connector_id.clone(),
                    McpConnectorRuntimeStatus {
                        state: McpConnectorState::Running,
                        pid: None,
                        started_at: Some(now),
                        stopped_at: None,
                        last_error: None,
                        restart_count: 0,
                    },
                );
                state.discoveries.insert(
                    connector_id.clone(),
                    McpConnectorDiscovery {
                        tools: connector.tools.clone(),
                        resources: connector.resources.clone(),
                        prompts: connector.prompts.clone(),
                    },
                );
                state
                    .workers
                    .insert(connector_id.clone(), spawn_mock_worker(connector));
            }
        }
        registry
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, McpRegistryState> {
        self.inner.lock().expect("shared MCP registry poisoned")
    }

    pub fn status(&self, id: &str) -> McpConnectorRuntimeStatus {
        self.lock().statuses.get(id).cloned().unwrap_or_default()
    }

    pub fn ensure_placeholder(&self, id: &str) {
        self.lock().statuses.entry(id.to_string()).or_default();
    }

    pub fn remove(&self, id: &str) {
        let mut registry = self.lock();
        if let Some(worker) = registry.workers.remove(id) {
            worker.stop();
        }
        registry.statuses.remove(id);
        registry.discoveries.remove(id);
    }

    pub fn set_status(&self, id: &str, status: McpConnectorRuntimeStatus) {
        self.lock().statuses.insert(id.to_string(), status);
    }

    fn set_discovery(&self, id: &str, discovery: McpConnectorDiscovery) {
        self.lock().discoveries.insert(id.to_string(), discovery);
    }

    pub fn clear_discovery(&self, id: &str) {
        self.lock().discoveries.remove(id);
    }

    pub fn list_discovered_tools(&self) -> Vec<McpDiscoveredTool> {
        let registry = self.lock();
        let mut tools = registry
            .discoveries
            .values()
            .flat_map(|discovery| discovery.tools.iter().cloned())
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.exposed_name.cmp(&right.exposed_name));
        tools
    }

    pub fn list_discovered_resources(
        &self,
        connector_id: Option<&str>,
    ) -> Vec<McpDiscoveredResource> {
        let registry = self.lock();
        let mut resources = registry
            .discoveries
            .iter()
            .filter(|(id, _)| connector_id.is_none_or(|expected| expected == id.as_str()))
            .flat_map(|(_, discovery)| discovery.resources.iter().cloned())
            .collect::<Vec<_>>();
        resources.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.uri.cmp(&right.uri))
        });
        resources
    }

    pub fn list_discovered_prompts(&self, connector_id: Option<&str>) -> Vec<McpDiscoveredPrompt> {
        let registry = self.lock();
        let mut prompts = registry
            .discoveries
            .iter()
            .filter(|(id, _)| connector_id.is_none_or(|expected| expected == id.as_str()))
            .flat_map(|(_, discovery)| discovery.prompts.iter().cloned())
            .collect::<Vec<_>>();
        prompts.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.name.cmp(&right.name))
        });
        prompts
    }

    pub fn call_tool(
        &self,
        exposed_name: &str,
        arguments_json: &str,
    ) -> Result<McpCallOutput, String> {
        let (connector_id, remote_name, worker) = {
            let registry = self.lock();
            let (connector_id, tool) = registry
                .discoveries
                .iter()
                .flat_map(|(connector_id, discovery)| {
                    discovery
                        .tools
                        .iter()
                        .map(move |tool| (connector_id.as_str(), tool))
                })
                .find(|(_, tool)| tool.exposed_name == exposed_name)
                .ok_or_else(|| format!("unknown MCP tool {exposed_name}"))?;
            let worker = registry
                .workers
                .get(connector_id)
                .cloned()
                .ok_or_else(|| format!("MCP connector {connector_id} is not running"))?;
            (connector_id.to_string(), tool.remote_name.clone(), worker)
        };
        worker.call_tool(
            connector_id.as_str(),
            exposed_name,
            remote_name.as_str(),
            arguments_json,
        )
    }

    pub fn read_resource(
        &self,
        connector_id: &str,
        uri: &str,
    ) -> Result<McpReadResourceOutput, String> {
        let worker = self
            .lock()
            .workers
            .get(connector_id)
            .cloned()
            .ok_or_else(|| format!("MCP connector {connector_id} is not running"))?;
        worker.read_resource(connector_id, uri)
    }

    pub fn get_prompt(
        &self,
        connector_id: &str,
        name: &str,
        arguments: Option<BTreeMap<String, String>>,
    ) -> Result<McpGetPromptOutput, String> {
        let worker = self
            .lock()
            .workers
            .get(connector_id)
            .cloned()
            .ok_or_else(|| format!("MCP connector {connector_id} is not running"))?;
        worker.get_prompt(connector_id, name, arguments)
    }

    pub fn ensure_started(&self, connector: &McpConnectorConfig, now: i64) -> Result<(), String> {
        {
            let registry = self.lock();
            if registry.workers.contains_key(&connector.id)
                && matches!(
                    registry
                        .statuses
                        .get(&connector.id)
                        .map(|status| status.state),
                    Some(McpConnectorState::Starting | McpConnectorState::Running)
                )
            {
                return Ok(());
            }
        }

        self.mark_starting(&connector.id, now);
        match (self.starter)(connector, self.clone(), now) {
            Ok(worker) => {
                self.lock().workers.insert(connector.id.clone(), worker);
                Ok(())
            }
            Err(error) => {
                self.mark_failed(&connector.id, now, error.clone());
                Err(error)
            }
        }
    }

    pub fn ensure_stopped(&self, id: &str, now: i64) {
        let worker = self.lock().workers.remove(id);
        if let Some(worker) = worker {
            worker.stop();
        }
        self.mark_stopped(id, now);
    }

    pub fn mark_running(&self, id: &str, now: i64, pid: Option<u32>) {
        let mut registry = self.lock();
        let status = registry.statuses.entry(id.to_string()).or_default();
        status.state = McpConnectorState::Running;
        status.pid = pid;
        status.started_at = Some(now);
        status.stopped_at = None;
        status.last_error = None;
    }

    pub fn mark_failed(&self, id: &str, now: i64, error: String) {
        let mut registry = self.lock();
        registry.workers.remove(id);
        registry.discoveries.remove(id);
        let status = registry.statuses.entry(id.to_string()).or_default();
        status.state = McpConnectorState::Failed;
        status.pid = None;
        status.stopped_at = Some(now);
        status.last_error = Some(error);
    }

    pub fn mark_stopped(&self, id: &str, now: i64) {
        let mut registry = self.lock();
        registry.workers.remove(id);
        registry.discoveries.remove(id);
        let status = registry.statuses.entry(id.to_string()).or_default();
        status.state = McpConnectorState::Stopped;
        status.pid = None;
        status.stopped_at = Some(now);
    }

    fn mark_starting(&self, id: &str, now: i64) {
        let mut registry = self.lock();
        let status = registry.statuses.entry(id.to_string()).or_default();
        if status.started_at.is_some() || status.last_error.is_some() || status.restart_count > 0 {
            status.restart_count = status.restart_count.saturating_add(1);
        }
        status.state = McpConnectorState::Starting;
        status.started_at = Some(now);
        status.stopped_at = None;
        status.last_error = None;
        status.pid = None;
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn default_stdio_starter(
    connector: &McpConnectorConfig,
    registry: SharedMcpRegistry,
    now: i64,
) -> Result<McpWorkerControl, String> {
    let connector = connector.clone();
    let connector_id = connector.id.clone();
    let registry_for_thread = registry.clone();
    let (command_tx, command_rx) = mpsc::channel();
    let worker = McpWorkerControl::new(command_tx);

    thread::Builder::new()
        .name(format!("mcp-{}", connector_id))
        .spawn(move || {
            let registry = registry_for_thread;
            let runtime_connector_id = connector_id.clone();
            let registry_for_runtime = registry.clone();
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    let registry = registry_for_runtime.clone();
                    runtime.block_on(async move {
                        let transport = TokioChildProcess::new(
                            Command::new(&connector.command).configure(|command| {
                                command.args(&connector.args);
                                if let Some(cwd) = &connector.cwd {
                                    command.current_dir(cwd);
                                }
                                for (key, value) in &connector.env {
                                    command.env(key, value);
                                }
                            }),
                        )
                        .map_err(|error| error.to_string())?;
                        let pid = transport.id();
                        let service = ().serve(transport).await.map_err(|error| error.to_string())?;
                        let peer_info = service
                            .peer_info()
                            .cloned()
                            .ok_or_else(|| "MCP peer did not expose initialize info".to_string())?;
                        let discovery = discover_connector_capabilities(
                            runtime_connector_id.as_str(),
                            &service,
                            &peer_info.capabilities,
                        )
                        .await?;
                        registry.set_discovery(&runtime_connector_id, discovery);
                        registry.mark_running(&runtime_connector_id, now, pid);
                        run_stdio_worker_loop(runtime_connector_id.as_str(), &service, command_rx)
                            .await?;
                        let _ = service.cancel().await;
                        Ok::<(), String>(())
                    })
                });

            match result {
                Ok(()) => registry.mark_stopped(&connector_id, unix_timestamp()),
                Err(error) => registry.mark_failed(&connector_id, unix_timestamp(), error),
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(worker)
}

async fn discover_connector_capabilities(
    connector_id: &str,
    service: &RunningService<RoleClient, ()>,
    capabilities: &rmcp::model::ServerCapabilities,
) -> Result<McpConnectorDiscovery, String> {
    let tools = if capabilities.tools.is_some() {
        build_discovered_tools(
            connector_id,
            service
                .list_all_tools()
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        Vec::new()
    };
    let resources = if capabilities.resources.is_some() {
        service
            .list_all_resources()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|resource| McpDiscoveredResource {
                connector_id: connector_id.to_string(),
                uri: resource.uri.clone(),
                name: resource.name.clone(),
                title: resource.title.clone(),
                description: resource.description.clone(),
                mime_type: resource.mime_type.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let prompts = if capabilities.prompts.is_some() {
        service
            .list_all_prompts()
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|prompt| McpDiscoveredPrompt {
                connector_id: connector_id.to_string(),
                name: prompt.name.clone(),
                title: prompt.title.clone(),
                description: prompt.description.clone(),
                arguments: prompt
                    .arguments
                    .unwrap_or_default()
                    .into_iter()
                    .map(|argument| McpDiscoveredPromptArgument {
                        name: argument.name,
                        description: argument.description,
                        required: argument.required.unwrap_or(false),
                    })
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(McpConnectorDiscovery {
        tools,
        resources,
        prompts,
    })
}

async fn run_stdio_worker_loop(
    connector_id: &str,
    service: &RunningService<RoleClient, ()>,
    command_rx: mpsc::Receiver<McpWorkerCommand>,
) -> Result<(), String> {
    loop {
        match command_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(McpWorkerCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Ok(McpWorkerCommand::CallTool {
                connector_id: response_connector_id,
                exposed_name,
                remote_name,
                arguments_json,
                response_tx,
            }) => {
                let result = invoke_stdio_tool(
                    &response_connector_id,
                    &exposed_name,
                    &remote_name,
                    &arguments_json,
                    service,
                )
                .await;
                let _ = response_tx.send(result);
            }
            Ok(McpWorkerCommand::ReadResource {
                connector_id: response_connector_id,
                uri,
                response_tx,
            }) => {
                let result =
                    invoke_stdio_read_resource(&response_connector_id, &uri, service).await;
                let _ = response_tx.send(result);
            }
            Ok(McpWorkerCommand::GetPrompt {
                connector_id: response_connector_id,
                name,
                arguments,
                response_tx,
            }) => {
                let result =
                    invoke_stdio_get_prompt(&response_connector_id, &name, arguments, service)
                        .await;
                let _ = response_tx.send(result);
            }
        }
    }
    let _ = connector_id;
    Ok(())
}

async fn invoke_stdio_tool(
    connector_id: &str,
    exposed_name: &str,
    remote_name: &str,
    arguments_json: &str,
    service: &RunningService<RoleClient, ()>,
) -> Result<McpCallOutput, String> {
    let parsed = serde_json::from_str::<serde_json::Value>(arguments_json)
        .map_err(|error| error.to_string())?;
    let arguments = parsed
        .as_object()
        .cloned()
        .ok_or_else(|| "dynamic MCP tool arguments must be a JSON object".to_string())?;
    let result = service
        .call_tool(CallToolRequestParams {
            meta: None,
            name: remote_name.to_string().into(),
            arguments: Some(arguments),
            task: None,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(McpCallOutput {
        connector_id: connector_id.to_string(),
        exposed_name: exposed_name.to_string(),
        remote_name: remote_name.to_string(),
        content_text: flatten_content_text(&result.content),
        structured_content_json: result.structured_content.map(|value| value.to_string()),
        is_error: result.is_error.unwrap_or(false),
    })
}

async fn invoke_stdio_read_resource(
    connector_id: &str,
    uri: &str,
    service: &RunningService<RoleClient, ()>,
) -> Result<McpReadResourceOutput, String> {
    let result = service
        .read_resource(ReadResourceRequestParams {
            meta: None,
            uri: uri.to_string(),
        })
        .await
        .map_err(|error| error.to_string())?;
    let (contents, text) = flatten_resource_contents(result.contents);
    Ok(McpReadResourceOutput {
        connector_id: connector_id.to_string(),
        uri: uri.to_string(),
        text,
        contents,
    })
}

async fn invoke_stdio_get_prompt(
    connector_id: &str,
    name: &str,
    arguments: Option<BTreeMap<String, String>>,
    service: &RunningService<RoleClient, ()>,
) -> Result<McpGetPromptOutput, String> {
    let result = service
        .get_prompt(GetPromptRequestParams {
            meta: None,
            name: name.to_string(),
            arguments: arguments.map(|items| {
                items
                    .into_iter()
                    .map(|(key, value)| (key, serde_json::Value::String(value)))
                    .collect()
            }),
        })
        .await
        .map_err(|error| error.to_string())?;
    let (messages, text) = flatten_prompt_messages(result.messages);
    Ok(McpGetPromptOutput {
        connector_id: connector_id.to_string(),
        name: name.to_string(),
        description: result.description,
        text,
        messages,
    })
}

fn build_discovered_tools(
    connector_id: &str,
    tools: Vec<rmcp::model::Tool>,
) -> Vec<McpDiscoveredTool> {
    let mut used_names = BTreeMap::<String, usize>::new();
    let mut discovered = tools
        .into_iter()
        .map(|tool| {
            let exposed_name = disambiguate_exposed_name(
                format!(
                    "mcp__{}__{}",
                    sanitize_mcp_name_segment(connector_id),
                    sanitize_mcp_name_segment(tool.name.as_ref())
                ),
                &mut used_names,
            );
            let annotations = tool
                .annotations
                .clone()
                .unwrap_or_else(ToolAnnotations::new);
            McpDiscoveredTool {
                exposed_name,
                remote_name: tool.name.to_string(),
                title: tool.title.clone().or_else(|| annotations.title.clone()),
                description: tool.description.map(|description| description.into_owned()),
                input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
                read_only: annotations.read_only_hint.unwrap_or(false),
                destructive: annotations.is_destructive(),
            }
        })
        .collect::<Vec<_>>();
    discovered.sort_by(|left, right| left.exposed_name.cmp(&right.exposed_name));
    discovered
}

fn disambiguate_exposed_name(base: String, used_names: &mut BTreeMap<String, usize>) -> String {
    let counter = used_names.entry(base.clone()).or_insert(0);
    let resolved = if *counter == 0 {
        base
    } else {
        format!("{base}-{}", *counter + 1)
    };
    *counter += 1;
    resolved
}

fn sanitize_mcp_name_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '_' | '-' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '-',
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed.to_string()
    }
}

fn flatten_content_text(content: &[Content]) -> String {
    content
        .iter()
        .filter_map(|item| match &item.raw {
            rmcp::model::RawContent::Text(text) => Some(text.text.clone()),
            rmcp::model::RawContent::Resource(resource) => match &resource.resource {
                ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
                ResourceContents::BlobResourceContents { .. } => None,
            },
            rmcp::model::RawContent::ResourceLink(resource) => resource
                .description
                .clone()
                .or_else(|| resource.title.clone())
                .or_else(|| Some(resource.uri.clone())),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn flatten_resource_contents(
    contents: Vec<ResourceContents>,
) -> (Vec<McpResourceContentOutput>, String) {
    let mut text_parts = Vec::new();
    let flattened = contents
        .into_iter()
        .map(|content| match content {
            ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                ..
            } => {
                text_parts.push(text.clone());
                McpResourceContentOutput {
                    kind: "text".to_string(),
                    uri,
                    mime_type,
                    text: Some(text),
                    blob: None,
                }
            }
            ResourceContents::BlobResourceContents {
                uri,
                mime_type,
                blob,
                ..
            } => McpResourceContentOutput {
                kind: "blob".to_string(),
                uri,
                mime_type,
                text: None,
                blob: Some(blob),
            },
        })
        .collect::<Vec<_>>();
    (flattened, text_parts.join("\n"))
}

fn flatten_prompt_messages(
    messages: Vec<rmcp::model::PromptMessage>,
) -> (Vec<McpPromptMessageOutput>, String) {
    let mut text_parts = Vec::new();
    let flattened = messages
        .into_iter()
        .map(|message| {
            let role = match message.role {
                PromptMessageRole::User => "user",
                PromptMessageRole::Assistant => "assistant",
            }
            .to_string();
            match message.content {
                PromptMessageContent::Text { text } => {
                    text_parts.push(text.clone());
                    McpPromptMessageOutput {
                        role,
                        content_type: "text".to_string(),
                        text: Some(text),
                        uri: None,
                        mime_type: None,
                    }
                }
                PromptMessageContent::Resource { resource } => match &resource.resource {
                    ResourceContents::TextResourceContents {
                        uri,
                        mime_type,
                        text,
                        ..
                    } => {
                        text_parts.push(text.clone());
                        McpPromptMessageOutput {
                            role,
                            content_type: "resource".to_string(),
                            text: Some(text.clone()),
                            uri: Some(uri.clone()),
                            mime_type: mime_type.clone(),
                        }
                    }
                    ResourceContents::BlobResourceContents { uri, mime_type, .. } => {
                        McpPromptMessageOutput {
                            role,
                            content_type: "resource".to_string(),
                            text: None,
                            uri: Some(uri.clone()),
                            mime_type: mime_type.clone(),
                        }
                    }
                },
                PromptMessageContent::ResourceLink { link } => McpPromptMessageOutput {
                    role,
                    content_type: "resource_link".to_string(),
                    text: link.description.clone().or_else(|| link.title.clone()),
                    uri: Some(link.uri.clone()),
                    mime_type: link.mime_type.clone(),
                },
                PromptMessageContent::Image { image } => McpPromptMessageOutput {
                    role,
                    content_type: "image".to_string(),
                    text: None,
                    uri: None,
                    mime_type: Some(image.mime_type.clone()),
                },
            }
        })
        .collect::<Vec<_>>();
    (flattened, text_parts.join("\n"))
}

fn spawn_mock_worker(runtime: MockMcpConnectorRuntime) -> McpWorkerControl {
    let (command_tx, command_rx) = mpsc::channel();
    thread::Builder::new()
        .name(format!("mcp-mock-{}", runtime.id))
        .spawn(move || {
            loop {
                match command_rx.recv() {
                    Ok(McpWorkerCommand::Stop) | Err(_) => break,
                    Ok(McpWorkerCommand::CallTool {
                        connector_id,
                        exposed_name,
                        remote_name,
                        response_tx,
                        ..
                    }) => {
                        let missing_remote_name = remote_name.clone();
                        let result = runtime
                            .tool_results
                            .get(&remote_name)
                            .cloned()
                            .map(|result| McpCallOutput {
                                connector_id,
                                exposed_name,
                                remote_name,
                                content_text: result.content_text,
                                structured_content_json: result
                                    .structured_content
                                    .map(|value| value.to_string()),
                                is_error: result.is_error,
                            })
                            .ok_or_else(|| {
                                format!("missing mock MCP tool result for {missing_remote_name}")
                            });
                        let _ = response_tx.send(result);
                    }
                    Ok(McpWorkerCommand::ReadResource {
                        connector_id,
                        uri,
                        response_tx,
                    }) => {
                        let missing_uri = uri.clone();
                        let result = runtime
                            .resource_reads
                            .get(&uri)
                            .cloned()
                            .map(|result| McpReadResourceOutput {
                                connector_id,
                                uri,
                                text: result.text,
                                contents: result.contents,
                            })
                            .ok_or_else(|| {
                                format!("missing mock MCP resource read for {missing_uri}")
                            });
                        let _ = response_tx.send(result);
                    }
                    Ok(McpWorkerCommand::GetPrompt {
                        connector_id,
                        name,
                        response_tx,
                        ..
                    }) => {
                        let missing_name = name.clone();
                        let result = runtime
                            .prompt_gets
                            .get(&name)
                            .cloned()
                            .map(|result| McpGetPromptOutput {
                                connector_id,
                                name,
                                description: result.description,
                                text: result.text,
                                messages: result.messages,
                            })
                            .ok_or_else(|| format!("missing mock MCP prompt for {missing_name}"));
                        let _ = response_tx.send(result);
                    }
                }
            }
        })
        .expect("spawn mock MCP worker");
    McpWorkerControl::new(command_tx)
}

#[cfg(test)]
mod tests {
    use super::{McpConnectorRuntimeStatus, McpConnectorState, SharedMcpRegistry};
    use agent_runtime::mcp::{McpConnectorConfig, McpConnectorTransport};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn registry_starts_once_and_can_be_stopped() {
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = starts.clone();
        let registry = SharedMcpRegistry::with_starter(move |connector, registry, now| {
            starts_clone.fetch_add(1, Ordering::Relaxed);
            registry.mark_running(&connector.id, now, Some(42));
            Ok(super::McpWorkerControl::noop())
        });
        let connector = McpConnectorConfig {
            id: "filesystem".to_string(),
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
        };

        registry
            .ensure_started(&connector, 10)
            .expect("first start succeeds");
        registry
            .ensure_started(&connector, 11)
            .expect("second start is ignored");
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(
            registry.status("filesystem").state,
            McpConnectorState::Running
        );

        registry.ensure_stopped("filesystem", 12);
        let status = registry.status("filesystem");
        assert_eq!(status.state, McpConnectorState::Stopped);
        assert_eq!(status.stopped_at, Some(12));
    }

    #[test]
    fn registry_restarts_after_failed_state() {
        let starts = Arc::new(AtomicUsize::new(0));
        let starts_clone = starts.clone();
        let registry = SharedMcpRegistry::with_starter(move |connector, registry, now| {
            starts_clone.fetch_add(1, Ordering::Relaxed);
            registry.mark_running(&connector.id, now, None);
            Ok(super::McpWorkerControl::noop())
        });
        registry.set_status(
            "filesystem",
            McpConnectorRuntimeStatus {
                state: McpConnectorState::Failed,
                pid: None,
                started_at: Some(5),
                stopped_at: Some(6),
                last_error: Some("boom".to_string()),
                restart_count: 0,
            },
        );
        let connector = McpConnectorConfig {
            id: "filesystem".to_string(),
            transport: McpConnectorTransport::Stdio,
            command: "npx".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
        };

        registry
            .ensure_started(&connector, 10)
            .expect("restart succeeds");
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(registry.status("filesystem").restart_count, 1);
    }
}
