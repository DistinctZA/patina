<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, basename } from "$lib/api";
  import { PaneState } from "$lib/pane-state.svelte";
  import Pane from "$lib/Pane.svelte";

  const left = new PaneState("left");
  const right = new PaneState("right");
  const panes = { left, right };

  let activeId = $state<"left" | "right">("left");
  const active = $derived(panes[activeId]);
  const other = $derived(activeId === "left" ? right : left);

  type Prompt = { title: string; action: string; value: string; submit: (value: string) => Promise<void> };
  let prompt = $state<Prompt | null>(null);
  let toast = $state<string | null>(null);
  let busy = $state(false);
  let toastTimer: ReturnType<typeof setTimeout>;

  function showError(err: unknown) {
    toast = String(err);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 5000);
  }

  async function run(task: () => Promise<unknown>) {
    busy = true;
    try {
      await task();
    } catch (err) {
      showError(err);
    } finally {
      busy = false;
    }
  }

  async function refreshBoth() {
    await Promise.all([left.refresh(), right.refresh()]);
  }

  // ── Commands ───────────────────────────────────────────────────────────

  function copyToOther() {
    const paths = active.targets().map((e) => e.path);
    if (paths.length) run(() => api.copyInto(paths, other.path).then(refreshBoth));
  }

  function moveToOther() {
    const paths = active.targets().map((e) => e.path);
    if (paths.length) run(() => api.moveInto(paths, other.path).then(refreshBoth));
  }

  function trash() {
    const paths = active.targets().map((e) => e.path);
    if (paths.length) run(() => api.moveToTrash(paths).then(refreshBoth));
  }

  function newFolder() {
    const pane = active;
    prompt = {
      title: "New folder",
      action: "Create",
      value: "untitled folder",
      submit: async (name) => {
        const created = await api.createFolder(pane.path, name);
        await pane.load(pane.path, { focus: created });
        pane.moveCursor(pane.cursor);
      },
    };
  }

  function rename() {
    const pane = active;
    const entry = pane.current;
    if (!entry) return;
    prompt = {
      title: `Rename “${entry.name}”`,
      action: "Rename",
      value: entry.name,
      submit: async (name) => {
        const renamed = await api.rename(entry.path, name);
        await pane.load(pane.path, { focus: renamed });
        pane.moveCursor(pane.cursor);
      },
    };
  }

  async function submitPrompt(e: SubmitEvent) {
    e.preventDefault();
    if (!prompt) return;
    const { submit, value } = prompt;
    try {
      await submit(value);
      prompt = null;
    } catch (err) {
      showError(err);
    }
  }

  /** Focuses the dialog input and selects the name without its extension. */
  function selectName(input: HTMLInputElement) {
    tick().then(() => {
      input.focus();
      const dot = input.value.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : input.value.length);
    });
  }

  // ── Keyboard ───────────────────────────────────────────────────────────

  let typed = "";
  let typedTimer: ReturnType<typeof setTimeout>;

  function onKey(e: KeyboardEvent) {
    if (prompt) {
      if (e.key === "Escape") prompt = null;
      return;
    }
    const pane = active;
    const page = Math.max(1, Math.floor((window.innerHeight - 160) / 24) - 1);
    const cmd = e.metaKey;

    const handled = (() => {
      switch (true) {
        case e.key === "Tab":
          activeId = activeId === "left" ? "right" : "left";
          return true;
        case e.key === "ArrowDown" && cmd:
        case e.key === "Enter":
          run(async () => pane.open());
          return true;
        case e.key === "ArrowUp" && cmd:
        case e.key === "Backspace" && !cmd:
          run(async () => pane.goUp());
          return true;
        case e.key === "[" && cmd:
          run(async () => pane.goBack());
          return true;
        case e.key === "]" && cmd:
          run(async () => pane.goForward());
          return true;
        case e.key === "ArrowDown":
          pane.moveCursor(pane.cursor + 1, e.shiftKey);
          return true;
        case e.key === "ArrowUp":
          pane.moveCursor(pane.cursor - 1, e.shiftKey);
          return true;
        case e.key === "PageDown":
          pane.moveCursor(pane.cursor + page, e.shiftKey);
          return true;
        case e.key === "PageUp":
          pane.moveCursor(pane.cursor - page, e.shiftKey);
          return true;
        case e.key === "Home":
          pane.moveCursor(0, e.shiftKey);
          return true;
        case e.key === "End":
          pane.moveCursor(pane.entries.length - 1, e.shiftKey);
          return true;
        case e.key === "F5":
          copyToOther();
          return true;
        case e.key === "F6":
          moveToOther();
          return true;
        case e.key === "F2":
          rename();
          return true;
        case e.key === "F7":
        case e.key.toLowerCase() === "n" && cmd && e.shiftKey:
          newFolder();
          return true;
        case e.key === "F8":
        case e.key === "Backspace" && cmd:
          trash();
          return true;
        // Shift and Option change e.key on a Mac (⇧. is ">", ⌥R is "®"), so
        // these match the physical key instead.
        case e.code === "Period" && cmd && e.shiftKey:
          run(() => pane.toggleHidden());
          return true;
        case e.code === "KeyR" && cmd && e.altKey:
          if (pane.current) run(() => api.revealInFinder(pane.current!.path));
          return true;
        case e.code === "KeyR" && cmd:
          run(refreshBoth);
          return true;
        case e.key === "a" && cmd:
          pane.selectAll();
          return true;
        case e.key === "Escape":
          pane.clearSelection();
          return true;
        case e.key.length === 1 && !cmd && !e.ctrlKey && !e.altKey:
          // Type the start of a name to jump to it.
          clearTimeout(typedTimer);
          typed += e.key;
          typedTimer = setTimeout(() => (typed = ""), 800);
          pane.jumpTo(typed);
          return true;
        default:
          return false;
      }
    })();

    if (handled) e.preventDefault();
  }

  // ── Startup ────────────────────────────────────────────────────────────

  onMount(() => {
    // Refresh a pane shortly after its folder changes on disk; bursts of
    // events (e.g. a big copy) collapse into one reload.
    const timers: Record<string, ReturnType<typeof setTimeout>> = {};
    const unlisten = listen<{ pane: "left" | "right"; path: string }>("dir-changed", ({ payload }) => {
      const pane = panes[payload.pane];
      if (!pane || pane.path !== payload.path) return;
      clearTimeout(timers[payload.pane]);
      timers[payload.pane] = setTimeout(() => pane.refresh().catch(showError), 150);
    });

    api
      .homeDir()
      .then((home) => Promise.all([left.load(home), right.load(home)]))
      .catch(showError);

    return () => {
      unlisten.then((fn) => fn());
    };
  });

  const targetCount = $derived(active.selected.size || (active.current ? 1 : 0));
</script>

<svelte:window onkeydown={onKey} />
<svelte:head><title>Patina — {active.title}</title></svelte:head>

<main>
  <div class="panes">
    <Pane pane={left} active={activeId === "left"} onactivate={() => (activeId = "left")} />
    <Pane pane={right} active={activeId === "right"} onactivate={() => (activeId = "right")} />
  </div>

  <footer class="toolbar">
    <button onclick={copyToOther} disabled={busy || !targetCount}>
      Copy {activeId === "left" ? "→" : "←"} <kbd>F5</kbd>
    </button>
    <button onclick={moveToOther} disabled={busy || !targetCount}>
      Move {activeId === "left" ? "→" : "←"} <kbd>F6</kbd>
    </button>
    <button onclick={rename} disabled={busy || !active.current}>Rename <kbd>F2</kbd></button>
    <button onclick={newFolder} disabled={busy}>New Folder <kbd>F7</kbd></button>
    <button onclick={trash} disabled={busy || !targetCount}>Move to Bin <kbd>⌘⌫</kbd></button>
    <span class="spacer"></span>
    <button onclick={() => run(() => active.toggleHidden())}>
      {active.showHidden ? "Hide" : "Show"} hidden <kbd>⌘⇧.</kbd>
    </button>
  </footer>

  {#if toast}
    <div class="toast" role="alert">{toast}</div>
  {/if}

  {#if prompt}
    <div
      class="backdrop"
      role="presentation"
      onmousedown={(e) => e.target === e.currentTarget && (prompt = null)}
    >
      <form class="dialog" onsubmit={submitPrompt}>
        <h2>{prompt.title}</h2>
        <input bind:value={prompt.value} use:selectName spellcheck="false" autocomplete="off" />
        <div class="actions">
          <button type="button" onclick={() => (prompt = null)}>Cancel</button>
          <button type="submit" class="primary" disabled={!prompt.value.trim()}>{prompt.action}</button>
        </div>
      </form>
    </div>
  {/if}
</main>

<style>
  :global(:root) {
    --bg: #ecebe8;
    --pane: #ffffff;
    --border: #d9d7d2;
    --text: #1f2422;
    --muted: #6b716e;
    --faint: #a3a8a5;
    --hover: rgba(0, 0, 0, 0.05);
    --stripe: rgba(0, 0, 0, 0.025);
    --accent: #2f8a7a;
    --accent-soft: rgba(47, 138, 122, 0.45);
    --selection: #2f8a7a;
    --selection-text: #ffffff;
    --selection-muted: rgba(47, 138, 122, 0.16);
    --folder: #5fa89a;
    --button: #ffffff;
    color-scheme: light dark;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #161a19;
      --pane: #1e2322;
      --border: #2e3533;
      --text: #e4e8e6;
      --muted: #8d9592;
      --faint: #5a625f;
      --hover: rgba(255, 255, 255, 0.06);
      --stripe: rgba(255, 255, 255, 0.025);
      --accent: #5cc2ae;
      --accent-soft: rgba(92, 194, 174, 0.45);
      --selection: #2f7f70;
      --selection-text: #ffffff;
      --selection-muted: rgba(92, 194, 174, 0.16);
      --folder: #5cb3a2;
      --button: #262c2a;
    }
  }

  :global(html, body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
    font: 13px/1.3 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    -webkit-user-select: none;
    user-select: none;
    cursor: default;
  }
  :global(button) {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    cursor: default;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 8px;
    box-sizing: border-box;
    gap: 8px;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .toolbar button,
  .actions button {
    padding: 4px 10px;
    border-radius: 6px;
    background: var(--button);
    border: 1px solid var(--border);
  }
  .toolbar button:hover:not(:disabled) {
    border-color: var(--accent-soft);
  }
  .toolbar button:disabled {
    opacity: 0.45;
  }
  .toolbar .spacer {
    flex: 1;
  }
  kbd {
    margin-left: 4px;
    font: 10px ui-monospace, "SF Mono", monospace;
    color: var(--muted);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 52px;
    transform: translateX(-50%);
    max-width: min(560px, 90vw);
    padding: 8px 14px;
    border-radius: 8px;
    background: #b3412f;
    color: white;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: start center;
    padding-top: 18vh;
    background: rgba(0, 0, 0, 0.25);
  }
  .dialog {
    width: min(380px, 90vw);
    padding: 16px;
    border-radius: 10px;
    background: var(--pane);
    border: 1px solid var(--border);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.3);
  }
  .dialog h2 {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 600;
  }
  .dialog input {
    width: 100%;
    box-sizing: border-box;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  .dialog input:focus {
    border-color: var(--accent);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  .actions .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .actions .primary:disabled {
    opacity: 0.5;
  }
</style>
