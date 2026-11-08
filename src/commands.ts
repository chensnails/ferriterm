// Template helpers for the quick command library and the command history filter.

const VARIABLE_PATTERN = /\{\{\s*([A-Za-z_][\w.-]*)\s*\}\}/g;

// Commands that may carry secrets are kept out of history and the execution log.
const SENSITIVE_PATTERN =
  /(password|passwd|passphrase|secret|token|api[-_]?key|private[-_]?key|access[-_]?key)/i;
const PASSWORD_FLAG = /(?:^|\s)(?:mysql|mariadb|mysqldump|psql|redis-cli)\b[^\n]*\s-p\S+/i;

export function templateVariables(template: string): string[] {
  const names: string[] = [];
  for (const match of template.matchAll(VARIABLE_PATTERN)) {
    if (!names.includes(match[1])) names.push(match[1]);
  }
  return names;
}

export function renderTemplate(
  template: string,
  values: Record<string, string>,
): string {
  return template.replace(VARIABLE_PATTERN, (_, name: string) =>
    (values[name] ?? "").trim(),
  );
}

export function isSensitiveCommand(line: string): boolean {
  return SENSITIVE_PATTERN.test(line) || PASSWORD_FLAG.test(line);
}
