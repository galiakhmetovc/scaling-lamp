import type { SettingsRawFileContent, SettingsSnapshot } from "../lib/types";

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
