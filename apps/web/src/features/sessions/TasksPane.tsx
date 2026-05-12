import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import type { SessionTask } from "../../types";
import { formatTime, parseJsonLabel, short } from "../../utils/format";

export function TasksPane({ tasks }: { tasks: SessionTask[] }) {
  if (tasks.length === 0) {
    return <EmptyState title="Задач нет" detail="Task registry для выбранной сессии пуст." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Task</TableCell>
            <TableCell>Статус</TableCell>
            <TableCell>Исполнитель</TableCell>
            <TableCell>Контекст</TableCell>
            <TableCell>Обновлена</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {tasks.map((task) => (
            <TableRow key={task.id} hover>
              <TableCell>
                <Typography fontWeight={700}>{task.kind}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(task.id, 32)}
                </Typography>
              </TableCell>
              <TableCell>
                <StatusChip value={task.status} />
                <Typography variant="caption" color="text.secondary" display="block" sx={{ mt: 0.5 }}>
                  {task.attempt_count}/{task.max_attempts} попыток
                </Typography>
              </TableCell>
              <TableCell>{task.executor_agent_id || task.owner_agent_id || "—"}</TableCell>
              <TableCell sx={{ maxWidth: 420 }}>
                <Typography variant="body2" noWrap>
                  {parseJsonLabel(task.context_ref_json)}
                </Typography>
                {task.error ? (
                  <Typography variant="caption" color="error">
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
