import {
  Chip,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography
} from "@mui/material";
import { EmptyState, JsonBlock } from "../../components/common";
import type { AgentDetail, ToolCatalogItem } from "../../types";
import { agentAllowsTool, groupToolCatalogByFamily } from "./toolCatalog";

function policyChips(tool: ToolCatalogItem) {
  return (
    <Stack direction="row" spacing={0.5} flexWrap="wrap" useFlexGap>
      <Chip label={tool.read_only ? "read-only" : "write"} size="small" variant="outlined" />
      {tool.destructive ? <Chip label="destructive" size="small" color="warning" variant="outlined" /> : null}
      {tool.requires_approval ? <Chip label="approval" size="small" color="warning" variant="outlined" /> : null}
      {tool.automatic ? <Chip label="auto" size="small" variant="outlined" /> : null}
      {!tool.available ? <Chip label="runtime off" size="small" color="error" variant="outlined" /> : null}
    </Stack>
  );
}

export function ToolCatalogTable({
  tools,
  filter,
  agent
}: {
  tools: ToolCatalogItem[];
  filter: string;
  agent: AgentDetail | null;
}) {
  const normalizedFilter = filter.trim().toLowerCase();
  const filtered = tools.filter((tool) => {
    if (!normalizedFilter) {
      return true;
    }
    return [
      tool.id,
      tool.family,
      tool.origin,
      tool.connector_id ?? "",
      tool.remote_name ?? "",
      tool.title ?? "",
      tool.description
    ]
      .join(" ")
      .toLowerCase()
      .includes(normalizedFilter);
  });
  const groups = groupToolCatalogByFamily(filtered);

  if (filtered.length === 0) {
    return <EmptyState title="Tools не найдены" detail="Нет tools под текущий фильтр." />;
  }

  return (
    <Stack spacing={1.5}>
      {groups.map((group) => (
        <TableContainer key={group.family} component={Paper} variant="outlined">
          <Stack direction="row" spacing={1} alignItems="center" sx={{ px: 1.5, py: 1 }}>
            <Typography fontWeight={700} className="mono">
              {group.family}
            </Typography>
            <Chip label={`${group.tools.length}`} size="small" variant="outlined" />
          </Stack>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Tool</TableCell>
                <TableCell>Статус</TableCell>
                <TableCell>Описание</TableCell>
                <TableCell>Schema</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {group.tools.map((tool) => {
                const allowed = agent ? agentAllowsTool(agent.allowed_tools, tool.id) : null;
                return (
                  <TableRow key={tool.id} hover>
                    <TableCell sx={{ minWidth: 260 }}>
                      <Typography fontWeight={700} className="mono" sx={{ wordBreak: "break-word" }}>
                        {tool.id}
                      </Typography>
                      <Stack direction="row" spacing={0.5} flexWrap="wrap" useFlexGap sx={{ mt: 0.5 }}>
                        <Chip label={tool.origin} size="small" variant="outlined" />
                        {tool.connector_id ? <Chip label={`connector: ${tool.connector_id}`} size="small" variant="outlined" /> : null}
                        {tool.remote_name ? <Chip label={`remote: ${tool.remote_name}`} size="small" variant="outlined" /> : null}
                      </Stack>
                    </TableCell>
                    <TableCell sx={{ minWidth: 180 }}>
                      <Stack spacing={0.75}>
                        {allowed !== null ? (
                          <Chip
                            label={allowed ? "allowed for agent" : "blocked for agent"}
                            size="small"
                            color={allowed ? "success" : "default"}
                            variant="outlined"
                          />
                        ) : null}
                        {policyChips(tool)}
                        {tool.availability_note ? (
                          <Typography variant="caption" color="error">
                            {tool.availability_note}
                          </Typography>
                        ) : null}
                      </Stack>
                    </TableCell>
                    <TableCell sx={{ maxWidth: 420 }}>
                      {tool.title ? <Typography fontWeight={700}>{tool.title}</Typography> : null}
                      <Typography variant="body2">{tool.description}</Typography>
                    </TableCell>
                    <TableCell sx={{ minWidth: 280, maxWidth: 420 }}>
                      <JsonBlock value={tool.input_schema} />
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </TableContainer>
      ))}
    </Stack>
  );
}
