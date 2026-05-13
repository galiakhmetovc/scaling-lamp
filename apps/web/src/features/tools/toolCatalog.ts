import type { ToolCatalogItem } from "../../types";

const FAMILY_ORDER = [
  "fs",
  "web",
  "browser",
  "exec",
  "plan",
  "offload",
  "memory",
  "mcp",
  "agent"
];

export type ToolCatalogFamilyGroup = {
  family: string;
  tools: ToolCatalogItem[];
};

export type ToolCatalogStats = {
  total: number;
  builtIn: number;
  mcp: number;
  destructive: number;
  unavailable: number;
};

function familyRank(family: string): number {
  const index = FAMILY_ORDER.indexOf(family);
  return index >= 0 ? index : FAMILY_ORDER.length;
}

export function groupToolCatalogByFamily(tools: ToolCatalogItem[]): ToolCatalogFamilyGroup[] {
  const groups = new Map<string, ToolCatalogItem[]>();
  for (const tool of tools) {
    const group = groups.get(tool.family) ?? [];
    group.push(tool);
    groups.set(tool.family, group);
  }

  return [...groups.entries()]
    .sort(([left], [right]) => familyRank(left) - familyRank(right) || left.localeCompare(right))
    .map(([family, groupTools]) => ({
      family,
      tools: [...groupTools].sort((left, right) => left.id.localeCompare(right.id))
    }));
}

export function summarizeToolCatalog(tools: ToolCatalogItem[]): ToolCatalogStats {
  return {
    total: tools.length,
    builtIn: tools.filter((tool) => tool.origin === "built_in").length,
    mcp: tools.filter((tool) => tool.origin === "mcp").length,
    destructive: tools.filter((tool) => tool.destructive).length,
    unavailable: tools.filter((tool) => !tool.available).length
  };
}

export function agentAllowsTool(allowedTools: string[], toolId: string): boolean {
  return allowedTools.includes(toolId) || (toolId.startsWith("mcp__") && (allowedTools.includes("mcp") || allowedTools.includes("mcp_call")));
}
