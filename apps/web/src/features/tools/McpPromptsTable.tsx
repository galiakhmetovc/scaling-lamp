import {
  Button,
  Chip,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography
} from "@mui/material";
import type { McpPrompt, McpPromptList } from "../../types";

export function McpPromptsTable({
  prompts,
  onGet
}: {
  prompts: McpPromptList | null;
  onGet: (prompt: McpPrompt) => void;
}) {
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Prompt</TableCell>
            <TableCell>Connector</TableCell>
            <TableCell>Arguments</TableCell>
            <TableCell align="right">Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {(prompts?.results ?? []).map((prompt) => (
            <TableRow key={`${prompt.connector_id}-${prompt.name}`} hover>
              <TableCell>
                <Typography variant="body2" fontWeight={700}>
                  {prompt.title || prompt.name}
                </Typography>
                <Typography variant="caption" className="mono" color="text.secondary">
                  {prompt.name}
                </Typography>
                {prompt.description ? (
                  <Typography variant="caption" component="div" color="text.secondary">
                    {prompt.description}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>
                <Chip label={prompt.connector_id} size="small" variant="outlined" />
              </TableCell>
              <TableCell>
                <Stack direction="row" spacing={0.5} flexWrap="wrap" useFlexGap>
                  {prompt.arguments.length ? (
                    prompt.arguments.map((argument) => (
                      <Chip
                        key={argument.name}
                        label={`${argument.name}${argument.required ? " *" : ""}`}
                        size="small"
                        variant="outlined"
                      />
                    ))
                  ) : (
                    <Typography variant="caption" color="text.secondary">
                      нет аргументов
                    </Typography>
                  )}
                </Stack>
              </TableCell>
              <TableCell align="right">
                <Button size="small" onClick={() => onGet(prompt)}>
                  Get
                </Button>
              </TableCell>
            </TableRow>
          ))}
          {prompts && prompts.results.length === 0 ? (
            <TableRow>
              <TableCell colSpan={4}>MCP prompts не найдены.</TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
