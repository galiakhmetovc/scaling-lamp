import {
  Button,
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
import { EmptyState, StatusChip } from "../../components/common";
import type { McpConnector } from "../../types";
import { formatTime } from "../../utils/format";

export function McpConnectorsTable({
  connectors,
  busyId,
  onToggle,
  onRestart
}: {
  connectors: McpConnector[];
  busyId: string | null;
  onToggle: (connector: McpConnector) => void;
  onRestart: (connector: McpConnector) => void;
}) {
  if (connectors.length === 0) {
    return <EmptyState title="MCP connectors не настроены" detail="Runtime не вернул ни одного коннектора из /v1/mcp/connectors." />;
  }

  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Connector</TableCell>
            <TableCell>Runtime</TableCell>
            <TableCell>Команда</TableCell>
            <TableCell>Env / cwd</TableCell>
            <TableCell>Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {connectors.map((connector) => (
            <TableRow key={connector.id} hover>
              <TableCell sx={{ minWidth: 180 }}>
                <Typography fontWeight={700} className="mono">
                  {connector.id}
                </Typography>
                <Stack direction="row" spacing={0.5} flexWrap="wrap" useFlexGap sx={{ mt: 0.5 }}>
                  <Chip label={connector.transport} size="small" variant="outlined" />
                  <Chip label={connector.enabled ? "enabled" : "disabled"} size="small" color={connector.enabled ? "success" : "default"} variant="outlined" />
                </Stack>
              </TableCell>
              <TableCell sx={{ minWidth: 180 }}>
                <StatusChip value={connector.runtime.state} />
                <Typography variant="caption" display="block" color="text.secondary" sx={{ mt: 0.5 }}>
                  pid={connector.runtime.pid ?? "none"} · restarts={connector.runtime.restart_count}
                </Typography>
                {connector.runtime.last_error ? (
                  <Typography variant="caption" display="block" color="error">
                    {connector.runtime.last_error}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell sx={{ maxWidth: 420 }}>
                <Typography className="mono" sx={{ wordBreak: "break-word" }}>
                  {connector.command} {connector.args.join(" ")}
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  updated: {formatTime(connector.updated_at)}
                </Typography>
              </TableCell>
              <TableCell sx={{ maxWidth: 300 }}>
                <Typography variant="caption" display="block" className="mono">
                  cwd={connector.cwd || "<none>"}
                </Typography>
                <Typography variant="caption" display="block" className="mono" sx={{ wordBreak: "break-word" }}>
                  env keys={Object.keys(connector.env).join(", ") || "<none>"}
                </Typography>
              </TableCell>
              <TableCell>
                <Stack direction="row" spacing={1}>
                  <Button size="small" variant="outlined" disabled={busyId === connector.id} onClick={() => onToggle(connector)}>
                    {connector.enabled ? "Disable" : "Enable"}
                  </Button>
                  <Button size="small" variant="outlined" disabled={busyId === connector.id || !connector.enabled} onClick={() => onRestart(connector)}>
                    Restart
                  </Button>
                </Stack>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
