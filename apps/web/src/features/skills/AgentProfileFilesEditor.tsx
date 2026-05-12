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
  TextField,
  Typography
} from "@mui/material";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { AgentFile, AgentFileEntry, AgentFiles } from "../../types";

function skillPath(name: string): string {
  return `skills/${name.trim()}/SKILL.md`;
}

function skillSkeleton(name: string): string {
  const normalizedName = name.trim();
  return `---\nname: ${normalizedName}\ndescription: Коротко опиши, когда агент должен использовать этот skill.\n---\n\n# ${normalizedName}\n\n## Когда использовать\n\nИспользуй этот skill, когда ...\n\n## Порядок работы\n\n1. Определи входные данные.\n2. Выполни действия через канонические tools.\n3. Кратко сообщи результат оператору.\n`;
}

function isValidSkillName(value: string): boolean {
  return /^[a-zA-Z0-9._-]+$/.test(value.trim());
}

export function AgentProfileFilesEditor({ agentId }: { agentId: string }) {
  const [fileList, setFileList] = useState<AgentFiles | null>(null);
  const [selectedFile, setSelectedFile] = useState<AgentFile | null>(null);
  const [content, setContent] = useState("");
  const [newSkillName, setNewSkillName] = useState("");
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

  async function createSkill() {
    const name = newSkillName.trim();
    if (!isValidSkillName(name)) {
      setError("Skill name должен содержать только латиницу, цифры, '.', '_' или '-'.");
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const path = skillPath(name);
      await api.agentFileWrite(agentId, path, skillSkeleton(name), "create");
      setNewSkillName("");
      await load();
      const created = await api.agentFileRead(agentId, path);
      setSelectedFile(created);
      setContent(created.content);
      setNotice(`Создан skill: ${path}`);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    setFileList(null);
    setSelectedFile(null);
    setContent("");
    setNewSkillName("");
    setError(null);
    setNotice(null);
    void load();
  }, [agentId]);

  const dirty = selectedFile ? selectedFile.content !== content : false;

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1.5}>
          <Stack minWidth={0}>
            <Typography fontWeight={700}>Файлы профиля агента</Typography>
            <Typography variant="caption" color="text.secondary" className="mono" sx={{ wordBreak: "break-all" }}>
              agent={agentId} · home={fileList?.agent_home ?? "loading"}
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`files: ${fileList?.files.length ?? 0}`} variant="outlined" />
            {loading ? <Chip label="loading" color="info" variant="outlined" /> : null}
            <Button variant="outlined" disabled={loading} onClick={() => void load()}>
              Обновить
            </Button>
          </Stack>
        </Stack>
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}
      {notice ? <Alert severity="success">{notice}</Alert> : null}

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems="stretch">
          <TextField
            fullWidth
            size="small"
            label="Новый skill"
            value={newSkillName}
            onChange={(event) => setNewSkillName(event.target.value)}
            placeholder="telegram-operator-workflow"
            error={Boolean(newSkillName.trim()) && !isValidSkillName(newSkillName)}
            helperText="Будет создан файл skills/<name>/SKILL.md"
          />
          <Button variant="contained" disabled={saving || !newSkillName.trim()} onClick={() => void createSkill()}>
            Создать skill
          </Button>
        </Stack>
      </Paper>

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
            {(fileList?.files ?? []).map((file) => (
              <TableRow key={file.path} hover selected={selectedFile?.path === file.path}>
                <TableCell className="mono">{file.path}</TableCell>
                <TableCell>{file.kind}</TableCell>
                <TableCell>{file.byte_len}</TableCell>
                <TableCell align="right">
                  <Button size="small" variant="outlined" onClick={() => void readFile(file)}>
                    Открыть
                  </Button>
                </TableCell>
              </TableRow>
            ))}
            {!loading && (fileList?.files ?? []).length === 0 ? (
              <TableRow>
                <TableCell colSpan={4}>
                  <EmptyState title="Файлы профиля не найдены" detail="Ожидаются SYSTEM.md, AGENTS.md и skills/*/SKILL.md." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>

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
              minRows={18}
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
