export type SkillProfileFileEntry = {
  path: string;
  kind: string;
  byte_len: number;
};

export function skillPath(name: string): string {
  return `skills/${name.trim()}/SKILL.md`;
}

export function skillSkeleton(name: string): string {
  const normalizedName = name.trim();
  return `---\nname: ${normalizedName}\ndescription: Коротко опиши, когда агент должен использовать этот skill.\n---\n\n# ${normalizedName}\n\n## Когда использовать\n\nИспользуй этот skill, когда ...\n\n## Порядок работы\n\n1. Определи входные данные.\n2. Выполни действия через канонические tools.\n3. Кратко сообщи результат оператору.\n`;
}

export function isValidSkillName(value: string): boolean {
  return /^[a-zA-Z0-9._-]+$/.test(value.trim());
}

export function skillNameFromPath(path: string): string | null {
  const match = /^skills\/([^/]+)\//.exec(path);
  return match?.[1] ?? null;
}

export function getPromptProfileFiles(files: SkillProfileFileEntry[]): SkillProfileFileEntry[] {
  const promptOrder = new Map([
    ["SYSTEM.md", 0],
    ["AGENTS.md", 1]
  ]);
  return files
    .filter((file) => promptOrder.has(file.path))
    .sort((left, right) => {
      return (promptOrder.get(left.path) ?? 99) - (promptOrder.get(right.path) ?? 99);
    });
}

export function getSkillProfileFiles(files: SkillProfileFileEntry[]): SkillProfileFileEntry[] {
  return files
    .filter((file) => skillNameFromPath(file.path))
    .sort((left, right) => {
      const leftName = skillNameFromPath(left.path) ?? left.path;
      const rightName = skillNameFromPath(right.path) ?? right.path;
      const byName = leftName.localeCompare(rightName, "ru");
      if (byName !== 0) {
        return byName;
      }
      if (left.path.endsWith("/SKILL.md") && !right.path.endsWith("/SKILL.md")) {
        return -1;
      }
      if (!left.path.endsWith("/SKILL.md") && right.path.endsWith("/SKILL.md")) {
        return 1;
      }
      const leftDepth = left.path.split("/").length;
      const rightDepth = right.path.split("/").length;
      if (leftDepth !== rightDepth) {
        return leftDepth - rightDepth;
      }
      return left.path.localeCompare(right.path, "ru");
    });
}
