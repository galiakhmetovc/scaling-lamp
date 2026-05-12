import { createFileRoute } from '@tanstack/react-router'
import { requireLocalOrAuth } from '../../server/auth-middleware'
import {
  fetchAgentdJson,
  getAgentdBaseUrl,
  type TeamdRuntimeStatus,
} from '../../server/teamd-agentd-client'

type PingResponse = {
  ok: boolean
  error?: string
  status?: number
  agentdUrl: string
  runtime?: TeamdRuntimeStatus
}

export const Route = createFileRoute('/api/ping')({
  server: {
    handlers: {
      GET: async ({ request }) => {
        if (!requireLocalOrAuth(request)) {
          return Response.json(
            {
              ok: false,
              error: 'Authentication required',
              status: 401,
              agentdUrl: getAgentdBaseUrl(),
            } satisfies PingResponse,
            { status: 401 },
          )
        }

        const statusResult = await fetchAgentdJson<TeamdRuntimeStatus>('/v1/status')
        if (!statusResult.ok) {
          return Response.json(
            {
              ok: false,
              error: statusResult.error,
              status: 503,
              agentdUrl: getAgentdBaseUrl(),
            } satisfies PingResponse,
            { status: 503 },
          )
        }

        return Response.json(
          {
            ok: true,
            status: 200,
            agentdUrl: getAgentdBaseUrl(),
            runtime: statusResult.data,
          } satisfies PingResponse,
          { status: 200 },
        )
      },
    },
  },
})
