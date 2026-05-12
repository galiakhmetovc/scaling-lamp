import {
  Alert,
  Button,
  Checkbox,
  FormControlLabel,
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
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { WorkspaceEntry, WorkspaceFile, WorkspaceList } from "../../types";
import { FilePreview } from "./FilePreview";

function parentPath(path: string): string {
  const normalized = path.replace(/\/+$/, "");
  const index = normalized.lastIndexOf("/");
  return index > 0 ? normalized.slice(0, index) : "";
}

export function WorkspaceFilesPane({ sessionId }: { sessionId: string }) {
  const [path, setPath] = useState("");
  const [recursive, setRecursive] = useState(false);
  const [list, setList] = useState<WorkspaceList | null>(null);
  const [selectedFile, setSelectedFile] = useState<WorkspaceFile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load(nextPath = path, nextOffset = 0) {
    setLoading(true);
    setError(null);
    try {
      const result = await api.workspaceList(sessionId, {
        path: nextPath,
        recursive,
        limit: 100,
        offset: nextOffset
      });
      setList(result);
      setPath(result.path);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function readFile(entry: WorkspaceEntry) {
    setError(null);
    try {
      setSelectedFile(await api.workspaceRead(sessionId, entry.path));
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  function download(pathToDownload: string) {
    window.open(api.workspaceDownloadUrl(sessionId, pathToDownload), "_blank", "noopener,noreferrer");
  }

  useEffect(() => {
    void load("");
  }, [sessionId]);

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems="center">
          <TextField
            fullWidth
            size="small"
            label="Путь в workspace"
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="например: docs или scratch/browser"
          />
          <FormControlLabel
            control={<Checkbox checked={recursive} onChange={(event) => setRecursive(event.target.checked)} />}
            label="recursive"
          />
          <Button variant="outlined" onClick={() => void load(parentPath(path))}>
            Вверх
          </Button>
          <Button variant="contained" onClick={() => void load(path)}>
            Открыть
          </Button>
        </Stack>
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}

      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Путь</TableCell>
              <TableCell>Тип</TableCell>
              <TableCell>Размер</TableCell>
              <TableCell align="right">Действия</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {(list?.entries ?? []).map((entry) => (
              <TableRow key={entry.path} hover>
                <TableCell className="mono">{entry.path}</TableCell>
                <TableCell>{entry.kind}</TableCell>
                <TableCell>{entry.bytes ?? "—"}</TableCell>
                <TableCell align="right">
                  <Stack direction="row" spacing={1} justifyContent="flex-end">
                    {entry.kind === "directory" ? (
                      <Button size="small" variant="outlined" onClick={() => void load(entry.path)}>
                        Открыть
                      </Button>
                    ) : (
                      <>
                        <Button size="small" variant="outlined" onClick={() => void readFile(entry)}>
                          Читать
                        </Button>
                        <Button size="small" variant="outlined" onClick={() => download(entry.path)}>
                          Скачать
                        </Button>
                      </>
                    )}
                  </Stack>
                </TableCell>
              </TableRow>
            ))}
            {!loading && (list?.entries ?? []).length === 0 ? (
              <TableRow>
                <TableCell colSpan={4}>
                  <EmptyState title="Workspace пуст" detail="В этом пути нет файлов или директорий." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>

      {list ? (
        <Typography variant="caption" color="text.secondary" className="mono">
          root={list.workspace_root} · total={list.total} · offset={list.offset} · next={list.next_offset ?? "none"}
        </Typography>
      ) : null}

      {selectedFile ? (
        <FilePreview
          title={selectedFile.path}
          subtitle={selectedFile.workspace_root}
          bytes={selectedFile.byte_len}
          content={selectedFile.content}
          text={selectedFile.text}
          truncated={selectedFile.content_truncated}
          onDownload={() => download(selectedFile.path)}
        />
      ) : null}
    </Stack>
  );
}
