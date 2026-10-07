import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

function rule(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = styles.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`));
  assert.ok(match, `Missing CSS rule for ${selector}`);
  return match[1].replace(/\s+/g, "");
}

test("all five voice names have readable non-ellipsized controls", () => {
  const picker = rule(".voice-picker");
  const columns = picker.match(/grid-template-columns:repeat\((\d+),minmax\((\d+)px,1fr\)\)/);
  assert.ok(columns, "Voice picker must define bounded responsive columns");
  assert.ok(Number(columns[1]) <= 3, "Five narrow columns truncate voice names");
  assert.ok(Number(columns[2]) >= 130, "Voice controls need at least 130px for name and preview");

  const label = rule(".voice-option > button:first-child span");
  assert.match(label, /white-space:nowrap/);
  assert.doesNotMatch(label, /text-overflow:ellipsis/);
});
