import {
  Button,
  Chip,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography
} from "@mui/material";
import type { McpResource, McpResourceList } from "../../types";

export function McpResourcesTable({
  resources,
  onRead
}: {
  resources: McpResourceList | null;
  onRead: (resource: McpResource) => void;
}) {
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Resource</TableCell>
            <TableCell>Connector</TableCell>
            <TableCell>MIME</TableCell>
            <TableCell align="right">Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {(resources?.results ?? []).map((resource) => (
            <TableRow key={`${resource.connector_id}-${resource.uri}`} hover>
              <TableCell>
                <Typography variant="body2" fontWeight={700}>
                  {resource.title || resource.name}
                </Typography>
                <Typography variant="caption" className="mono" color="text.secondary">
                  {resource.uri}
                </Typography>
                {resource.description ? (
                  <Typography variant="caption" component="div" color="text.secondary">
                    {resource.description}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>
                <Chip label={resource.connector_id} size="small" variant="outlined" />
              </TableCell>
              <TableCell>{resource.mime_type || "—"}</TableCell>
              <TableCell align="right">
                <Button size="small" onClick={() => onRead(resource)}>
                  Read
                </Button>
              </TableCell>
            </TableRow>
          ))}
          {resources && resources.results.length === 0 ? (
            <TableRow>
              <TableCell colSpan={4}>MCP resources не найдены.</TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
