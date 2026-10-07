import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";

const source = await readFile(new URL("../src/Overlay.tsx", import.meta.url), "utf8");
const parsed = ts.createSourceFile(
  "Overlay.tsx",
  source,
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TSX,
);
let assistantBranch;
function visit(node) {
  if (ts.isConditionalExpression(node) && node.condition.getText(parsed) === "assistantActive") {
    assistantBranch = node.whenTrue;
  }
  ts.forEachChild(node, visit);
}
visit(parsed);
assert.ok(assistantBranch, "Assistant overlay render branch is missing");

const compiled = ts.transpileModule(
  `const render = () => (${assistantBranch.getText(parsed)});`,
  {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.None,
      jsx: ts.JsxEmit.React,
    },
  },
).outputText;

function renderAssistantNotice(notice) {
  const Icon = (props) => React.createElement("svg", props);
  const scope = {
    card: { current: null },
    assistantPhase: "closing",
    language: "bg",
    beginOverlayDrag() {},
    assistantNoteTranscript: { active: false, text: "" },
    assistantStatus: "Приключвам разговора…",
    GripHorizontal: Icon,
    LoaderCircle: Icon,
    Square: Icon,
    stopAssistant() {},
    notice,
    errorMessage: (message) => message,
  };
  const render = new Function(
    "React",
    "scope",
    `with (scope) { ${compiled}; return render; }`,
  )(React, scope);
  return renderToStaticMarkup(render());
}

test("closing assistant overlay renders the unsaved-note warning", () => {
  const html = renderAssistantNotice("Забележката не е записана.");
  assert.match(html, /class="notice-text"/u);
  assert.match(html, /Забележката не е записана\./u);
});

test("assistant overlay does not invent a warning without a notice", () => {
  const html = renderAssistantNotice(null);
  assert.doesNotMatch(html, /class="notice-text"/u);
});
