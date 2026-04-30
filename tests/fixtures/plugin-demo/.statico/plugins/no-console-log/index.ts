import { Plugin, Issue } from "../../../../../../sdks/typescript/src/index";

const plugin = Plugin.create("no-console-log", {
  hooks: { analyze_file: "add" },
  languages: ["typescript"],
  rules: [
    { id: "no-console-log", severity: "warning", description: "Detect console.log statements" },
  ],
});

plugin.onAnalyzeFile((params) => {
  const issues: Issue[] = [];
  const lines = params.source.split("\n");

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    // Simple detection — skip comments
    const trimmed = line.trim();
    if (trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*")) {
      continue;
    }
    if (line.includes("console.log")) {
      issues.push({
        ruleId: "no-console-log",
        severity: "warning",
        message: `Found console.log — consider using a proper logger`,
        file: params.path,
        line: i + 1,
        column: line.indexOf("console.log") + 1,
        confidence: 0.95,
        suggestion: "Replace with a structured logger or remove",
      });
    }
  }

  return { issues };
});

plugin.start();
