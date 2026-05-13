import {
  Alert,
  Box,
  Button,
  Chip,
  Paper,
  Stack,
  Tab,
  Tabs,
  Typography
} from "@mui/material";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState, SectionHeader } from "../../components/common";
import type { SessionSkillStatus, SessionSummary } from "../../types";
import { AgentPromptFilesEditor } from "./AgentPromptFilesEditor";
import { AgentSkillCardsEditor } from "./AgentSkillCardsEditor";

type SkillsTab = "skills" | "prompts";

export function SkillsScreen({ selectedSession }: { selectedSession: SessionSummary | null }) {
  const [tab, setTab] = useState<SkillsTab>("skills");
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
        subtitle="Skills управляются как карточки. SYSTEM.md и AGENTS.md редактируются отдельно как prompt-файлы профиля."
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
              {selectedSession.id} · {selectedSession.agent_name} · profile={selectedSession.agent_profile_id}
            </Typography>
          </Paper>
          {error ? <Alert severity="error">{error}</Alert> : null}
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`skills: ${skills.length}`} variant="outlined" />
            <Chip label={`agent: ${selectedSession.agent_profile_id}`} color="primary" variant="outlined" />
          </Stack>
          <Paper variant="outlined">
            <Tabs value={tab} onChange={(_, value: SkillsTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
              <Tab value="skills" label="Skills" />
              <Tab value="prompts" label="SYSTEM / AGENTS" />
            </Tabs>
          </Paper>
          <Box>
            {tab === "skills" ? (
              <AgentSkillCardsEditor
                agentId={selectedSession.agent_profile_id}
                skills={skills}
                loading={loading}
                onSetEnabled={(name, enabled) => void setSkillEnabled(name, enabled)}
              />
            ) : null}
            {tab === "prompts" ? <AgentPromptFilesEditor agentId={selectedSession.agent_profile_id} /> : null}
          </Box>
        </>
      ) : (
        <EmptyState title="Сессия не выбрана" detail="Выбери сессию, чтобы увидеть её skills и режимы активации." />
      )}
    </Stack>
  );
}
