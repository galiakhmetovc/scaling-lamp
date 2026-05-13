import {
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Typography
} from "@mui/material";
import { useEffect, useState } from "react";
import { EmptyState, StatusChip } from "../../components/common";
import type { McpConnector } from "../../types";
import { formatTime } from "../../utils/format";

export function McpConnectorsTable({
  connectors,
  busyId,
  onSave,
  onToggle,
  onRestart
}: {
  connectors: McpConnector[];
  busyId: string | null;
  onSave: (connector: McpConnector, patch: Partial<Pick<McpConnector, "command" | "args" | "env" | "cwd" | "enabled">>) => void;
  onToggle: (connector: McpConnector) => void;
  onRestart: (connector: McpConnector) => void;
}) {
  const [editing, setEditing] = useState<McpConnector | null>(null);
  const [command, setCommand] = useState("");
  const [argsJson, setArgsJson] = useState("[]");
  const [envJson, setEnvJson] = useState("{}");
  const [cwd, setCwd] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!editing) {
      return;
    }
    setCommand(editing.command);
    setArgsJson(JSON.stringify(editing.args, null, 2));
    setEnvJson(JSON.stringify(editing.env, null, 2));
    setCwd(editing.cwd ?? "");
    setError(null);
  }, [editing]);

  function saveEditing() {
    if (!editing) {
      return;
    }
    try {
      const parsedArgs = JSON.parse(argsJson || "[]") as unknown;
      const parsedEnv = JSON.parse(envJson || "{}") as unknown;
      if (!Array.isArray(parsedArgs) || !parsedArgs.every((item) => typeof item === "string")) {
        throw new Error("args должен быть JSON-массивом строк");
      }
      if (!parsedEnv || typeof parsedEnv !== "object" || Array.isArray(parsedEnv) || !Object.values(parsedEnv).every((item) => typeof item === "string")) {
        throw new Error("env должен быть JSON-объектом строк");
      }
      onSave(editing, {
        command,
        args: parsedArgs,
        env: parsedEnv as Record<string, string>,
        cwd: cwd.trim() || null
      });
      setEditing(null);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    }
  }

  if (connectors.length === 0) {
    return <EmptyState title="MCP connectors не настроены" detail="Runtime не вернул ни одного коннектора из /v1/mcp/connectors." />;
  }

  return (
    <>
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
                  <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                    <Button size="small" variant="outlined" disabled={busyId === connector.id} onClick={() => setEditing(connector)}>
                      Configure
                    </Button>
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
      <Dialog open={Boolean(editing)} onClose={() => setEditing(null)} fullWidth maxWidth="md">
        <DialogTitle>Configure MCP connector: {editing?.id}</DialogTitle>
        <DialogContent>
          <Stack spacing={1.5} sx={{ pt: 1 }}>
            {error ? <Typography color="error">{error}</Typography> : null}
            <TextField label="Command" value={command} onChange={(event) => setCommand(event.target.value)} fullWidth />
            <TextField label="Args JSON" value={argsJson} onChange={(event) => setArgsJson(event.target.value)} fullWidth multiline minRows={4} inputProps={{ className: "mono" }} />
            <TextField label="Env JSON" value={envJson} onChange={(event) => setEnvJson(event.target.value)} fullWidth multiline minRows={4} inputProps={{ className: "mono" }} />
            <TextField label="Cwd" value={cwd} onChange={(event) => setCwd(event.target.value)} fullWidth inputProps={{ className: "mono" }} />
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setEditing(null)}>Отмена</Button>
          <Button variant="contained" onClick={saveEditing}>
            Сохранить
          </Button>
        </DialogActions>
      </Dialog>
    </>
  );
}
