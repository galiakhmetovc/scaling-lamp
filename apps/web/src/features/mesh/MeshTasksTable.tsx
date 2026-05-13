import { Button, Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import type { AgentSummary, SessionSummary, SessionTask } from "../../types";
import { formatTime, parseJsonLabel, short } from "../../utils/format";
import { sessionTitle } from "../../ui/entityLabels";

function agentLabel(agentId: string | null | undefined, agents: AgentSummary[]): string {
  if (!agentId) {
    return "—";
  }
  return agents.find((agent) => agent.id === agentId)?.name ?? agentId;
}

export function MeshTasksTable({
  tasks,
  sessions = [],
  agents = [],
  onOpenSession
}: {
  tasks: SessionTask[];
  sessions?: SessionSummary[];
  agents?: AgentSummary[];
  onOpenSession: (sessionId: string) => void;
}) {
  if (tasks.length === 0) {
    return <EmptyState title="Task registry пуст" detail="В последних runtime данных нет agent_task/delegate задач." />;
  }

  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Task</TableCell>
            <TableCell>Статус</TableCell>
            <TableCell>Маршрут</TableCell>
            <TableCell>Контекст / результат</TableCell>
            <TableCell>Обновлена</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {tasks.map((task) => (
            <TableRow key={task.id} hover>
              <TableCell>
                <Typography fontWeight={700}>{task.kind}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(task.id, 34)}
                </Typography>
              </TableCell>
              <TableCell>
                <StatusChip value={task.status} />
                <Typography variant="caption" display="block" color="text.secondary" sx={{ mt: 0.5 }}>
                  attempts {task.attempt_count}/{task.max_attempts}
                </Typography>
              </TableCell>
              <TableCell>
                <Typography variant="body2">
                  {agentLabel(task.owner_agent_id, agents)} → {agentLabel(task.executor_agent_id, agents)}
                </Typography>
                {task.source_session_id ? (
                  <Button size="small" variant="text" onClick={() => onOpenSession(task.source_session_id!)}>
                    {sessionTitle(task.source_session_id, sessions).primary}
                  </Button>
                ) : null}
                {task.source_session_id ? (
                  <Typography variant="caption" color="text.secondary" className="mono" display="block">
                    {short(task.source_session_id, 28)}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell sx={{ maxWidth: 460 }}>
                <Typography variant="body2" noWrap>
                  {parseJsonLabel(task.context_ref_json)}
                </Typography>
                {task.result_ref_json ? (
                  <Typography variant="caption" color="text.secondary" display="block" noWrap>
                    result: {parseJsonLabel(task.result_ref_json)}
                  </Typography>
                ) : null}
                {task.error ? (
                  <Typography variant="caption" color="error" display="block" noWrap>
                    {task.error}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>{formatTime(task.updated_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
