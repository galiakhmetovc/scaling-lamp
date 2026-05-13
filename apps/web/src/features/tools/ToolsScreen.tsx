import {
  Alert,
  Button,
  Chip,
  Dialog,
  DialogContent,
  DialogTitle,
  Paper,
  Stack,
  Tab,
  Tabs,
  TextField
} from "@mui/material";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { JsonBlock, Metric, SectionHeader } from "../../components/common";
import type {
  AgentDetail,
  McpConnector,
  McpPrompt,
  McpPromptGet,
  McpPromptList,
  McpResource,
  McpResourceList,
  McpResourceRead,
  SessionSummary,
  ToolCallSummary,
  ToolCatalog
} from "../../types";
import { ToolsTable } from "./ToolsTable";
import { McpConnectorsTable } from "./McpConnectorsTable";
import { McpPromptsTable } from "./McpPromptsTable";
import { McpResourcesTable } from "./McpResourcesTable";
import { ToolCatalogTable } from "./ToolCatalogTable";
import { summarizeToolCatalog } from "./toolCatalog";

type ToolsTab = "catalog" | "calls" | "mcp" | "resources" | "prompts";

export function ToolsScreen({
  selectedSession,
  recentTools,
  filter,
  onFilterChange
}: {
  selectedSession: SessionSummary | null;
  recentTools: ToolCallSummary[];
  filter: string;
  onFilterChange: (value: string) => void;
}) {
  const [tab, setTab] = useState<ToolsTab>("catalog");
  const [catalog, setCatalog] = useState<ToolCatalog | null>(null);
  const [connectors, setConnectors] = useState<McpConnector[]>([]);
  const [resources, setResources] = useState<McpResourceList | null>(null);
  const [prompts, setPrompts] = useState<McpPromptList | null>(null);
  const [mcpDetail, setMcpDetail] = useState<McpResourceRead | McpPromptGet | null>(null);
  const [agent, setAgent] = useState<AgentDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [busyConnectorId, setBusyConnectorId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load(signal?: AbortSignal) {
    setLoading(true);
    setError(null);
    try {
      const [nextCatalog, nextConnectors, nextAgent] = await Promise.all([
        api.toolCatalog(signal),
        api.mcpConnectors(signal),
        selectedSession ? api.agentDetail(selectedSession.agent_profile_id, signal) : Promise.resolve(null)
      ]);
      const [nextResources, nextPrompts] = await Promise.all([
        api.mcpResources({ limit: 50 }, signal),
        api.mcpPrompts({ limit: 50 }, signal)
      ]);
      setCatalog(nextCatalog);
      setConnectors(nextConnectors);
      setResources(nextResources);
      setPrompts(nextPrompts);
      setAgent(nextAgent);
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setLoading(false);
      }
    }
  }

  async function toggleConnector(connector: McpConnector) {
    setBusyConnectorId(connector.id);
    setError(null);
    try {
      await api.updateMcpConnector(connector.id, { enabled: !connector.enabled });
      await load();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : String(updateError));
    } finally {
      setBusyConnectorId(null);
    }
  }

  async function restartConnector(connector: McpConnector) {
    setBusyConnectorId(connector.id);
    setError(null);
    try {
      await api.restartMcpConnector(connector.id);
      await load();
    } catch (restartError) {
      setError(restartError instanceof Error ? restartError.message : String(restartError));
    } finally {
      setBusyConnectorId(null);
    }
  }

  async function readResource(resource: McpResource) {
    setError(null);
    try {
      setMcpDetail(await api.mcpReadResource(resource.connector_id, resource.uri));
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  async function getPrompt(prompt: McpPrompt) {
    setError(null);
    try {
      setMcpDetail(await api.mcpGetPrompt(prompt.connector_id, prompt.name));
    } catch (promptError) {
      setError(promptError instanceof Error ? promptError.message : String(promptError));
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [selectedSession?.agent_profile_id]);

  const stats = summarizeToolCatalog(catalog?.tools ?? []);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Tools / MCP"
        subtitle="Каталог tools, реальные MCP connectors и последние вызовы из runtime. Allowed/blocked считается относительно агента выбранной сессии."
        action={
          <Button variant="outlined" disabled={loading} onClick={() => void load()}>
            Обновить
          </Button>
        }
      />
      {error ? <Alert severity="error">{error}</Alert> : null}
      <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
        <Metric label="Tools" value={stats.total} hint={`${stats.builtIn} built-in · ${stats.mcp} MCP`} />
        <Metric label="Risk" value={stats.destructive} hint="destructive tools" />
        <Metric label="Unavailable" value={stats.unavailable} hint="disabled by runtime config" />
        <Metric label="MCP connectors" value={connectors.length} hint={`${connectors.filter((connector) => connector.enabled).length} enabled`} />
        <Metric label="MCP resources" value={resources?.total_results ?? 0} hint={`${resources?.results.length ?? 0} on page`} />
        <Metric label="MCP prompts" value={prompts?.total_results ?? 0} hint={`${prompts?.results.length ?? 0} on page`} />
      </Stack>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5} alignItems={{ xs: "stretch", md: "center" }}>
          <TextField
            fullWidth
            size="small"
            label="Фильтр"
            value={filter}
            onChange={(event) => onFilterChange(event.target.value)}
            placeholder="tool, family, connector, status, schema"
          />
          {selectedSession ? (
            <Chip label={`agent: ${selectedSession.agent_profile_id}`} color="primary" variant="outlined" />
          ) : (
            <Chip label="agent not selected" variant="outlined" />
          )}
        </Stack>
      </Paper>
      <Paper variant="outlined">
        <Tabs value={tab} onChange={(_, value: ToolsTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
          <Tab value="catalog" label="Catalog" />
          <Tab value="calls" label="Recent calls" />
          <Tab value="mcp" label="MCP connectors" />
          <Tab value="resources" label="MCP resources" />
          <Tab value="prompts" label="MCP prompts" />
        </Tabs>
      </Paper>

      {tab === "catalog" ? <ToolCatalogTable tools={catalog?.tools ?? []} filter={filter} agent={agent} /> : null}
      {tab === "calls" ? <ToolsTable tools={recentTools} filter={filter} onFilterChange={onFilterChange} /> : null}
      {tab === "mcp" ? (
        <McpConnectorsTable
          connectors={connectors}
          busyId={busyConnectorId}
          onToggle={(connector) => void toggleConnector(connector)}
          onRestart={(connector) => void restartConnector(connector)}
        />
      ) : null}
      {tab === "resources" ? <McpResourcesTable resources={resources} onRead={(resource) => void readResource(resource)} /> : null}
      {tab === "prompts" ? <McpPromptsTable prompts={prompts} onGet={(prompt) => void getPrompt(prompt)} /> : null}

      <Dialog open={Boolean(mcpDetail)} onClose={() => setMcpDetail(null)} fullWidth maxWidth="lg">
        <DialogTitle>MCP detail</DialogTitle>
        <DialogContent>{mcpDetail ? <JsonBlock value={mcpDetail} /> : null}</DialogContent>
      </Dialog>
    </Stack>
  );
}
