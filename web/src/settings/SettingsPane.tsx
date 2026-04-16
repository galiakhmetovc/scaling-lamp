import type { SettingsRawFileContent, SettingsSnapshot } from "../lib/types";
import { detectSettingsDirty, isRevisionConflict, partitionSettingsFields } from "./model";

type SettingsPaneProps = {
  settings: SettingsSnapshot | null;
  draft: Record<string, unknown>;
  rawFile: SettingsRawFileContent | null;
  selectedRawPath: string;
  rawDraft: string;
  error: string;
  onDraftChange: React.Dispatch<React.SetStateAction<Record<string, unknown>>>;
  onApply: () => void;
  onSelectRaw: (path: string) => void;
  onRawChange: (content: string) => void;
  onApplyRaw: () => void;
};

export function SettingsPane(props: SettingsPaneProps) {
  const { settings, draft, rawFile, selectedRawPath, rawDraft, error, onDraftChange, onApply, onSelectRaw, onRawChange, onApplyRaw } = props;
  const dirty = detectSettingsDirty(settings, draft, rawFile, rawDraft);
  const conflict = error && isRevisionConflict(error);
  const { quickControls, advancedFields } = partitionSettingsFields(settings);

  return (
    <div className="two-column">
      <section className="surface surface-primary">
        <div className="section-title">
          <span>Settings form</span>
          <span className="muted">{settings?.revision ?? "-"}</span>
        </div>
        <div className="status-chip-row">
          <span>{dirty.formDirty ? "form dirty" : "form clean"}</span>
          <span>{dirty.rawDirty ? "raw dirty" : "raw clean"}</span>
        </div>
        {error ? <div className={conflict ? "error-banner conflict" : "error-banner"}>{error}</div> : null}
        {quickControls.length > 0 ? (
          <div className="detail-card compact">
            <div className="section-title">
              <strong>Chat quick controls</strong>
              <span className="muted">{quickControls.length}</span>
            </div>
            <div className="form-grid">
              {quickControls.map((field) => (
                <SettingsFieldInput key={field.key} field={field} draft={draft} onDraftChange={onDraftChange} />
              ))}
            </div>
          </div>
        ) : null}
        {advancedFields.length > 0 ? (
          <div className="detail-card compact">
            <div className="section-title">
              <strong>Advanced settings</strong>
              <span className="muted">{advancedFields.length}</span>
            </div>
            <div className="form-grid">
              {advancedFields.map((field) => (
                <SettingsFieldInput key={field.key} field={field} draft={draft} onDraftChange={onDraftChange} />
              ))}
            </div>
          </div>
        ) : null}
        <button onClick={onApply}>Apply settings</button>
      </section>

      <section className="surface surface-secondary">
        <div className="section-title">
          <span>Raw YAML</span>
          <span className="muted">{selectedRawPath || "no file selected"}</span>
        </div>
        <select value={selectedRawPath} onChange={(event) => onSelectRaw(event.target.value)}>
          <option value="">Select raw file</option>
          {(settings?.raw_files ?? []).map((file) => (
            <option key={file.path} value={file.path}>
              {file.path}
            </option>
          ))}
        </select>
        {rawFile ? <div className="muted">{`rev ${rawFile.revision}`}</div> : null}
        <textarea value={rawDraft} onChange={(event) => onRawChange(event.target.value)} placeholder="Raw YAML content" />
        <button onClick={onApplyRaw} disabled={!rawFile}>Apply raw file</button>
      </section>
    </div>
  );
}

function SettingsFieldInput(props: {
  field: NonNullable<SettingsPaneProps["settings"]>["form_fields"][number];
  draft: Record<string, unknown>;
  onDraftChange: React.Dispatch<React.SetStateAction<Record<string, unknown>>>;
}) {
  const { field, draft, onDraftChange } = props;

  return (
    <label>
      <span>{field.label}</span>
      {field.type === "bool" ? (
        <input
          type="checkbox"
          checked={Boolean(draft[field.key])}
          onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.checked }))}
        />
      ) : field.type === "int" ? (
        <input
          type="number"
          value={String(draft[field.key] ?? "")}
          onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: Number.parseInt(event.target.value || "0", 10) }))}
        />
      ) : field.enum && field.enum.length > 0 ? (
        <select
          value={String(draft[field.key] ?? "")}
          onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.value }))}
        >
          {field.enum.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      ) : (
        <input
          value={String(draft[field.key] ?? "")}
          onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.value }))}
        />
      )}
    </label>
  );
}
