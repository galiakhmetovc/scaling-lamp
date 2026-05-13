import { Alert, Box, Button, Paper, Stack, Tab, Tabs } from "@mui/material";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState, SectionHeader } from "../../components/common";
import type { AgentDetail, AgentSummary, AgentUpdatePatch, SessionSummary } from "../../types";
import { AgentPromptFilesEditor } from "../skills/AgentPromptFilesEditor";
import { AgentSkillCardsEditor } from "../skills/AgentSkillCardsEditor";
import { AgentLinkedSessions } from "./AgentLinkedSessions";
import { AgentProfileEditor } from "./AgentProfileEditor";
import { AgentsListPane } from "./AgentsListPane";
import { sessionsForAgent } from "./agentProfile";

type AgentTab = "profile" | "prompts" | "skills";

export function AgentsScreen({
  agents,
  sessions,
  loading,
  onCreateAgent,
  onOpenSession,
  onRefresh
}: {
  agents: AgentSummary[];
  sessions: SessionSummary[];
  loading: boolean;
  onCreateAgent: () => void;
  onOpenSession: (sessionId: string) => void;
  onRefresh: () => void;
}) {
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(agents[0]?.id ?? null);
  const [detail, setDetail] = useState<AgentDetail | null>(null);
  const [tab, setTab] = useState<AgentTab>("profile");
  const [detailLoading, setDetailLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  async function loadDetail(agentId: string, signal?: AbortSignal) {
    setDetailLoading(true);
    setError(null);
    try {
      setDetail(await api.agentDetail(agentId, signal));
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setDetailLoading(false);
      }
    }
  }

  async function saveProfile(patch: AgentUpdatePatch) {
    if (!selectedAgentId) {
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await api.updateAgent(selectedAgentId, patch);
      setDetail(updated);
      setSelectedAgentId(updated.id);
      setNotice("Профиль агента сохранён.");
      onRefresh();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function deleteProfile() {
    if (!selectedAgentId || !detail) {
      return;
    }
    if (!window.confirm(`Удалить профиль агента ${detail.name} (${detail.id})? Файлы agent_home не удаляются.`)) {
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      await api.deleteAgent(selectedAgentId);
      setNotice(`Профиль ${detail.id} удалён.`);
      setDetail(null);
      const nextAgent = agents.find((agent) => agent.id !== selectedAgentId) ?? null;
      setSelectedAgentId(nextAgent?.id ?? null);
      onRefresh();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : String(deleteError));
    } finally {
      setSaving(false);
    }
  }

  useEffect(() => {
    setSelectedAgentId((current) => {
      if (current && agents.some((agent) => agent.id === current)) {
        return current;
      }
      return agents[0]?.id ?? null;
    });
  }, [agents]);

  useEffect(() => {
    if (!selectedAgentId) {
      setDetail(null);
      return;
    }
    const controller = new AbortController();
    void loadDetail(selectedAgentId, controller.signal);
    return () => controller.abort();
  }, [selectedAgentId]);

  const linkedSessions = selectedAgentId ? sessionsForAgent(sessions, selectedAgentId) : [];

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Агенты"
        subtitle="Создание, prompt-файлы, skills и allowed tools для Agent Profiles."
        action={
          <Button variant="outlined" disabled={loading} onClick={onRefresh}>
            Обновить
          </Button>
        }
      />
      <Box className="agents-layout">
        <AgentsListPane
          agents={agents}
          selectedAgentId={selectedAgentId}
          onSelectAgent={(agentId) => {
            setTab("profile");
            setSelectedAgentId(agentId);
          }}
          onCreate={onCreateAgent}
        />

        <Stack spacing={1.5} minWidth={0}>
          {error ? <Alert severity="error">{error}</Alert> : null}
          {detailLoading ? <Alert severity="info">Загружаю профиль агента...</Alert> : null}
          {!selectedAgentId ? (
            <EmptyState title="Агент не выбран" detail="Выбери профиль слева или создай нового агента." />
          ) : null}
          {detail ? (
            <>
              <Paper variant="outlined">
                <Tabs value={tab} onChange={(_, value: AgentTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
                  <Tab value="profile" label="Профиль / tools" />
                  <Tab value="prompts" label="SYSTEM / AGENTS" />
                  <Tab value="skills" label="Skills" />
                </Tabs>
              </Paper>
              {tab === "profile" ? (
                <>
                  <AgentProfileEditor
                    agent={detail}
                    saving={saving}
                    error={null}
                    notice={notice}
                    onSave={(patch) => void saveProfile(patch)}
                    onDelete={() => void deleteProfile()}
                  />
                  <Paper variant="outlined" sx={{ p: 1.5 }}>
                    <AgentLinkedSessions
                      sessions={linkedSessions}
                      onOpenSession={(sessionId) => {
                        onOpenSession(sessionId);
                      }}
                    />
                  </Paper>
                </>
              ) : null}
              {tab === "prompts" ? <AgentPromptFilesEditor agentId={detail.id} /> : null}
              {tab === "skills" ? <AgentSkillCardsEditor agentId={detail.id} skills={[]} loading={false} /> : null}
            </>
          ) : null}
        </Stack>
      </Box>
    </Stack>
  );
}
