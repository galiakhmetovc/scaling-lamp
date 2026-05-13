import { Stack } from "@mui/material";
import type {
  AgentSummary,
  DeliveryTarget,
  DeliveryTargetCreateOptions,
  DeliveryTargetUpdatePatch,
  SessionOutputRoute,
  SessionOutputRouteCreateOptions,
  SessionOutputRouteUpdatePatch,
  SessionSummary,
  TelegramChat
} from "../../types";
import { DeliveryTargetsPanel } from "./DeliveryTargetsPanel";
import { SessionOutputRoutesPanel } from "./SessionOutputRoutesPanel";
import { TelegramBindingsPanel } from "./TelegramBindingsPanel";

export function RoutesView({
  targets,
  outputRoutes,
  chats,
  sessions,
  agents,
  onOpenSession,
  onCreateTarget,
  onUpdateTarget,
  onCreateOutputRoute,
  onUpdateOutputRoute
}: {
  targets: DeliveryTarget[];
  outputRoutes: SessionOutputRoute[];
  chats: TelegramChat[];
  sessions: SessionSummary[];
  agents: AgentSummary[];
  onOpenSession: (sessionId: string) => void;
  onCreateTarget: (targetId: string, options: DeliveryTargetCreateOptions) => Promise<void>;
  onUpdateTarget: (targetId: string, patch: DeliveryTargetUpdatePatch) => Promise<void>;
  onCreateOutputRoute: (sessionId: string, targetId: string, options: SessionOutputRouteCreateOptions) => Promise<void>;
  onUpdateOutputRoute: (routeId: string, patch: SessionOutputRouteUpdatePatch) => Promise<void>;
}) {
  return (
    <Stack spacing={2}>
      <DeliveryTargetsPanel targets={targets} sessions={sessions} agents={agents} onCreate={onCreateTarget} onUpdate={onUpdateTarget} />
      <SessionOutputRoutesPanel
        routes={outputRoutes}
        targets={targets}
        sessions={sessions}
        onOpenSession={onOpenSession}
        onCreate={onCreateOutputRoute}
        onUpdate={onUpdateOutputRoute}
      />
      <TelegramBindingsPanel chats={chats} sessions={sessions} agents={agents} onOpenSession={onOpenSession} />
    </Stack>
  );
}
