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
import { WorkspaceCreatePane } from "./WorkspaceCreatePane";
import { WorkspaceEditor } from "./WorkspaceEditor";

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
  const [editorContent, setEditorContent] = useState("");
  const [newFilePath, setNewFilePath] = useState("");
  const [newFileContent, setNewFileContent] = useState("");
  const [newDirPath, setNewDirPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
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
      const file = await api.workspaceRead(sessionId, entry.path);
      setSelectedFile(file);
      setEditorContent(file.content ?? "");
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  async function saveSelectedFile() {
    if (!selectedFile || selectedFile.content_truncated) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceWrite(sessionId, selectedFile.path, editorContent, "overwrite");
      const refreshed = await api.workspaceRead(sessionId, selectedFile.path);
      setSelectedFile(refreshed);
      setEditorContent(refreshed.content ?? "");
      await load(path);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function createFile() {
    const targetPath = newFilePath.trim();
    if (!targetPath) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceWrite(sessionId, targetPath, newFileContent, "create");
      const file = await api.workspaceRead(sessionId, targetPath);
      setSelectedFile(file);
      setEditorContent(file.content ?? "");
      setNewFilePath("");
      setNewFileContent("");
      await load(parentPath(targetPath));
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setSaving(false);
    }
  }

  async function createDirectory() {
    const targetPath = newDirPath.trim();
    if (!targetPath) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceMkdir(sessionId, targetPath);
      setNewDirPath("");
      await load(parentPath(targetPath));
    } catch (mkdirError) {
      setError(mkdirError instanceof Error ? mkdirError.message : String(mkdirError));
    } finally {
      setSaving(false);
    }
  }

  async function trashEntry(entry: WorkspaceEntry) {
    if (!window.confirm(`Переместить в .trash: ${entry.path}?`)) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceTrash(sessionId, entry.path);
      if (selectedFile?.path === entry.path) {
        setSelectedFile(null);
        setEditorContent("");
      }
      await load(path);
    } catch (trashError) {
      setError(trashError instanceof Error ? trashError.message : String(trashError));
    } finally {
      setSaving(false);
    }
  }

  function download(pathToDownload: string) {
    window.open(api.workspaceDownloadUrl(sessionId, pathToDownload), "_blank", "noopener,noreferrer");
  }

  useEffect(() => {
    setSelectedFile(null);
    setEditorContent("");
    void load("");
  }, [sessionId]);

  const editorDirty = selectedFile ? (selectedFile.content ?? "") !== editorContent : false;

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

      <WorkspaceCreatePane
        filePath={newFilePath}
        fileContent={newFileContent}
        dirPath={newDirPath}
        saving={saving}
        onFilePathChange={setNewFilePath}
        onFileContentChange={setNewFileContent}
        onDirPathChange={setNewDirPath}
        onCreateFile={() => void createFile()}
        onCreateDirectory={() => void createDirectory()}
      />

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
                      <>
                        <Button size="small" variant="outlined" onClick={() => void load(entry.path)}>
                          Открыть
                        </Button>
                        <Button size="small" color="error" variant="outlined" onClick={() => void trashEntry(entry)}>
                          В .trash
                        </Button>
                      </>
                    ) : (
                      <>
                        <Button size="small" variant="outlined" onClick={() => void readFile(entry)}>
                          Открыть
                        </Button>
                        <Button size="small" variant="outlined" onClick={() => download(entry.path)}>
                          Скачать
                        </Button>
                        <Button size="small" color="error" variant="outlined" onClick={() => void trashEntry(entry)}>
                          В .trash
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
        <WorkspaceEditor
          file={selectedFile}
          content={editorContent}
          dirty={editorDirty}
          saving={saving}
          onContentChange={setEditorContent}
          onSave={() => void saveSelectedFile()}
          onDownload={() => download(selectedFile.path)}
        />
      ) : null}
    </Stack>
  );
}
