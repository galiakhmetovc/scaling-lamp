import {
  Alert,
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
import { useEffect, useState } from "react";
import { api } from "../../api";
import { EmptyState } from "../../components/common";
import type { ArtifactFile, ArtifactFileSummary } from "../../types";
import { formatTime } from "../../utils/format";
import { FilePreview } from "./FilePreview";

export function ArtifactFilesPane({ sessionId }: { sessionId: string }) {
  const [artifacts, setArtifacts] = useState<ArtifactFileSummary[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState<ArtifactFile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const result = await api.artifactFiles(sessionId);
      setArtifacts(result.artifacts);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function readArtifact(artifact: ArtifactFileSummary) {
    setError(null);
    try {
      setSelectedArtifact(await api.artifactFile(sessionId, artifact.id));
    } catch (readError) {
      setError(readError instanceof Error ? readError.message : String(readError));
    }
  }

  function download(artifactId: string) {
    window.open(api.artifactDownloadUrl(sessionId, artifactId), "_blank", "noopener,noreferrer");
  }

  useEffect(() => {
    setSelectedArtifact(null);
    void load();
  }, [sessionId]);

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" justifyContent="space-between" alignItems="center" spacing={2}>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`artifacts: ${artifacts.length}`} variant="outlined" />
            {loading ? <Chip label="loading" color="info" variant="outlined" /> : null}
          </Stack>
          <Button variant="outlined" onClick={() => void load()} disabled={loading}>
            Обновить
          </Button>
        </Stack>
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}

      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>ID</TableCell>
              <TableCell>Kind</TableCell>
              <TableCell>Путь</TableCell>
              <TableCell>Размер</TableCell>
              <TableCell>Создан</TableCell>
              <TableCell align="right">Действия</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {artifacts.map((artifact) => (
              <TableRow key={artifact.id} hover>
                <TableCell className="mono">{artifact.id}</TableCell>
                <TableCell>{artifact.kind}</TableCell>
                <TableCell className="mono">{artifact.path}</TableCell>
                <TableCell>{artifact.byte_len}</TableCell>
                <TableCell>{formatTime(artifact.created_at)}</TableCell>
                <TableCell align="right">
                  <Stack direction="row" spacing={1} justifyContent="flex-end">
                    <Button size="small" variant="outlined" onClick={() => void readArtifact(artifact)}>
                      Читать
                    </Button>
                    <Button size="small" variant="outlined" onClick={() => download(artifact.id)}>
                      Скачать
                    </Button>
                  </Stack>
                </TableCell>
              </TableRow>
            ))}
            {!loading && artifacts.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6}>
                  <EmptyState title="Artifact files не найдены" detail="В этой сессии пока нет сохранённых artifact files." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>

      {selectedArtifact ? (
        <Stack spacing={1}>
          <FilePreview
            title={selectedArtifact.id}
            subtitle={selectedArtifact.path}
            bytes={selectedArtifact.byte_len}
            content={selectedArtifact.content}
            text={selectedArtifact.text}
            truncated={selectedArtifact.content_truncated}
            onDownload={() => download(selectedArtifact.id)}
          />
          <Typography component="pre" className="file-preview">
            {selectedArtifact.metadata_json || "{}"}
          </Typography>
        </Stack>
      ) : null}
    </Stack>
  );
}
