import type { SettingsFieldState, SettingsRawFileContent, SettingsSnapshot } from "../lib/types";

export function detectSettingsDirty(
  settings: SettingsSnapshot | null,
  draft: Record<string, unknown>,
  rawFile: SettingsRawFileContent | null,
  rawDraft: string,
): { formDirty: boolean; rawDirty: boolean } {
  const formDirty = Boolean(settings?.form_fields.some((field) => draft[field.key] !== field.value));
  const rawDirty = Boolean(rawFile && rawDraft !== rawFile.content);
  return { formDirty, rawDirty };
}

export function isRevisionConflict(error: string): boolean {
  return error.toLowerCase().includes("revision conflict");
}

export function partitionSettingsFields(settings: SettingsSnapshot | null): {
  quickControls: SettingsFieldState[];
  advancedFields: SettingsFieldState[];
} {
  if (!settings) {
    return { quickControls: [], advancedFields: [] };
  }

  const quickControlKeys = new Set(settings.quick_controls.map((field) => field.key));
  return {
    quickControls: settings.quick_controls,
    advancedFields: settings.form_fields.filter((field) => !quickControlKeys.has(field.key)),
  };
}
