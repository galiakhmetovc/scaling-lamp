type ToolLogEntry = {
  name: string;
  phase: string;
  arguments: Record<string, unknown>;
  result_text?: string;
  error_text?: string;
};

export function reverseToolLog(entries: ToolLogEntry[]): ToolLogEntry[] {
  return entries.slice().reverse();
}
