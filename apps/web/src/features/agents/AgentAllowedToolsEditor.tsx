import { Alert, Button, Chip, Paper, Stack, TextField, Typography } from "@mui/material";
import { useState } from "react";
import type { ToolCatalogItem } from "../../types";
import { agentAllowsTool } from "../tools/toolCatalog";
import { formatAllowedTools, toggleAllowedTool } from "./agentProfile";

export function AgentAllowedToolsEditor({
  allowedTools,
  allowedToolsText,
  toolCatalog,
  onChange
}: {
  allowedTools: string[];
  allowedToolsText: string;
  toolCatalog: ToolCatalogItem[];
  onChange: (value: string) => void;
}) {
  const [filter, setFilter] = useState("");
  const normalizedFilter = filter.trim().toLowerCase();
  const visibleTools = toolCatalog
    .filter((tool) => {
      if (!normalizedFilter) {
        return true;
      }
      return [tool.id, tool.family, tool.origin, tool.connector_id ?? "", tool.description]
        .join(" ")
        .toLowerCase()
        .includes(normalizedFilter);
    })
    .slice(0, 80);

  function setTool(toolId: string) {
    onChange(formatAllowedTools(toggleAllowedTool(allowedTools, toolId)));
  }

  return (
    <Stack spacing={1.25}>
      <TextField
        fullWidth
        multiline
        minRows={8}
        label="Allowed tools"
        value={allowedToolsText}
        onChange={(event) => onChange(event.target.value)}
        helperText="Один tool id на строку. Пустые строки и дубли будут убраны. Ниже можно включать/выключать tools из runtime catalog."
        inputProps={{ className: "mono" }}
      />
      <Paper variant="outlined" sx={{ p: 1.25 }}>
        <Stack spacing={1}>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems={{ md: "center" }}>
            <Typography fontWeight={700}>Catalog picker</Typography>
            <Chip label={`${toolCatalog.length} tools`} size="small" variant="outlined" />
            <Chip label={`${allowedTools.length} allowed`} size="small" variant="outlined" />
            <TextField
              size="small"
              label="Фильтр"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              sx={{ minWidth: 260 }}
            />
          </Stack>
          {visibleTools.length === 0 ? (
            <Typography variant="body2" color="text.secondary">
              Нет tools под текущий фильтр.
            </Typography>
          ) : (
            <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
              {visibleTools.map((tool) => {
                const allowed = agentAllowsTool(allowedTools, tool.id);
                return (
                  <Button
                    key={tool.id}
                    size="small"
                    color={allowed ? "success" : "inherit"}
                    variant={allowed ? "contained" : "outlined"}
                    onClick={() => setTool(tool.id)}
                    title={`${tool.id}\n${tool.description}`}
                    disabled={!tool.available}
                  >
                    {tool.id}
                  </Button>
                );
              })}
            </Stack>
          )}
          {toolCatalog.length === 0 ? (
            <Alert severity="warning">
              Runtime tool catalog не загружен. Можно редактировать raw list вручную, но picker недоступен.
            </Alert>
          ) : null}
        </Stack>
      </Paper>
    </Stack>
  );
}
