export type ParsedToolDebugDetail = {
  meta: Map<string, string>;
  arguments: string | null;
  result: Map<string, string>;
  resultPreview: string | null;
  raw: string;
};

type SectionName = "meta" | "arguments" | "result" | "result_preview";

function parseKeyValue(line: string): [string, string] | null {
  const separator = line.indexOf(":");
  if (separator <= 0) {
    return null;
  }
  return [line.slice(0, separator).trim(), line.slice(separator + 1).trim()];
}

export function parseToolDebugDetail(detail?: string | null): ParsedToolDebugDetail {
  const raw = detail ?? "";
  const meta = new Map<string, string>();
  const result = new Map<string, string>();
  const argumentsLines: string[] = [];
  const resultPreviewLines: string[] = [];
  let section: SectionName = "meta";

  for (const line of raw.split("\n")) {
    const marker = line.trim();
    if (marker === "arguments:") {
      section = "arguments";
      continue;
    }
    if (marker === "result:") {
      section = "result";
      continue;
    }
    if (marker === "result_preview:") {
      section = "result_preview";
      continue;
    }

    if (section === "arguments") {
      argumentsLines.push(line);
      continue;
    }
    if (section === "result_preview") {
      resultPreviewLines.push(line);
      continue;
    }
    if (section === "result") {
      const parsed = parseKeyValue(line);
      if (parsed) {
        result.set(parsed[0], parsed[1]);
      }
      continue;
    }

    const parsed = parseKeyValue(line);
    if (parsed) {
      meta.set(parsed[0], parsed[1]);
    }
  }

  return {
    meta,
    arguments: argumentsLines.join("\n").trim() || null,
    result,
    resultPreview: resultPreviewLines.join("\n").trim() || null,
    raw
  };
}
