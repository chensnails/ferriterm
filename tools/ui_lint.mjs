import fs from "node:fs";
import path from "node:path";

/**
 * Static checks for the design baseline. Views must not re-declare sizes or
 * colours that belong to tokens, and every ui-* class in the kit/views must
 * have a stylesheet rule. Add a trailing `ui-lint-allow` comment for a
 * deliberate exception on a single line.
 */

const root = process.cwd();
const componentDir = path.join(root, "src", "components");
const css = fs.readFileSync(path.join(root, "src", "style.css"), "utf8");
const sources = fs
  .readdirSync(componentDir)
  .filter((file) => file.endsWith(".tsx") && file !== "icons.tsx")
  .map((file) => ({
    file,
    text: fs.readFileSync(path.join(componentDir, file), "utf8"),
  }));

const problems = [];
const hardCodedPx =
  /\b(?:fontSize|borderRadius|width|height|padding|margin|gap)\s*:\s*["'`]?\d+(?:\.\d+)?px/;
const hardCodedColour = /#[0-9a-fA-F]{3,8}\b/;

for (const { file, text } of sources) {
  text.split(/\r?\n/).forEach((line, index) => {
    const trimmed = line.trim();
    if (
      line.includes("ui-lint-allow") ||
      trimmed.startsWith("*") ||
      trimmed.startsWith("//") ||
      trimmed.startsWith("/*")
    ) {
      return;
    }
    const where = `${file}:${index + 1}`;
    if (hardCodedPx.test(line)) {
      problems.push(`${where}: hard-coded px dimension, use a design token`);
    }
    if (hardCodedColour.test(line)) {
      problems.push(`${where}: hard-coded colour, use a design token`);
    }
  });
}

const usedClasses = new Set();
for (const { text } of sources) {
  for (const match of text.matchAll(/["'`](ui-[a-z-]+)["'`]/g)) {
    usedClasses.add(match[1]);
  }
}
for (const className of usedClasses) {
  if (!css.includes(`.${className}`)) {
    problems.push(`.${className}: used in components but missing from style.css`);
  }
}

const requiredTokens = [
  "--island-bg",
  "--island-gap",
  "--island-pad",
  "--island-radius",
  "--hover-bg",
  "--selected-bg",
  "--menu-sel-bg",
  "--danger-bg",
  "--divider",
  "--sub-bg",
  "--panel-header-h",
  "--status-bar-h",
  "--activity-rail-w",
  "--tool-strip-w",
];
for (const token of requiredTokens) {
  if (!css.includes(`${token}:`)) {
    problems.push(`${token}: missing design baseline token`);
  }
}

if (problems.length > 0) {
  console.error(`ui-lint: ${problems.length} problem(s)`);
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}

console.log(
  `ui-lint: OK (${sources.length} components, ${usedClasses.size} ui-* classes, ${requiredTokens.length} baseline tokens)`,
);
