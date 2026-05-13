import { Button, Chip, Paper, Stack, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { AgentSummary } from "../../types";
import { formatTime } from "../../utils/format";

export function AgentsListPane({
  agents,
  selectedAgentId,
  onSelectAgent,
  onCreate
}: {
  agents: AgentSummary[];
  selectedAgentId: string | null;
  onSelectAgent: (agentId: string) => void;
  onCreate: () => void;
}) {
  return (
    <Stack spacing={1.5}>
      <Stack direction="row" justifyContent="space-between" alignItems="center">
        <Typography variant="body2" color="text.secondary">
          Профили агентов из canonical runtime. Создание и правки идут через `agentd`.
        </Typography>
        <Button variant="contained" onClick={onCreate}>
          Создать
        </Button>
      </Stack>
      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>ID</TableCell>
              <TableCell>Имя</TableCell>
              <TableCell>Шаблон</TableCell>
              <TableCell>Workspace</TableCell>
              <TableCell>Обновлён</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {agents.map((agent) => (
              <TableRow
                key={agent.id}
                hover
                selected={agent.id === selectedAgentId}
                sx={{ cursor: "pointer" }}
                onClick={() => onSelectAgent(agent.id)}
              >
                <TableCell className="mono">{agent.id}</TableCell>
                <TableCell>{agent.name}</TableCell>
                <TableCell>
                  <Chip label={agent.template_kind} variant="outlined" size="small" />
                </TableCell>
                <TableCell className="mono">{agent.default_workspace_root || "—"}</TableCell>
                <TableCell>{formatTime(agent.updated_at)}</TableCell>
              </TableRow>
            ))}
            {agents.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5}>
                  <EmptyState title="Агентов нет" detail="Snapshot не вернул agent profiles." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>
    </Stack>
  );
}
