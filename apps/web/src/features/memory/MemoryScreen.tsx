import { useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  MenuItem,
  Paper,
  Stack,
  Tab,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Tabs,
  TextField,
  Typography
} from "@mui/material";
import { api } from "../../api";
import { EmptyState, JsonBlock, Metric, SectionHeader } from "../../components/common";
import type {
  KvEntry,
  KvList,
  MemoryRecallPreview,
  SemanticMemoryItem,
  SemanticMemoryList,
  SemanticMemorySearch,
  SessionSummary
} from "../../types";
import { formatTime, short } from "../../utils/format";
import { describeMemoryLayer, jsonPreview, memoryScopeRequiresSession, memoryScopes, parseJsonInput, type MemoryScope } from "./memoryModel";

type MemoryTab = "recall" | "semantic" | "kv" | "boundary";

export function MemoryScreen({ selectedSession }: { selectedSession: SessionSummary | null }) {
  const [tab, setTab] = useState<MemoryTab>("recall");
  const [scope, setScope] = useState<MemoryScope>("operator");
  const [memoryQuery, setMemoryQuery] = useState("");
  const [recallQuery, setRecallQuery] = useState("");
  const [memoryOffset, setMemoryOffset] = useState(0);
  const [kvOffset, setKvOffset] = useState(0);
  const [kvPrefix, setKvPrefix] = useState("");
  const [semanticList, setSemanticList] = useState<SemanticMemoryList | null>(null);
  const [semanticSearch, setSemanticSearch] = useState<SemanticMemorySearch | null>(null);
  const [kvList, setKvList] = useState<KvList | null>(null);
  const [recallPreview, setRecallPreview] = useState<MemoryRecallPreview | null>(null);
  const [editingMemory, setEditingMemory] = useState<SemanticMemoryItem | null>(null);
  const [memoryEditText, setMemoryEditText] = useState("");
  const [memoryEditMetadata, setMemoryEditMetadata] = useState("null");
  const [kvKey, setKvKey] = useState("");
  const [kvValue, setKvValue] = useState("{}");
  const [kvMetadata, setKvMetadata] = useState("null");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sessionId = selectedSession?.id ?? null;
  const scopeNeedsSession = memoryScopeRequiresSession(scope);
  const scopeAvailable = !scopeNeedsSession || Boolean(sessionId);

  async function load(signal?: AbortSignal) {
    if (!scopeAvailable) {
      setSemanticList(null);
      setSemanticSearch(null);
      setKvList(null);
      setRecallPreview(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [nextSemanticList, nextKvList, nextRecall] = await Promise.all([
        api.semanticMemoryList(sessionId, { scope, limit: 20, offset: memoryOffset }, signal),
        api.kvList(sessionId, { scope, prefix: kvPrefix, limit: 50, offset: kvOffset }, signal),
        sessionId ? api.memoryRecallPreview(sessionId, recallQuery, signal) : Promise.resolve(null)
      ]);
      setSemanticList(nextSemanticList);
      setSemanticSearch(null);
      setKvList(nextKvList);
      setRecallPreview(nextRecall);
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setLoading(false);
      }
    }
  }

  async function searchMemory() {
    if (!scopeAvailable) {
      return;
    }
    const query = memoryQuery.trim();
    if (!query) {
      setSemanticSearch(null);
      await load();
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const result = await api.semanticMemorySearch(sessionId, { query, scope, limit: 20 });
      setSemanticSearch(result);
    } catch (searchError) {
      setError(searchError instanceof Error ? searchError.message : String(searchError));
    } finally {
      setLoading(false);
    }
  }

  async function saveMemoryEdit() {
    if (!editingMemory) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.semanticMemoryUpdate(editingMemory.id, memoryEditText, parseJsonInput(memoryEditMetadata));
      setEditingMemory(null);
      await load();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setBusy(false);
    }
  }

  async function deleteMemory(memory: SemanticMemoryItem) {
    if (!window.confirm(`Удалить memory ${memory.id}?`)) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.semanticMemoryDelete(memory.id);
      await load();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : String(deleteError));
    } finally {
      setBusy(false);
    }
  }

  async function putKv() {
    if (!scopeAvailable || !kvKey.trim()) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.kvPut(sessionId, {
        scope,
        key: kvKey.trim(),
        value: parseJsonInput(kvValue),
        metadata: parseJsonInput(kvMetadata)
      });
      setKvKey("");
      await load();
    } catch (putError) {
      setError(putError instanceof Error ? putError.message : String(putError));
    } finally {
      setBusy(false);
    }
  }

  async function deleteKv(entry: KvEntry) {
    if (!scopeAvailable || !window.confirm(`Удалить KV ${entry.scope}/${entry.namespace_id}/${entry.key}?`)) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.kvDelete(sessionId, {
        scope: entry.scope,
        key: entry.key,
        expected_revision: entry.revision
      });
      await load();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : String(deleteError));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [sessionId, scope, memoryOffset, kvOffset]);

  const semanticRows = semanticSearch?.results ?? semanticList?.results ?? [];

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Память"
        subtitle="Операторский доступ ко всей Mem0 semantic memory и scoped KV. Сессия нужна только для workspace/agent/session scopes и recall preview."
        action={
          <Button variant="outlined" disabled={loading} onClick={() => void load()}>
            Обновить
          </Button>
        }
      />
      {error ? <Alert severity="error">{error}</Alert> : null}
      {!scopeAvailable ? (
        <Alert severity="info">
          Scope `{scope}` вычисляется от выбранной сессии. Выбери сессию или переключись на `operator`/`agent_shared`, чтобы смотреть глобальную
          память без привязки к чату.
        </Alert>
      ) : null}

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5} alignItems={{ xs: "stretch", md: "center" }}>
          <TextField select size="small" label="Scope" value={scope} onChange={(event) => setScope(event.target.value as MemoryScope)} sx={{ minWidth: 180 }}>
            {memoryScopes.map((item) => (
              <MenuItem key={item} value={item}>
                {item}
              </MenuItem>
            ))}
          </TextField>
          <Chip label={selectedSession ? `context session: ${short(selectedSession.id, 24)}` : "context session: not required"} variant="outlined" />
          {selectedSession ? <Chip label={`agent: ${selectedSession.agent_profile_id}`} variant="outlined" /> : null}
          <Chip label={`scope: ${scope}`} color="primary" variant="outlined" />
        </Stack>
      </Paper>

      <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
        <Metric label="Semantic" value={semanticRows.length} hint={semanticSearch ? "search results" : "listed memories"} />
        <Metric label="KV" value={kvList?.results.length ?? 0} hint={kvList?.truncated ? "page truncated" : "current page"} />
        <Metric label="Recall" value={recallPreview?.items.length ?? 0} hint={recallPreview?.enabled ? "items injected" : "disabled/empty"} />
      </Stack>

      <Paper variant="outlined">
        <Tabs value={tab} onChange={(_, value: MemoryTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
          <Tab value="recall" label="Recall preview" />
          <Tab value="semantic" label="Semantic memory" />
          <Tab value="kv" label="KV" />
          <Tab value="boundary" label="Что где хранить" />
        </Tabs>
      </Paper>

      {tab === "recall" && sessionId ? (
        <RecallPreviewPane
          query={recallQuery}
          preview={recallPreview}
          onQueryChange={setRecallQuery}
          onRefresh={() => void load()}
        />
      ) : null}
      {tab === "recall" && !sessionId ? (
        <EmptyState title="Recall preview требует сессию" detail="Memory recall показывает, что будет подмешано в prompt конкретного чата." />
      ) : null}
      {tab === "semantic" ? (
        <SemanticMemoryPane
          query={memoryQuery}
          rows={semanticRows}
          list={semanticList}
          search={semanticSearch}
          loading={loading || busy}
          onQueryChange={setMemoryQuery}
          onSearch={() => void searchMemory()}
          onClearSearch={() => {
            setMemoryQuery("");
            setSemanticSearch(null);
          }}
          onEdit={(item) => {
            setEditingMemory(item);
            setMemoryEditText(item.memory);
            setMemoryEditMetadata(JSON.stringify(item.metadata ?? null, null, 2));
          }}
          onDelete={(item) => void deleteMemory(item)}
          onPage={(offset) => setMemoryOffset(Math.max(0, offset))}
        />
      ) : null}
      {tab === "kv" ? (
        <KvPane
          kvList={kvList}
          prefix={kvPrefix}
          kvKey={kvKey}
          kvValue={kvValue}
          kvMetadata={kvMetadata}
          loading={loading || busy}
          onPrefixChange={setKvPrefix}
          onReload={() => {
            setKvOffset(0);
            void load();
          }}
          onKeyChange={setKvKey}
          onValueChange={setKvValue}
          onMetadataChange={setKvMetadata}
          onPut={() => void putKv()}
          onDelete={(entry) => void deleteKv(entry)}
          onPage={(offset) => setKvOffset(Math.max(0, offset))}
        />
      ) : null}
      {tab === "boundary" ? <MemoryBoundaryPane /> : null}

      <Dialog open={Boolean(editingMemory)} onClose={() => setEditingMemory(null)} fullWidth maxWidth="md">
        <DialogTitle>Редактировать semantic memory</DialogTitle>
        <DialogContent>
          <Stack spacing={1.5} sx={{ mt: 1 }}>
            <Typography variant="caption" className="mono" color="text.secondary">
              {editingMemory?.id}
            </Typography>
            <TextField
              label="Memory text"
              value={memoryEditText}
              onChange={(event) => setMemoryEditText(event.target.value)}
              multiline
              minRows={4}
            />
            <TextField
              label="Metadata JSON"
              value={memoryEditMetadata}
              onChange={(event) => setMemoryEditMetadata(event.target.value)}
              multiline
              minRows={5}
              className="mono"
            />
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setEditingMemory(null)}>Отмена</Button>
          <Button variant="contained" disabled={busy} onClick={() => void saveMemoryEdit()}>
            Сохранить
          </Button>
        </DialogActions>
      </Dialog>
    </Stack>
  );
}

function RecallPreviewPane({
  query,
  preview,
  onQueryChange,
  onRefresh
}: {
  query: string;
  preview: MemoryRecallPreview | null;
  onQueryChange: (value: string) => void;
  onRefresh: () => void;
}) {
  return (
    <Stack spacing={2}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
          <TextField
            fullWidth
            size="small"
            label="Query override"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Пусто = последний user message сессии"
          />
          <Button variant="contained" onClick={onRefresh}>
            Preview
          </Button>
        </Stack>
      </Paper>
      {preview?.items.length ? (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Scope</TableCell>
                <TableCell>Memory</TableCell>
                <TableCell>Score</TableCell>
                <TableCell>Source</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {preview.items.map((item) => (
                <TableRow key={`${item.scope}-${item.memory_id}`}>
                  <TableCell>
                    <Chip label={item.scope} size="small" variant="outlined" />
                  </TableCell>
                  <TableCell>
                    <Typography variant="body2">{item.memory}</Typography>
                    <Typography variant="caption" color="text.secondary" className="mono">
                      {item.memory_id}
                    </Typography>
                  </TableCell>
                  <TableCell>{item.score ?? "—"}</TableCell>
                  <TableCell>{item.source ?? "—"}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      ) : (
        <EmptyState
          title="Recall пуст"
          detail={preview?.enabled === false ? "Memory recall выключен или Mem0 недоступен." : "По текущему запросу ничего не найдено."}
        />
      )}
      <JsonBlock value={preview ?? { enabled: false }} />
    </Stack>
  );
}

function SemanticMemoryPane({
  query,
  rows,
  list,
  search,
  loading,
  onQueryChange,
  onSearch,
  onClearSearch,
  onEdit,
  onDelete,
  onPage
}: {
  query: string;
  rows: SemanticMemoryItem[];
  list: SemanticMemoryList | null;
  search: SemanticMemorySearch | null;
  loading: boolean;
  onQueryChange: (value: string) => void;
  onSearch: () => void;
  onClearSearch: () => void;
  onEdit: (item: SemanticMemoryItem) => void;
  onDelete: (item: SemanticMemoryItem) => void;
  onPage: (offset: number) => void;
}) {
  const offset = list?.offset ?? 0;
  const limit = list?.limit ?? 20;
  return (
    <Stack spacing={2}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
          <TextField
            fullWidth
            size="small"
            label="Semantic search"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Например: настройка GitLab"
          />
          <Button variant="contained" disabled={loading} onClick={onSearch}>
            Найти
          </Button>
          {search ? (
            <Button variant="outlined" onClick={onClearSearch}>
              Список
            </Button>
          ) : null}
        </Stack>
      </Paper>
      {rows.length ? (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Memory</TableCell>
                <TableCell>Score</TableCell>
                <TableCell>Scope ids</TableCell>
                <TableCell>Metadata</TableCell>
                <TableCell align="right">Действия</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {rows.map((item) => (
                <TableRow key={item.id} hover>
                  <TableCell sx={{ maxWidth: 520 }}>
                    <Typography variant="body2">{item.memory}</Typography>
                    <Typography variant="caption" color="text.secondary" className="mono">
                      {item.id}
                    </Typography>
                  </TableCell>
                  <TableCell>{item.score ?? "—"}</TableCell>
                  <TableCell className="mono">
                    <Typography variant="caption" component="div">
                      user={item.user_id ?? "—"}
                    </Typography>
                    <Typography variant="caption" component="div">
                      agent={item.agent_id ?? "—"}
                    </Typography>
                    <Typography variant="caption" component="div">
                      run={item.run_id ?? "—"}
                    </Typography>
                  </TableCell>
                  <TableCell className="mono">{jsonPreview(item.metadata)}</TableCell>
                  <TableCell align="right">
                    <Stack direction="row" spacing={1} justifyContent="flex-end">
                      <Button size="small" onClick={() => onEdit(item)}>
                        Edit
                      </Button>
                      <Button size="small" color="error" onClick={() => onDelete(item)}>
                        Delete
                      </Button>
                    </Stack>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      ) : (
        <EmptyState title="Semantic memory пуст" detail="Добавлять память может агент через memory_add или curator после turn." />
      )}
      {!search ? (
        <Stack direction="row" spacing={1}>
          <Button variant="outlined" disabled={offset === 0} onClick={() => onPage(offset - limit)}>
            Назад
          </Button>
          <Button variant="outlined" disabled={!list?.next_offset} onClick={() => onPage(list?.next_offset ?? offset + limit)}>
            Дальше
          </Button>
        </Stack>
      ) : null}
    </Stack>
  );
}

function KvPane({
  kvList,
  prefix,
  kvKey,
  kvValue,
  kvMetadata,
  loading,
  onPrefixChange,
  onReload,
  onKeyChange,
  onValueChange,
  onMetadataChange,
  onPut,
  onDelete,
  onPage
}: {
  kvList: KvList | null;
  prefix: string;
  kvKey: string;
  kvValue: string;
  kvMetadata: string;
  loading: boolean;
  onPrefixChange: (value: string) => void;
  onReload: () => void;
  onKeyChange: (value: string) => void;
  onValueChange: (value: string) => void;
  onMetadataChange: (value: string) => void;
  onPut: () => void;
  onDelete: (entry: KvEntry) => void;
  onPage: (offset: number) => void;
}) {
  const offset = kvList?.offset ?? 0;
  const limit = kvList?.limit ?? 50;
  return (
    <Stack spacing={2}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack spacing={1.5}>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
            <TextField fullWidth size="small" label="Prefix filter" value={prefix} onChange={(event) => onPrefixChange(event.target.value)} />
            <Button variant="contained" disabled={loading} onClick={onReload}>
              Применить
            </Button>
          </Stack>
          <Box sx={{ display: "grid", gridTemplateColumns: { xs: "1fr", md: "240px 1fr 1fr auto" }, gap: 1.5 }}>
            <TextField size="small" label="Key" value={kvKey} onChange={(event) => onKeyChange(event.target.value)} />
            <TextField size="small" label="Value JSON" value={kvValue} onChange={(event) => onValueChange(event.target.value)} />
            <TextField size="small" label="Metadata JSON" value={kvMetadata} onChange={(event) => onMetadataChange(event.target.value)} />
            <Button variant="outlined" disabled={loading || !kvKey.trim()} onClick={onPut}>
              Put
            </Button>
          </Box>
        </Stack>
      </Paper>
      {kvList?.results.length ? (
        <TableContainer component={Paper} variant="outlined">
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Key</TableCell>
                <TableCell>Value</TableCell>
                <TableCell>Revision</TableCell>
                <TableCell>Updated</TableCell>
                <TableCell>Namespace</TableCell>
                <TableCell align="right">Действия</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {kvList.results.map((entry) => (
                <TableRow key={`${entry.scope}-${entry.namespace_id}-${entry.key}`} hover>
                  <TableCell className="mono">{entry.key}</TableCell>
                  <TableCell className="mono">{jsonPreview(entry.value)}</TableCell>
                  <TableCell>{entry.revision}</TableCell>
                  <TableCell>{formatTime(entry.updated_at)}</TableCell>
                  <TableCell>
                    <Typography variant="caption" component="div">
                      {entry.scope}
                    </Typography>
                    <Typography variant="caption" color="text.secondary" className="mono">
                      {short(entry.namespace_id, 28)}
                    </Typography>
                  </TableCell>
                  <TableCell align="right">
                    <Button size="small" color="error" onClick={() => onDelete(entry)}>
                      Delete
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>
      ) : (
        <EmptyState title="KV пуст" detail="KV подходит для точных настроек, флагов, выбранных ресурсов и обмена между агентами." />
      )}
      <Stack direction="row" spacing={1}>
        <Button variant="outlined" disabled={offset === 0} onClick={() => onPage(offset - limit)}>
          Назад
        </Button>
        <Button variant="outlined" disabled={!kvList?.next_offset} onClick={() => onPage(kvList?.next_offset ?? offset + limit)}>
          Дальше
        </Button>
      </Stack>
    </Stack>
  );
}

function MemoryBoundaryPane() {
  return (
    <Stack spacing={2}>
      {(["mem0", "kv", "silverbullet"] as const).map((layer) => (
        <Paper key={layer} variant="outlined" sx={{ p: 2 }}>
          <Typography variant="subtitle1" fontWeight={800}>
            {layer}
          </Typography>
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            {describeMemoryLayer(layer)}
          </Typography>
        </Paper>
      ))}
      <Alert severity="info">
        Для прозрачности агент должен проговаривать действия: «ищу в памяти», «читаю KV», «пишу в SilverBullet». Этот экран показывает
        состояние слоёв, но не заменяет skill-инструкции агента.
      </Alert>
    </Stack>
  );
}
