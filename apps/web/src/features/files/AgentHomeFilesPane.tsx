import {
  Alert,
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
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { AgentFile, AgentFileEntry, AgentFiles } from "../../types";
import { FilePreview } from "./FilePreview";

export function AgentHomeFilesPane({ agentId }: { agentId: string }) {
  const [listing, setListing] = useState<AgentFiles | null>(null);
  const [selectedFile, setSelectedFile] = useState<AgentFile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      setListing(await api.agentFiles(agentId));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function readFile(entry: AgentFileEntry) {
    setError(null);
    try {
      setSelectedFile(await api.agentFileRead(agentId, entry.path));
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  useEffect(() => {
    setListing(null);
    setSelectedFile(null);
    void load();
  }, [agentId]);

  const files = listing?.files ?? [];

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" justifyContent="space-between" alignItems="center" spacing={2}>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={listing ? `${listing.agent_name} (${listing.agent_id})` : agentId} color="primary" variant="outlined" />
            <Chip label={`files: ${files.length}`} variant="outlined" />
            {loading ? <Chip label="loading" color="info" variant="outlined" /> : null}
          </Stack>
          <Button variant="outlined" onClick={() => void load()} disabled={loading}>
            Обновить
          </Button>
        </Stack>
        {listing ? (
          <Typography variant="caption" color="text.secondary" className="mono">
            {listing.agent_home}
          </Typography>
        ) : null}
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}

      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Путь</TableCell>
              <TableCell>Kind</TableCell>
              <TableCell>Размер</TableCell>
              <TableCell align="right">Действия</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {files.map((entry) => (
              <TableRow key={entry.path} hover selected={selectedFile?.path === entry.path}>
                <TableCell className="mono">{entry.path}</TableCell>
                <TableCell>{entry.kind}</TableCell>
                <TableCell>{entry.byte_len}</TableCell>
                <TableCell align="right">
                  <Button size="small" variant="outlined" disabled={entry.kind !== "file"} onClick={() => void readFile(entry)}>
                    Читать
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {!loading && files.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4}>
                  <EmptyState title="Agent home пуст" detail="Для этого агента не найдено доступных файлов." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>

      {selectedFile ? (
        <FilePreview
          title={selectedFile.path}
          subtitle={selectedFile.agent_home}
          bytes={selectedFile.byte_len}
          content={selectedFile.content}
          text
          truncated={false}
          onDownload={() => undefined}
        />
      ) : null}
    </Stack>
  );
}
