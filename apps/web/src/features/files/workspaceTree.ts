export type WorkspaceTreeEntry = {
  path: string;
  kind: "file" | "directory" | string;
  bytes?: number | null;
};

export type WorkspaceTreeNode = WorkspaceTreeEntry & {
  label: string;
  depth: number;
};

function basename(path: string): string {
  const normalized = path.replace(/\/+$/, "");
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(index + 1) : normalized;
}

function depth(path: string): number {
  const normalized = path.replace(/^\/+|\/+$/g, "");
  if (!normalized) {
    return 0;
  }
  return normalized.split("/").length - 1;
}

export function joinWorkspacePath(parent: string, child: string): string {
  const left = parent.replace(/^\/+|\/+$/g, "");
  const right = child.replace(/^\/+|\/+$/g, "");
  if (!left) {
    return right;
  }
  if (!right) {
    return left;
  }
  return `${left}/${right}`;
}

export function getParentPath(path: string): string {
  const normalized = path.replace(/^\/+|\/+$/g, "");
  const index = normalized.lastIndexOf("/");
  return index > 0 ? normalized.slice(0, index) : "";
}

export function buildWorkspaceTreeNodes(entries: WorkspaceTreeEntry[]): WorkspaceTreeNode[] {
  return [...entries]
    .sort((left, right) => {
      if (left.kind === "directory" && right.kind !== "directory") {
        return -1;
      }
      if (left.kind !== "directory" && right.kind === "directory") {
        return 1;
      }
      return basename(left.path).localeCompare(basename(right.path), "ru");
    })
    .map((entry) => ({
      ...entry,
      label: basename(entry.path) || entry.path || ".",
      depth: depth(entry.path)
    }));
}
