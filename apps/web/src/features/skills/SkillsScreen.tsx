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
import { EmptyState, SectionHeader } from "../../components/common";
import type { SessionSkillStatus, SessionSummary } from "../../types";

function modeColor(mode: string): "success" | "warning" | "default" {
  if (mode === "automatic" || mode === "manual") {
    return "success";
  }
  if (mode === "disabled") {
    return "warning";
  }
  return "default";
}

export function SkillsScreen({ selectedSession }: { selectedSession: SessionSummary | null }) {
  const [skills, setSkills] = useState<SessionSkillStatus[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    if (!selectedSession) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setSkills(await api.sessionSkills(selectedSession.id));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function setSkillEnabled(name: string, enabled: boolean) {
    if (!selectedSession) {
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const nextSkills = enabled
        ? await api.enableSessionSkill(selectedSession.id, name)
        : await api.disableSessionSkill(selectedSession.id, name);
      setSkills(nextSkills);
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : String(updateError));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    setSkills([]);
    void load();
  }, [selectedSession?.id]);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Skills"
        subtitle="Навыки выбранной сессии: automatic активируется по триггерам, manual включён вручную, disabled выключен."
        action={
          <Button variant="outlined" disabled={loading || !selectedSession} onClick={() => void load()}>
            Обновить
          </Button>
        }
      />
      {selectedSession ? (
        <>
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Typography variant="h6">{selectedSession.title || selectedSession.id}</Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {selectedSession.id} · {selectedSession.agent_name}
            </Typography>
          </Paper>
          {error ? <Alert severity="error">{error}</Alert> : null}
          <TableContainer component={Paper} variant="outlined">
            <Table size="small">
              <TableHead>
                <TableRow>
                  <TableCell>Skill</TableCell>
                  <TableCell>Описание</TableCell>
                  <TableCell>Mode</TableCell>
                  <TableCell align="right">Действия</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {skills.map((skill) => (
                  <TableRow key={skill.name} hover>
                    <TableCell className="mono">{skill.name}</TableCell>
                    <TableCell>{skill.description}</TableCell>
                    <TableCell>
                      <Chip label={skill.mode} color={modeColor(skill.mode)} variant="outlined" />
                    </TableCell>
                    <TableCell align="right">
                      <Stack direction="row" spacing={1} justifyContent="flex-end">
                        <Button
                          size="small"
                          variant="outlined"
                          disabled={loading || skill.mode === "manual"}
                          onClick={() => void setSkillEnabled(skill.name, true)}
                        >
                          Enable
                        </Button>
                        <Button
                          size="small"
                          color="warning"
                          variant="outlined"
                          disabled={loading || skill.mode === "disabled"}
                          onClick={() => void setSkillEnabled(skill.name, false)}
                        >
                          Disable
                        </Button>
                      </Stack>
                    </TableCell>
                  </TableRow>
                ))}
                {!loading && skills.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={4}>
                      <EmptyState title="Skills не найдены" detail="Для выбранной сессии каталог навыков пуст или недоступен." />
                    </TableCell>
                  </TableRow>
                ) : null}
              </TableBody>
            </Table>
          </TableContainer>
        </>
      ) : (
        <EmptyState title="Сессия не выбрана" detail="Выбери сессию, чтобы увидеть её skills и режимы активации." />
      )}
    </Stack>
  );
}
