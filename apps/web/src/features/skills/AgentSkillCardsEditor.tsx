import { Alert, Button, Chip, Paper, Stack, TextField, Typography } from "@mui/material";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { AgentFile, AgentFileEntry, AgentFiles, SessionSkillStatus } from "../../types";
import {
  getSkillProfileFiles,
  isValidSkillName,
  skillNameFromPath,
  skillPath,
  skillSkeleton
} from "./skillProfileFiles";

function modeColor(mode: string): "success" | "warning" | "default" {
  if (mode === "automatic" || mode === "manual") {
    return "success";
  }
  if (mode === "disabled") {
    return "warning";
  }
  return "default";
}

export function AgentSkillCardsEditor({
  agentId,
  skills,
  loading,
  onSetEnabled
}: {
  agentId: string;
  skills: SessionSkillStatus[];
  loading: boolean;
  onSetEnabled?: (name: string, enabled: boolean) => void;
}) {
  const [fileList, setFileList] = useState<AgentFiles | null>(null);
  const [selectedFile, setSelectedFile] = useState<AgentFile | null>(null);
  const [content, setContent] = useState("");
  const [newSkillName, setNewSkillName] = useState("");
  const [profileLoading, setProfileLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const skillsByName = useMemo(() => {
    return new Map(skills.map((skill) => [skill.name, skill]));
  }, [skills]);

  async function loadProfileFiles() {
    setProfileLoading(true);
    setError(null);
    try {
      setFileList(await api.agentFiles(agentId));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setProfileLoading(false);
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
      await loadProfileFiles();
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
      await loadProfileFiles();
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
    void loadProfileFiles();
  }, [agentId]);

  const skillFiles = getSkillProfileFiles(fileList?.files ?? []);
  const dirty = selectedFile ? selectedFile.content !== content : false;

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", lg: "row" }} spacing={1.5} justifyContent="space-between">
          <Stack minWidth={0}>
            <Typography fontWeight={700}>Skill folders</Typography>
            <Typography variant="caption" color="text.secondary">
              Показываются все файлы из `skills/&lt;name&gt;/...`. `SKILL.md` задаёт активацию, соседние файлы — примеры, справочники и assets.
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`catalog: ${skills.length}`} variant="outlined" />
            <Chip label={`skill files: ${skillFiles.length}`} variant="outlined" />
            {loading || profileLoading ? <Chip label="loading" color="info" variant="outlined" /> : null}
            <Button variant="outlined" disabled={profileLoading} onClick={() => void loadProfileFiles()}>
              Обновить файлы
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

      {skillFiles.length === 0 && !profileLoading ? (
        <EmptyState title="Skill-файлы не найдены" detail="Ожидается папка skills/<name>/ с SKILL.md и опциональными соседними файлами." />
      ) : (
        <div className="skill-card-grid">
          {skillFiles.map((file) => {
            const name = skillNameFromPath(file.path) ?? file.path;
            const status = skillsByName.get(name);
            const mode = status?.mode ?? "file-only";
            const relativePath = file.path.replace(`skills/${name}/`, "");
            return (
              <button
                key={file.path}
                type="button"
                className={`skill-card ${selectedFile?.path === file.path ? "is-selected" : ""}`}
                onClick={() => void readFile(file)}
              >
                <Stack spacing={1} alignItems="stretch">
                  <Stack direction="row" spacing={1} justifyContent="space-between" alignItems="flex-start">
                    <Typography fontWeight={700} className="mono" sx={{ wordBreak: "break-word" }}>
                      {name}
                    </Typography>
                    <Chip label={mode} color={modeColor(mode)} variant="outlined" />
                  </Stack>
                  <Typography variant="body2" color="text.secondary" sx={{ wordBreak: "break-word" }}>
                    {relativePath === "SKILL.md"
                      ? status?.description || "SKILL.md есть в профиле, но не найден в активном catalog snapshot."
                      : `Дополнительный файл skill folder: ${relativePath}`}
                  </Typography>
                  <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                    <Chip label={`${file.byte_len} bytes`} variant="outlined" />
                    <Chip label={file.path} variant="outlined" className="mono" />
                  </Stack>
                  {onSetEnabled ? (
                    <Stack direction="row" spacing={1}>
                      <Button
                        size="small"
                        variant="outlined"
                        disabled={loading || mode === "manual"}
                        onClick={(event) => {
                          event.stopPropagation();
                          onSetEnabled(name, true);
                        }}
                      >
                        Enable
                      </Button>
                      <Button
                        size="small"
                        color="warning"
                        variant="outlined"
                        disabled={loading || mode === "disabled"}
                        onClick={(event) => {
                          event.stopPropagation();
                          onSetEnabled(name, false);
                        }}
                      >
                        Disable
                      </Button>
                    </Stack>
                  ) : null}
                </Stack>
              </button>
            );
          })}
        </div>
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
