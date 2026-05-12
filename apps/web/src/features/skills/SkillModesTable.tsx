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
  TableRow
} from "@mui/material";
import { EmptyState } from "../../components/common";
import type { SessionSkillStatus } from "../../types";

function modeColor(mode: string): "success" | "warning" | "default" {
  if (mode === "automatic" || mode === "manual") {
    return "success";
  }
  if (mode === "disabled") {
    return "warning";
  }
  return "default";
}

export function SkillModesTable({
  skills,
  loading,
  onSetEnabled
}: {
  skills: SessionSkillStatus[];
  loading: boolean;
  onSetEnabled: (name: string, enabled: boolean) => void;
}) {
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Skill</TableCell>
            <TableCell>Описание</TableCell>
            <TableCell>Mode</TableCell>
            <TableCell align="right">Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {skills.map((skill) => (
            <TableRow key={skill.name} hover>
              <TableCell className="mono">{skill.name}</TableCell>
              <TableCell>{skill.description}</TableCell>
              <TableCell>
                <Chip label={skill.mode} color={modeColor(skill.mode)} variant="outlined" />
              </TableCell>
              <TableCell align="right">
                <Stack direction="row" spacing={1} justifyContent="flex-end">
                  <Button
                    size="small"
                    variant="outlined"
                    disabled={loading || skill.mode === "manual"}
                    onClick={() => onSetEnabled(skill.name, true)}
                  >
                    Enable
                  </Button>
                  <Button
                    size="small"
                    color="warning"
                    variant="outlined"
                    disabled={loading || skill.mode === "disabled"}
                    onClick={() => onSetEnabled(skill.name, false)}
                  >
                    Disable
                  </Button>
                </Stack>
              </TableCell>
            </TableRow>
          ))}
          {!loading && skills.length === 0 ? (
            <TableRow>
              <TableCell colSpan={4}>
                <EmptyState title="Skills не найдены" detail="Для выбранной сессии каталог навыков пуст или недоступен." />
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
