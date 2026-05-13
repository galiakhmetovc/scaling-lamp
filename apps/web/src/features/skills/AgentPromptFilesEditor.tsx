import { Alert, Button, Chip, Paper, Stack, TextField, Typography } from "@mui/material";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { AgentFile, AgentFileEntry, AgentFiles } from "../../types";
import { getPromptProfileFiles } from "./skillProfileFiles";

export function AgentPromptFilesEditor({ agentId }: { agentId: string }) {
  const [fileList, setFileList] = useState<AgentFiles | null>(null);
  const [selectedFile, setSelectedFile] = useState<AgentFile | null>(null);
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      setFileList(await api.agentFiles(agentId));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function readFile(file: AgentFileEntry) {
    setError(null);
    setNotice(null);
    try {
      const nextFile = await api.agentFileRead(agentId, file.path);
      setSelectedFile(nextFile);
      setContent(nextFile.content);
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  async function saveFile() {
    if (!selectedFile) {
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      await api.agentFileWrite(agentId, selectedFile.path, content, "overwrite");
      const refreshed = await api.agentFileRead(agentId, selectedFile.path);
      setSelectedFile(refreshed);
      setContent(refreshed.content);
      await load();
      setNotice(`Сохранено: ${refreshed.path}`);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    setFileList(null);
    setSelectedFile(null);
    setContent("");
    setError(null);
    setNotice(null);
    void load();
  }, [agentId]);

  const promptFiles = getPromptProfileFiles(fileList?.files ?? []);
  const dirty = selectedFile ? selectedFile.content !== content : false;

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1.5}>
          <Stack minWidth={0}>
            <Typography fontWeight={700}>SYSTEM.md / AGENTS.md</Typography>
            <Typography variant="caption" color="text.secondary" className="mono" sx={{ wordBreak: "break-all" }}>
              agent={agentId} · home={fileList?.agent_home ?? "loading"}
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`prompt files: ${promptFiles.length}`} variant="outlined" />
            {loading ? <Chip label="loading" color="info" variant="outlined" /> : null}
            <Button variant="outlined" disabled={loading} onClick={() => void load()}>
              Обновить
            </Button>
          </Stack>
        </Stack>
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}
      {notice ? <Alert severity="success">{notice}</Alert> : null}

      {promptFiles.length === 0 && !loading ? (
        <EmptyState title="Prompt-файлы не найдены" detail="Ожидаются отдельные файлы SYSTEM.md и AGENTS.md в профиле агента." />
      ) : (
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
          {promptFiles.map((file) => (
            <button
              key={file.path}
              type="button"
              className={`skill-card prompt-file-card ${selectedFile?.path === file.path ? "is-selected" : ""}`}
              onClick={() => void readFile(file)}
            >
              <Typography fontWeight={700} className="mono">
                {file.path}
              </Typography>
              <Typography variant="body2" color="text.secondary">
                {file.path === "SYSTEM.md"
                  ? "Базовые правила поведения агента. Должны быть стабильными и общими."
                  : "Рабочие инструкции агента, стек, предпочтения и локальные правила."}
              </Typography>
              <Chip label={`${file.byte_len} bytes`} variant="outlined" />
            </button>
          ))}
        </Stack>
      )}

      {selectedFile ? (
        <Paper variant="outlined" sx={{ p: 1.5 }}>
          <Stack spacing={1.25}>
            <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1}>
              <Stack minWidth={0}>
                <Typography fontWeight={700} className="mono" sx={{ wordBreak: "break-word" }}>
                  {selectedFile.path}
                </Typography>
                <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                  <Chip label={selectedFile.kind} variant="outlined" />
                  <Chip label={`${selectedFile.byte_len} bytes`} variant="outlined" />
                  {dirty ? <Chip label="unsaved" color="warning" /> : null}
                </Stack>
              </Stack>
              <Button variant="contained" disabled={saving || !dirty} onClick={() => void saveFile()}>
                {saving ? "Сохранение..." : "Сохранить"}
              </Button>
            </Stack>
            <TextField
              fullWidth
              multiline
              minRows={22}
              value={content}
              onChange={(event) => setContent(event.target.value)}
              inputProps={{ className: "mono" }}
            />
          </Stack>
        </Paper>
      ) : null}
    </Stack>
  );
}
