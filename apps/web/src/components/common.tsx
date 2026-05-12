import type { ReactNode } from "react";
import {
  Box,
  Chip,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableRow,
  Typography
} from "@mui/material";
import { statusColor } from "../utils/format";

export function SectionHeader({
  title,
  subtitle,
  action
}: {
  title: string;
  subtitle?: string;
  action?: ReactNode;
}) {
  return (
    <Stack direction="row" alignItems="flex-start" justifyContent="space-between" spacing={2} sx={{ mb: 2 }}>
      <Box>
        <Typography variant="h5">{title}</Typography>
        {subtitle ? (
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            {subtitle}
          </Typography>
        ) : null}
      </Box>
      {action}
    </Stack>
  );
}

export function EmptyState({ title, detail }: { title: string; detail?: string }) {
  return (
    <Paper variant="outlined" sx={{ p: 3 }}>
      <Typography fontWeight={700}>{title}</Typography>
      {detail ? (
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
          {detail}
        </Typography>
      ) : null}
    </Paper>
  );
}

export function Metric({ label, value, hint }: { label: string; value: ReactNode; hint?: string }) {
  return (
    <Paper variant="outlined" sx={{ p: 1.5, minWidth: 150 }}>
      <Typography variant="caption" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="h6" sx={{ mt: 0.5 }}>
        {value}
      </Typography>
      {hint ? (
        <Typography variant="caption" color="text.secondary">
          {hint}
        </Typography>
      ) : null}
    </Paper>
  );
}

export function StatusChip({ value }: { value?: string | null }) {
  return <Chip label={value || "unknown"} color={statusColor(value)} variant="outlined" />;
}

export function JsonBlock({ value }: { value: unknown }) {
  return <pre className="json-block">{typeof value === "string" ? value : JSON.stringify(value, null, 2)}</pre>;
}

export function KeyValueTable({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <Table size="small">
      <TableBody>
        {rows.map(([label, value]) => (
          <TableRow key={label}>
            <TableCell sx={{ width: 118, color: "text.secondary" }}>{label}</TableCell>
            <TableCell>{value}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
