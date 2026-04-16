import type { SettingsRawFileContent, SettingsSnapshot } from "../lib/types";
import { detectSettingsDirty, isRevisionConflict } from "./model";

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
        <div className="form-grid">
          {(settings?.form_fields ?? []).map((field) => (
            <label key={field.key}>
              <span>{field.label}</span>
              {field.type === "bool" ? (
                <input
                  type="checkbox"
                  checked={Boolean(draft[field.key])}
                  onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.checked }))}
                />
              ) : (
                <input
                  value={String(draft[field.key] ?? "")}
                  onChange={(event) => onDraftChange((current) => ({ ...current, [field.key]: event.target.value }))}
                />
              )}
            </label>
          ))}
        </div>
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
