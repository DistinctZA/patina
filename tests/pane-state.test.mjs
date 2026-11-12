import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, test } from "node:test";
import { pathToFileURL } from "node:url";
import { compileModule } from "svelte/compiler";
import ts from "typescript";

// Compile the real Svelte state with the installed compiler. Only the IPC
// boundary is mocked; selection, navigation and async ordering are unchanged.
const scratch = mkdtempSync(join(tmpdir(), "patina-ui-tests-"));
after(() => rmSync(scratch, { recursive: true, force: true }));
const read = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
const transpile = (source) => ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext },
}).outputText;
const resolvePackages = (source) => source.replace(/from (['"])(@[^'"]+|svelte\/[^'"]+)\1/g,
  (_, _quote, specifier) => `from ${JSON.stringify(import.meta.resolve(specifier))}`);
writeFileSync(join(scratch, "api.mjs"), resolvePackages(transpile(read("src/lib/api.ts"))));
const paneCode = compileModule(transpile(read("src/lib/pane-state.svelte.ts")), {
  filename: "pane-state.svelte.js", generate: "client",
}).js.code;
writeFileSync(join(scratch, "pane.mjs"), resolvePackages(paneCode).replace('"./api"', '"./api.mjs"'));
const { api } = await import(pathToFileURL(join(scratch, "api.mjs")));
const { PaneState } = await import(pathToFileURL(join(scratch, "pane.mjs")));
api.watchDir = async () => {};

// Exercise the page's actual Rename command without mounting a webview.
const pageScript = read("src/routes/+page.svelte").split('<script lang="ts">')[1].split("</script>")[0];
const parsed = ts.createSourceFile("page.ts", pageScript, ts.ScriptTarget.Latest, true);
const rename = parsed.statements.find((s) => ts.isFunctionDeclaration(s) && s.name?.text === "rename");
assert.ok(rename, "page contains the Rename command");
const renamePrompt = new Function("active", "api", `
  let batch = null, prompt = null;
  ${transpile(rename.getText(parsed))}
  rename();
  return { batch, prompt };
`);

const entry = (name) => ({ name, path: `/fixture/${name}`, isDir: false, isSymlink: false,
  isHidden: false, size: 1, modified: null, tags: [] });
const listing = (path, entries = []) => ({ path, parent: path.slice(0, path.lastIndexOf("/")) || "/",
  entries, available: null, device: 1 });
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const missing = "The item couldn't be found; it may have been moved or deleted";

// Exercise the actual page shortcut and selection-to-IPC path. DOM stand-ins
// distinguish editable fields and native controls from the file pane.
class Element {
  constructor(tag = "DIV", isContentEditable = false) {
    this.tagName = tag;
    this.isContentEditable = isContentEditable;
  }
  closest(selectors) {
    return selectors.split(", ").includes(this.tagName.toLowerCase()) ? this : null;
  }
}
class Input extends Element {
  constructor(type = "text") { super("INPUT"); this.type = type; }
}
const pageFunction = (name) => {
  const fn = parsed.statements.find((s) => ts.isFunctionDeclaration(s) && s.name?.text === name);
  assert.ok(fn, `page contains ${name}`);
  return transpile(fn.getText(parsed));
};
const keyboard = new Function("active", "api", "run", "HTMLElement", "HTMLInputElement", `
  const window = { innerHeight: 900 };
  let prompt = null, batch = null, menu = null, typed = "", typedTimer;
  ${pageFunction("quickLook")}
  ${pageFunction("onKey")}
  return onKey;
`);
function pressSpace(pane, overrides = {}) {
  const calls = [];
  const onKey = keyboard(pane, { quickLook: (...args) => calls.push(args) }, (task) => task(), Element, Input);
  const event = { key: " ", code: "Space", target: new Element(), metaKey: false,
    ctrlKey: false, altKey: false, shiftKey: false, repeat: false, isComposing: false,
    defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...overrides };
  onKey(event);
  return { calls, prevented: event.defaultPrevented };
}

test("Space previews the selection, including when the cursor was Command-deselected", () => {
  const pane = new PaneState("left");
  pane.all = [entry("A photo.png"), entry("B.pdf")];
  pane.click(0, { metaKey: false, shiftKey: false });
  pane.click(1, { metaKey: true, shiftKey: false });
  assert.deepEqual(pressSpace(pane).calls, [[["/fixture/A photo.png", "/fixture/B.pdf"], 1]]);
  pane.click(1, { metaKey: true, shiftKey: false });
  assert.deepEqual(pressSpace(pane), { calls: [[["/fixture/A photo.png"], 0]], prevented: true });
});

test("Space uses the cursor without a selection and safely handles an empty folder", () => {
  const pane = new PaneState("left");
  assert.deepEqual(pressSpace(pane), { calls: [], prevented: true });
  pane.all = [entry("写真.png")];
  assert.deepEqual(pressSpace(pane).calls, [[["/fixture/写真.png"], 0]]);
  assert.deepEqual(pressSpace(pane, { repeat: true }), { calls: [], prevented: true });
});

test("Space remains available to text fields, native controls and composition", () => {
  const pane = new PaneState("left");
  pane.all = [entry("A.txt")];
  for (const target of [new Input(), new Input("search"), new Input("checkbox"),
    new Element("TEXTAREA"), new Element("SELECT"), new Element("BUTTON"), new Element("DIV", true)]) {
    assert.deepEqual(pressSpace(pane, { target }), { calls: [], prevented: false });
  }
  for (const override of [{ metaKey: true }, { altKey: true }, { ctrlKey: true }, { isComposing: true }]) {
    assert.deepEqual(pressSpace(pane, override), { calls: [], prevented: false });
  }
  assert.deepEqual(pressSpace(pane, { defaultPrevented: true }), { calls: [], prevented: true });
});

test("Rename uses the remaining selection after Command-deselecting another file", async () => {
  const pane = new PaneState("left");
  pane.path = "/fixture";
  pane.all = [entry("A.txt"), entry("B.txt")];
  pane.click(0, { metaKey: false, shiftKey: false });
  pane.click(1, { metaKey: true, shiftKey: false });
  pane.click(1, { metaKey: true, shiftKey: false });
  const { prompt, batch } = renamePrompt(pane, api);
  assert.equal(batch, null);
  assert.equal(prompt.value, "A.txt");
  let renamed;
  api.rename = async (path, name) => { renamed = { path, name }; return `/fixture/${name}`; };
  api.listDir = async (path) => listing(path, [entry("New.txt"), entry("B.txt")]);
  await prompt.submit("New.txt");
  assert.deepEqual(renamed, { path: "/fixture/A.txt", name: "New.txt" });
});

test("Rename retains the cursor fallback and multiple-selection batch flow", () => {
  const pane = new PaneState("left");
  pane.all = [entry("A.txt"), entry("B.txt")];
  assert.equal(renamePrompt(pane, api).prompt.value, "A.txt");
  pane.selectAll();
  const result = renamePrompt(pane, api);
  assert.equal(result.prompt, null);
  assert.equal(result.batch.entries.length, 2);
});

test("a stale refresh failure cannot undo a completed navigation", async () => {
  const pane = new PaneState("left");
  pane.path = "/old";
  const old = deferred();
  const calls = [];
  api.listDir = (path) => { calls.push(path); return path === "/old" ? old.promise : Promise.resolve(listing(path)); };
  const refreshing = pane.refresh();
  await pane.load("/new/deep");
  old.reject(missing);
  await refreshing;
  assert.equal(pane.path, "/new/deep");
  assert.deepEqual(calls, ["/old", "/new/deep"]);
});

test("a stale failure cannot interrupt a navigation that is still loading", async () => {
  const pane = new PaneState("left");
  pane.path = "/old";
  const old = deferred(), next = deferred();
  api.listDir = (path) => path === "/old" ? old.promise : next.promise;
  const refreshing = pane.refresh();
  const navigating = pane.load("/new/deep");
  old.reject(missing);
  await refreshing;
  assert.equal(pane.loading, true);
  next.resolve(listing("/new/deep"));
  assert.equal(await navigating, true);
  assert.equal(pane.path, "/new/deep");
});

test("a current missing folder still recovers to its nearest existing parent", async () => {
  const pane = new PaneState("left");
  pane.path = "/parent/gone";
  api.listDir = (path) => path === "/parent/gone" ? Promise.reject(missing) : Promise.resolve(listing(path));
  await pane.refresh();
  assert.equal(pane.path, "/parent");
});

test("navigation supersedes a failed ancestor lookup during recovery", async () => {
  const pane = new PaneState("left");
  pane.path = "/parent/gone";
  const parent = deferred(), recovering = deferred();
  api.listDir = (path) => {
    if (path === "/parent/gone") return Promise.reject(missing);
    if (path === "/parent") { recovering.resolve(); return parent.promise; }
    return Promise.resolve(listing(path));
  };
  const refreshing = pane.refresh();
  await recovering.promise;
  await pane.load("/new/deep");
  parent.reject(missing);
  await refreshing;
  assert.equal(pane.path, "/new/deep");
});
