import { Box, Chip, Paper, Stack, Tab, Tabs, Typography } from "@mui/material";
import { useState } from "react";
import { EmptyState, SectionHeader } from "../../components/common";
import type { SessionSummary } from "../../types";
import { ArtifactFilesPane } from "./ArtifactFilesPane";
import { WorkspaceFilesPane } from "./WorkspaceFilesPane";

type FilesTab = "workspace" | "artifacts";

export function FilesScreen({ selectedSession }: { selectedSession: SessionSummary | null }) {
  const [tab, setTab] = useState<FilesTab>("workspace");

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Файлы"
        subtitle="Полный read/download доступ к workspace выбранной сессии и artifact files, которые сохранил runtime."
      />
      {selectedSession ? (
        <>
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1.5}>
              <Box minWidth={0}>
                <Typography variant="h6">{selectedSession.title || selectedSession.id}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {selectedSession.id}
                </Typography>
              </Box>
              <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                <Chip label={selectedSession.agent_name} color="primary" variant="outlined" />
                <Chip label={`messages: ${selectedSession.message_count}`} variant="outlined" />
                <Chip label={`context: ${selectedSession.context_tokens}`} variant="outlined" />
              </Stack>
            </Stack>
          </Paper>

          <Paper variant="outlined">
            <Tabs value={tab} onChange={(_, value: FilesTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
              <Tab value="workspace" label="Workspace" />
              <Tab value="artifacts" label="Artifacts" />
            </Tabs>
          </Paper>

          {tab === "workspace" ? <WorkspaceFilesPane sessionId={selectedSession.id} /> : null}
          {tab === "artifacts" ? <ArtifactFilesPane sessionId={selectedSession.id} /> : null}
        </>
      ) : (
        <EmptyState title="Сессия не выбрана" detail="Открой чат или список сессий и выбери сессию для просмотра файлов." />
      )}
    </Stack>
  );
}
