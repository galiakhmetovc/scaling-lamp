import {
  Alert,
  Box,
  Button,
  Chip,
  Collapse,
  Divider,
  LinearProgress,
  Paper,
  Stack,
  TextField,
  Typography
} from "@mui/material";
import { useEffect, useState, type ReactNode } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { WorkspaceEntry, WorkspaceFile, WorkspaceList } from "../../types";
import { WorkspaceEditor } from "./WorkspaceEditor";
import { buildWorkspaceTreeNodes, getParentPath, joinWorkspacePath } from "./workspaceTree";

type DirectoryState = {
  entries: WorkspaceEntry[];
  total: number;
  loading: boolean;
  loaded: boolean;
  error: string | null;
};

type DirectoryStateMap = Record<string, DirectoryState>;

function emptyDirectoryState(): DirectoryState {
  return {
    entries: [],
    total: 0,
    loading: false,
    loaded: false,
    error: null
  };
}

function childNameForCreate(basePath: string, value: string): string {
  const trimmed = value.trim().replace(/^\/+/, "");
  if (!trimmed) {
    return "";
  }
  return trimmed.includes("/") ? trimmed : joinWorkspacePath(basePath, trimmed);
}

export function WorkspaceFilesPane({ sessionId, compact = false }: { sessionId: string; compact?: boolean }) {
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set([""]));
  const [directories, setDirectories] = useState<DirectoryStateMap>({});
  const [selectedPath, setSelectedPath] = useState("");
  const [selectedFile, setSelectedFile] = useState<WorkspaceFile | null>(null);
  const [editorContent, setEditorContent] = useState("");
  const [newFileName, setNewFileName] = useState("");
  const [newFileContent, setNewFileContent] = useState("");
  const [newDirName, setNewDirName] = useState("");
  const [uploadFile, setUploadFile] = useState<File | null>(null);
  const [uploadPath, setUploadPath] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function loadDirectory(path: string) {
    setDirectories((current) => ({
      ...current,
      [path]: {
        ...(current[path] ?? emptyDirectoryState()),
        loading: true,
        error: null
      }
    }));
    try {
      const result: WorkspaceList = await api.workspaceList(sessionId, {
        path,
        recursive: false,
        limit: 500,
        offset: 0
      });
      setDirectories((current) => ({
        ...current,
        [path]: {
          entries: result.entries,
          total: result.total,
          loading: false,
          loaded: true,
          error: null
        }
      }));
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError);
      setDirectories((current) => ({
        ...current,
        [path]: {
          ...(current[path] ?? emptyDirectoryState()),
          loading: false,
          error: message
        }
      }));
    }
  }

  async function toggleDirectory(path: string) {
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (path !== "" && next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
    if (!directories[path]?.loaded && !directories[path]?.loading) {
      await loadDirectory(path);
    }
  }

  async function readFile(entry: WorkspaceEntry) {
    setSelectedPath(entry.path);
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
      await loadDirectory(getParentPath(selectedFile.path));
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function createFile() {
    const targetPath = childNameForCreate(getParentPath(selectedPath), newFileName);
    if (!targetPath) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceWrite(sessionId, targetPath, newFileContent, "create");
      const file = await api.workspaceRead(sessionId, targetPath);
      setSelectedPath(targetPath);
      setSelectedFile(file);
      setEditorContent(file.content ?? "");
      setNewFileName("");
      setNewFileContent("");
      await loadDirectory(getParentPath(targetPath));
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setSaving(false);
    }
  }

  async function createDirectory() {
    const targetPath = childNameForCreate(getParentPath(selectedPath), newDirName);
    if (!targetPath) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceMkdir(sessionId, targetPath);
      setNewDirName("");
      setExpandedPaths((current) => new Set(current).add(targetPath));
      await loadDirectory(getParentPath(targetPath));
      await loadDirectory(targetPath);
    } catch (mkdirError) {
      setError(mkdirError instanceof Error ? mkdirError.message : String(mkdirError));
    } finally {
      setSaving(false);
    }
  }

  async function uploadSelectedFile() {
    if (!uploadFile) {
      return;
    }
    const targetPath = childNameForCreate(getParentPath(selectedPath), uploadPath || uploadFile.name);
    if (!targetPath) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceUpload(sessionId, targetPath, uploadFile, "create");
      setUploadFile(null);
      setUploadPath("");
      await loadDirectory(getParentPath(targetPath));
      try {
        const file = await api.workspaceRead(sessionId, targetPath);
        setSelectedPath(targetPath);
        setSelectedFile(file);
        setEditorContent(file.content ?? "");
      } catch {
        setSelectedPath(targetPath);
        setSelectedFile(null);
        setEditorContent("");
      }
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : String(uploadError));
    } finally {
      setSaving(false);
    }
  }

  async function trashSelectedFile() {
    if (!selectedFile || !window.confirm(`Переместить в .trash: ${selectedFile.path}?`)) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.workspaceTrash(sessionId, selectedFile.path);
      const parent = getParentPath(selectedFile.path);
      setSelectedFile(null);
      setSelectedPath(parent);
      setEditorContent("");
      await loadDirectory(parent);
    } catch (trashError) {
      setError(trashError instanceof Error ? trashError.message : String(trashError));
    } finally {
      setSaving(false);
    }
  }

  function download(pathToDownload: string) {
    window.open(api.workspaceDownloadUrl(sessionId, pathToDownload), "_blank", "noopener,noreferrer");
  }

  function renderDirectory(path: string, depth = 0): ReactNode {
    const state = directories[path] ?? emptyDirectoryState();
    const nodes = buildWorkspaceTreeNodes(state.entries);

    return (
      <Box key={path || "root"}>
        {state.loading ? <LinearProgress sx={{ my: 0.5 }} /> : null}
        {state.error ? (
          <Alert severity="error" sx={{ my: 0.75 }}>
            {state.error}
          </Alert>
        ) : null}
        {nodes.map((entry) => {
          const isDirectory = entry.kind === "directory";
          const isExpanded = expandedPaths.has(entry.path);
          const isSelected = selectedPath === entry.path || selectedFile?.path === entry.path;

          return (
            <Box key={entry.path}>
              <button
                type="button"
                className={`workspace-tree-row ${isSelected ? "is-selected" : ""}`}
                style={{ paddingLeft: 10 + depth * 18 }}
                onClick={() => {
                  if (isDirectory) {
                    setSelectedPath(entry.path);
                    setSelectedFile(null);
                    setEditorContent("");
                    void toggleDirectory(entry.path);
                  } else {
                    void readFile(entry);
                  }
                }}
              >
                <span className="workspace-tree-icon">{isDirectory ? (isExpanded ? "▾" : "▸") : "•"}</span>
                <span className="workspace-tree-name">{entry.label}</span>
                <span className="workspace-tree-meta">{isDirectory ? "dir" : `${entry.bytes ?? 0} b`}</span>
              </button>
              {isDirectory ? (
                <Collapse in={isExpanded} timeout="auto" unmountOnExit>
                  {renderDirectory(entry.path, depth + 1)}
                </Collapse>
              ) : null}
            </Box>
          );
        })}
        {state.loaded && nodes.length === 0 ? (
          <Typography variant="caption" color="text.secondary" sx={{ display: "block", pl: 1.25, py: 0.75 }}>
            Папка пуста
          </Typography>
        ) : null}
      </Box>
    );
  }

  useEffect(() => {
    setExpandedPaths(new Set([""]));
    setDirectories({});
    setSelectedPath("");
    setSelectedFile(null);
    setEditorContent("");
    void loadDirectory("");
  }, [sessionId]);

  const rootState = directories[""] ?? emptyDirectoryState();
  const editorDirty = selectedFile ? (selectedFile.content ?? "") !== editorContent : false;
  const currentBasePath = selectedFile ? getParentPath(selectedFile.path) : selectedPath;

  return (
    <Stack spacing={1.5}>
      {error ? <Alert severity="error">{error}</Alert> : null}

      <Box className={`workspace-browser ${compact ? "workspace-browser-compact" : ""}`}>
        <Paper variant="outlined" className="workspace-tree-panel">
          <Stack spacing={1.25}>
            <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between">
              <Typography fontWeight={700}>Workspace tree</Typography>
              <Button variant="outlined" onClick={() => void loadDirectory("")}>
                Обновить
              </Button>
            </Stack>
            <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
              <Chip label={`root items: ${rootState.total}`} variant="outlined" />
              <Chip label={rootState.loading ? "loading" : "ready"} color={rootState.loading ? "info" : "default"} variant="outlined" />
            </Stack>
            <Divider />
            {rootState.loaded && rootState.entries.length === 0 ? (
              <EmptyState title="Workspace пуст" detail="Агент пока не создал файлы в workspace." />
            ) : (
              <Box className="workspace-tree">{renderDirectory("")}</Box>
            )}
          </Stack>
        </Paper>

        <Stack spacing={1.5} minWidth={0}>
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Stack spacing={1.25}>
              <Stack direction={{ xs: "column", lg: "row" }} spacing={1} justifyContent="space-between">
                <Box minWidth={0}>
                  <Typography fontWeight={700}>Действия</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    base: {currentBasePath || "."}
                  </Typography>
                </Box>
                {selectedFile ? (
                  <Stack direction="row" spacing={1}>
                    <Button variant="outlined" onClick={() => download(selectedFile.path)}>
                      Скачать
                    </Button>
                    <Button color="error" variant="outlined" disabled={saving} onClick={() => void trashSelectedFile()}>
                      В .trash
                    </Button>
                  </Stack>
                ) : null}
              </Stack>
              <Stack direction={{ xs: "column", lg: "row" }} spacing={1}>
                <TextField
                  fullWidth
                  label="Новый файл"
                  value={newFileName}
                  onChange={(event) => setNewFileName(event.target.value)}
                  placeholder="notes/todo.md или todo.md"
                />
                <Button variant="contained" disabled={saving || !newFileName.trim()} onClick={() => void createFile()}>
                  Создать файл
                </Button>
              </Stack>
              <TextField
                fullWidth
                multiline
                minRows={3}
                label="Содержимое нового файла"
                value={newFileContent}
                onChange={(event) => setNewFileContent(event.target.value)}
                inputProps={{ className: "mono" }}
              />
              <Stack direction={{ xs: "column", lg: "row" }} spacing={1}>
                <TextField
                  fullWidth
                  label="Новая папка"
                  value={newDirName}
                  onChange={(event) => setNewDirName(event.target.value)}
                  placeholder="notes"
                />
                <Button variant="outlined" disabled={saving || !newDirName.trim()} onClick={() => void createDirectory()}>
                  Создать папку
                </Button>
              </Stack>
              <Divider />
              <Stack direction={{ xs: "column", lg: "row" }} spacing={1} alignItems={{ xs: "stretch", lg: "center" }}>
                <Button variant="outlined" component="label">
                  Выбрать файл
                  <input
                    hidden
                    type="file"
                    onChange={(event) => {
                      const file = event.target.files?.[0] ?? null;
                      setUploadFile(file);
                      if (file && !uploadPath.trim()) {
                        setUploadPath(file.name);
                      }
                    }}
                  />
                </Button>
                <TextField
                  fullWidth
                  label="Путь загрузки"
                  value={uploadPath}
                  onChange={(event) => setUploadPath(event.target.value)}
                  placeholder={uploadFile?.name ?? "uploads/file.pdf"}
                />
                <Button variant="contained" disabled={saving || !uploadFile || !uploadPath.trim()} onClick={() => void uploadSelectedFile()}>
                  Загрузить
                </Button>
              </Stack>
              {uploadFile ? (
                <Typography variant="caption" color="text.secondary">
                  {uploadFile.name} · {uploadFile.size.toLocaleString("ru-RU")} bytes · режим create, существующий файл не перезаписывается
                </Typography>
              ) : null}
            </Stack>
          </Paper>

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
          ) : (
            <Paper variant="outlined" sx={{ p: 2 }}>
              <EmptyState title="Файл не выбран" detail="Выбери файл в дереве слева, чтобы посмотреть или отредактировать его." />
            </Paper>
          )}
        </Stack>
      </Box>
    </Stack>
  );
}
