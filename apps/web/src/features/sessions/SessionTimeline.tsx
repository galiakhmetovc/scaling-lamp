import { Box, Chip, Divider, Paper, Stack, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import { formatTime, short } from "../../utils/format";
import { eventTone, type SessionEvent } from "./sessionEvents";

export function SessionTimeline({
  events,
  selectedEventId,
  onSelectEvent
}: {
  events: SessionEvent[];
  selectedEventId: string | null;
  onSelectEvent: (id: string) => void;
}) {
  if (events.length === 0) {
    return <EmptyState title="Timeline пуст" detail="Нет transcript/debug событий для выбранной сессии." />;
  }

  return (
    <Paper variant="outlined" className="timeline-panel">
      <Stack divider={<Divider flexItem />} spacing={0}>
        {events.map((event) => (
          <Box
            key={event.id}
            component="button"
            type="button"
            className={`timeline-event ${event.id === selectedEventId ? "is-selected" : ""}`}
            onClick={() => onSelectEvent(event.id)}
          >
            <Box className="timeline-marker" />
            <Box className="timeline-body">
              <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
                <Chip label={event.kind} color={eventTone(event.kind)} variant="outlined" />
                <Typography variant="caption" color="text.secondary">
                  {formatTime(event.createdAt)}
                </Typography>
                {event.runId ? (
                  <Typography variant="caption" color="text.secondary" className="mono">
                    run {short(event.runId, 18)}
                  </Typography>
                ) : null}
                {event.artifactId ? (
                  <Typography variant="caption" color="text.secondary" className="mono">
                    artifact {short(event.artifactId, 18)}
                  </Typography>
                ) : null}
              </Stack>
              <Typography fontWeight={700} sx={{ mt: 0.75 }}>
                {event.label || event.detailTitle}
              </Typography>
              <Typography variant="body2" color="text.secondary" noWrap>
                {event.detailTitle}
              </Typography>
              <Typography component="pre" className="timeline-preview">
                {event.detail}
              </Typography>
            </Box>
          </Box>
        ))}
      </Stack>
    </Paper>
  );
}
