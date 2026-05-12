import { createFileRoute } from '@tanstack/react-router'
import { requireLocalOrAuth } from '../../server/auth-middleware'
import {
  fetchAgentdJson,
  getAgentdBaseUrl,
  summarizeAgentdFailure,
  type TeamdRuntimeStatus,
  type TeamdWebSnapshot,
} from '../../server/teamd-agentd-client'

type TeamdStatusBody = {
  ok: boolean
  agentdUrl: string
  status?: TeamdRuntimeStatus
  snapshot?: TeamdWebSnapshot
  errors: Array<string>
}

export const Route = createFileRoute('/api/teamd-status')({
  server: {
    handlers: {
      GET: async ({ request }) => {
        if (!requireLocalOrAuth(request)) {
          return Response.json(
            {
              ok: false,
              agentdUrl: getAgentdBaseUrl(),
              errors: ['Authentication required'],
            } satisfies TeamdStatusBody,
            { status: 401 },
          )
        }

        const [statusResult, snapshotResult] = await Promise.all([
          fetchAgentdJson<TeamdRuntimeStatus>('/v1/status'),
          fetchAgentdJson<TeamdWebSnapshot>('/v1/web/snapshot'),
        ])

        const errors = [
          summarizeAgentdFailure(statusResult),
          summarizeAgentdFailure(snapshotResult),
        ].filter((entry): entry is string => Boolean(entry))

        return Response.json(
          {
            ok: statusResult.ok,
            agentdUrl: getAgentdBaseUrl(),
            status: statusResult.ok ? statusResult.data : undefined,
            snapshot: snapshotResult.ok ? snapshotResult.data : undefined,
            errors,
          } satisfies TeamdStatusBody,
          { status: statusResult.ok ? 200 : 503 },
        )
      },
    },
  },
})

