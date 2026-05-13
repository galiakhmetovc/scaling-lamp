import { Box, Chip, FormControl, InputLabel, MenuItem, Paper, Select, Stack, Tab, Tabs, Typography } from "@mui/material";
import { useEffect, useState } from "react";
import { EmptyState, SectionHeader } from "../../components/common";
import type { AgentSummary, SessionSummary } from "../../types";
import { short } from "../../utils/format";
import { AgentHomeFilesPane } from "./AgentHomeFilesPane";
import { ArtifactFilesPane } from "./ArtifactFilesPane";
import { WorkspaceFilesPane } from "./WorkspaceFilesPane";

type FilesTab = "workspace" | "artifacts" | "agent_home";

export function FilesScreen({
  selectedSession,
  sessions,
  agents
}: {
  selectedSession: SessionSummary | null;
  sessions: SessionSummary[];
  agents: AgentSummary[];
}) {
  const [tab, setTab] = useState<FilesTab>("workspace");
  const [sessionId, setSessionId] = useState(selectedSession?.id ?? sessions[0]?.id ?? "");
  const [agentId, setAgentId] = useState(selectedSession?.agent_profile_id ?? agents[0]?.id ?? "");

  useEffect(() => {
    if (selectedSession?.id) {
      setSessionId(selectedSession.id);
      setAgentId(selectedSession.agent_profile_id);
    }
  }, [selectedSession?.id, selectedSession?.agent_profile_id]);

  const effectiveSession = sessions.find((session) => session.id === sessionId) ?? selectedSession;

  useEffect(() => {
    if (!effectiveSession && agentId && tab !== "agent_home") {
      setTab("agent_home");
    }
  }, [agentId, effectiveSession, tab]);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Файлы"
        subtitle="Единый файловый экран: workspace сессии, artifacts сессии и agent_home выбранного Agent Profile. Проекты на диске не удаляются отсюда."
      />
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
          <FormControl size="small" fullWidth>
            <InputLabel id="files-session-label">Session root</InputLabel>
            <Select
              labelId="files-session-label"
              label="Session root"
              value={effectiveSession?.id ?? ""}
              onChange={(event) => {
                const nextSession = sessions.find((session) => session.id === event.target.value);
                setSessionId(event.target.value);
                if (nextSession) {
                  setAgentId(nextSession.agent_profile_id);
                }
              }}
            >
              {sessions.map((session) => (
                <MenuItem key={session.id} value={session.id}>
                  {session.title || short(session.id, 24)} · {session.agent_name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
          <FormControl size="small" fullWidth>
            <InputLabel id="files-agent-label">Agent home</InputLabel>
            <Select
              labelId="files-agent-label"
              label="Agent home"
              value={agentId}
              onChange={(event) => setAgentId(event.target.value)}
            >
              {agents.map((agent) => (
                <MenuItem key={agent.id} value={agent.id}>
                  {agent.name} ({agent.id})
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        </Stack>
      </Paper>
      {effectiveSession || agentId ? (
        <>
          {effectiveSession ? (
            <Paper variant="outlined" sx={{ p: 1.5 }}>
              <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1.5}>
                <Box minWidth={0}>
                  <Typography variant="h6">{effectiveSession.title || effectiveSession.id}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {effectiveSession.id}
                  </Typography>
                </Box>
                <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                  <Chip label={effectiveSession.agent_name} color="primary" variant="outlined" />
                  <Chip label={`messages: ${effectiveSession.message_count}`} variant="outlined" />
                  <Chip label={`context: ${effectiveSession.context_tokens}`} variant="outlined" />
                </Stack>
              </Stack>
            </Paper>
          ) : null}

          <Paper variant="outlined">
            <Tabs value={tab} onChange={(_, value: FilesTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
              <Tab value="workspace" label="Session workspace" disabled={!effectiveSession} />
              <Tab value="artifacts" label="Session artifacts" disabled={!effectiveSession} />
              <Tab value="agent_home" label="Agent home" disabled={!agentId} />
            </Tabs>
          </Paper>

          {tab === "workspace" && effectiveSession ? <WorkspaceFilesPane sessionId={effectiveSession.id} /> : null}
          {tab === "artifacts" && effectiveSession ? <ArtifactFilesPane sessionId={effectiveSession.id} /> : null}
          {tab === "agent_home" && agentId ? <AgentHomeFilesPane agentId={agentId} /> : null}
        </>
      ) : (
        <EmptyState title="Нет доступных файловых root" detail="Создай сессию или агента, чтобы увидеть workspace, artifacts или agent_home." />
      )}
    </Stack>
  );
}
