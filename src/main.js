const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── Action registry ──────────────────────────────────────────────────

const ACTIONS = [
  { value: "generate_guid", label: "Generate GUID", type: "generator" },
  { value: "timestamp_iso",  label: "Timestamp (ISO)", type: "generator" },
  { value: "timestamp_unix", label: "Timestamp (Unix)", type: "generator" },
  { value: "timestamp_utc",  label: "Timestamp (UTC)", type: "generator" },
  { value: "secret",        label: "Secret (keychain)", type: "generator", hasKey: true },
  { value: "unix_to_date",  label: "Unix → Date", type: "transformer" },
  { value: "date_to_unix",  label: "Date → Unix", type: "transformer" },
  { value: "format_json",   label: "Format JSON", type: "transformer" },
  { value: "format_xml",    label: "Format XML", type: "transformer" },
  { value: "format_yaml",   label: "Format YAML", type: "transformer" },
  { value: "base64_encode",  label: "Base64 Encode", type: "transformer" },
  { value: "base64_decode",  label: "Base64 Decode", type: "transformer" },
  { value: "url_encode",    label: "URL Encode", type: "transformer" },
  { value: "url_decode",    label: "URL Decode", type: "transformer" },
  { value: "jwt_decode",    label: "JWT Decode", type: "transformer" },
  { value: "hex_encode",    label: "Hex Encode", type: "transformer" },
  { value: "hex_decode",    label: "Hex Decode", type: "transformer" },
  { value: "html_decode",   label: "HTML Decode", type: "transformer" },
  { value: "md_to_html",    label: "Markdown → HTML", type: "transformer" },
  { value: "html_to_md",    label: "HTML → Markdown", type: "transformer" },
  { value: "number_convert", label: "Number Base Convert", type: "transformer" },
  { value: "color_convert",  label: "Color Convert", type: "transformer" },
  { value: "json_to_yaml",  label: "JSON → YAML", type: "transformer" },
  { value: "json_to_toml",  label: "JSON → TOML", type: "transformer" },
  { value: "yaml_to_json",  label: "YAML → JSON", type: "transformer" },
  { value: "toml_to_json",  label: "TOML → JSON", type: "transformer" },
  { value: "hash_md5",     label: "Hash MD5", type: "transformer" },
  { value: "hash_sha1",    label: "Hash SHA1", type: "transformer" },
  { value: "hash_sha256",  label: "Hash SHA256", type: "transformer" },
  { value: "uppercase",     label: "UPPERCASE", type: "transformer" },
  { value: "lowercase",     label: "lowercase", type: "transformer" },
  { value: "trim",          label: "Trim whitespace", type: "transformer" },
  { value: "lorem_ipsum",   label: "Lorem Ipsum", type: "generator", hasKey: true },
  { value: "roll",          label: "Roll Dice", type: "generator", keyOptional: true, keyPlaceholder: "e.g. 1d20 (blank = prompt at runtime)", display: true },
  { value: "regex_extract", label: "Regex Extract", type: "transformer", hasKey: true },
  { value: "count",         label: "Count", type: "transformer", display: true },
  { value: "snippet",      label: "Snippet (template)", type: "generator", hasTemplate: true },
];

function actionMeta(value) {
  return ACTIONS.find(a => a.value === value) || {};
}

function buildActionOptions(isFirstStep) {
  // First step: all actions. Later steps: only transformers.
  const list = isFirstStep ? ACTIONS : ACTIONS.filter(a => a.type === "transformer");
  return list.map(a => `<option value="${a.value}">${a.label}</option>`).join("");
}

// ── State ────────────────────────────────────────────────────────────

let commands = [];
let selectedIndex = 0;
let suggestedActions = []; // actions suggested based on clipboard content
let settings = { hotkey: "Ctrl+Shift+H", launch_on_startup: false, auto_paste: false };
let editingCmdIndex = -1;
let editingSteps = [];
let isRecordingHotkey = false;
let isRecordingCmdHotkey = false;
let editingCmdHotkey = null; // null = no hotkey

// Snippet prompt state
let snippetCmd = null;       // the command being executed
let snippetVars = [];        // unique variable names extracted from template
let snippetValues = {};      // {varName: "typed value"}
let snippetVarIndex = 0;     // which variable we're prompting for

// ── DOM refs ─────────────────────────────────────────────────────────

const paletteView = document.getElementById("palette-view");
const settingsView = document.getElementById("settings-view");
const searchEl = document.getElementById("search");
const resultsEl = document.getElementById("results");
const toastEl = document.getElementById("toast");

const settingsBtn = document.getElementById("settings-btn");
const backBtn = document.getElementById("back-btn");
const hotkeyInput = document.getElementById("hotkey-input");
const hotkeyRecordBtn = document.getElementById("hotkey-record-btn");
const autostartToggle = document.getElementById("autostart-toggle");
const autopasteToggle = document.getElementById("autopaste-toggle");
const saveSettingsBtn = document.getElementById("save-settings-btn");
const cmdList = document.getElementById("cmd-list");
const addCmdBtn = document.getElementById("add-cmd-btn");

const addSecretBtn = document.getElementById("add-secret-btn");
const secretForm = document.getElementById("secret-form");
const secretKeyInput = document.getElementById("secret-key");
const secretValueInput = document.getElementById("secret-value");
const saveSecretBtn = document.getElementById("save-secret-btn");
const cancelSecretBtn = document.getElementById("cancel-secret-btn");

const snippetView = document.getElementById("snippet-view");
const snippetVarNameEl = document.getElementById("snippet-var-name");
const snippetProgressEl = document.getElementById("snippet-progress");
const snippetInputEl = document.getElementById("snippet-input");
const snippetPreviewEl = document.getElementById("snippet-preview");

const rollPromptView = document.getElementById("roll-prompt-view");
const rollPromptInput = document.getElementById("roll-prompt-input");

const cmdHotkeyInput = document.getElementById("cmd-hotkey-input");
const cmdHotkeyRecordBtn = document.getElementById("cmd-hotkey-record-btn");
const cmdHotkeyClearBtn = document.getElementById("cmd-hotkey-clear-btn");
const cmdTriggerInput = document.getElementById("cmd-trigger-input");
const cmdModal = document.getElementById("cmd-modal");
const cmdModalTitle = document.getElementById("cmd-modal-title");
const cmdNameInput = document.getElementById("cmd-name");
const pipelineStepsEl = document.getElementById("pipeline-steps");
const addStepBtn = document.getElementById("add-step-btn");
const cmdSaveBtn = document.getElementById("cmd-save-btn");
const cmdCancelBtn = document.getElementById("cmd-cancel-btn");
const cmdDeleteBtn = document.getElementById("cmd-delete-btn");

// ── Palette ──────────────────────────────────────────────────────────

async function loadCommands() {
  try {
    commands = await invoke("get_commands");
    suggestedActions = await invoke("detect_clipboard");
  } catch (err) {
    commands = [];
    suggestedActions = [];
  }
  renderResults(getSorted());
}

function stepsLabel(steps) {
  return steps.map(s => {
    const meta = actionMeta(s.action);
    let label = meta.label || s.action;
    if (s.key) label += ":" + s.key;
    if (s.template) {
      const preview = s.template.length > 30 ? s.template.slice(0, 30) + "..." : s.template;
      label = preview;
    }
    return label;
  }).join(" → ");
}

function isSuggested(cmd) {
  // A command is suggested if its first step matches a detected action
  return cmd.steps.length > 0 && suggestedActions.includes(cmd.steps[0].action);
}

function getSorted() {
  // Sort: pinned first, then suggested, then rest
  return [...commands].sort((a, b) => {
    if (a.pinned && !b.pinned) return -1;
    if (!a.pinned && b.pinned) return 1;
    const aSug = isSuggested(a), bSug = isSuggested(b);
    if (aSug && !bSug) return -1;
    if (!aSug && bSug) return 1;
    return 0;
  });
}

function renderResults(filtered) {
  resultsEl.innerHTML = "";
  filtered.forEach((cmd, i) => {
    const li = document.createElement("li");
    const badges = [];
    if (cmd.pinned) badges.push('<span class="badge pin-badge">pinned</span>');
    if (isSuggested(cmd)) badges.push('<span class="badge suggest-badge">suggested</span>');

    li.innerHTML = `
      <button class="pin-btn ${cmd.pinned ? "pinned" : ""}" title="${cmd.pinned ? "Unpin" : "Pin to top"}">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="${cmd.pinned ? "currentColor" : "none"}" stroke="currentColor" stroke-width="2"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/></svg>
      </button>
      <span class="cmd-name">${escHtml(cmd.name)}</span>
      ${badges.join("")}
      <span class="action-tag">${escHtml(stepsLabel(cmd.steps))}</span>
    `;
    if (i === selectedIndex) li.classList.add("selected");

    // Pin button click
    li.querySelector(".pin-btn").addEventListener("click", async (e) => {
      e.stopPropagation();
      await invoke("toggle_pin", { name: cmd.name });
      commands = await invoke("get_commands");
      selectedIndex = 0;
      renderResults(searchEl.value.trim() ? getFiltered() : getSorted());
    });

    li.addEventListener("click", () => executeAt(i, filtered));
    li.addEventListener("mouseenter", () => {
      selectedIndex = i;
      updateSelection();
    });
    resultsEl.appendChild(li);
  });
}

function updateSelection() {
  const items = resultsEl.querySelectorAll("li");
  items.forEach((li, i) => li.classList.toggle("selected", i === selectedIndex));
  const sel = items[selectedIndex];
  if (sel) sel.scrollIntoView({ block: "nearest" });
}

function getFiltered() {
  const q = searchEl.value.toLowerCase().trim();
  if (!q) return getSorted();
  const filtered = commands.filter(c =>
    c.name.toLowerCase().includes(q) ||
    stepsLabel(c.steps).toLowerCase().includes(q)
  );
  return filtered;
}

function isDisplayCommand(cmd) {
  // Check if the last step is a display-type action (like count)
  const lastStep = cmd.steps[cmd.steps.length - 1];
  const meta = actionMeta(lastStep?.action);
  return meta.display === true;
}

function hasSnippetStep(cmd) {
  return cmd.steps.some(s => s.action === "snippet");
}

function hasRollPromptStep(cmd) {
  return cmd.steps.some(s => s.action === "roll" && !s.key);
}

async function executeAt(index, filtered) {
  const cmd = filtered[index];
  if (!cmd) return;

  // If command has a snippet step, start the snippet prompt flow
  if (hasSnippetStep(cmd)) {
    startSnippetPrompt(cmd);
    return;
  }

  // If command has a roll step without a preset notation, prompt for it
  if (hasRollPromptStep(cmd)) {
    startRollPrompt(cmd);
    return;
  }

  try {
    const result = await invoke("execute_command", { steps: cmd.steps });
    if (isDisplayCommand(cmd)) {
      showResultOverlay(result, cmd.name);
    } else {
      showToast("Copied!");
      setTimeout(() => invoke("hide_window"), 350);
    }
  } catch (err) {
    showToast("Error: " + err, true);
  }
}

function showResultOverlay(resultJson, title) {
  let data;
  try { data = JSON.parse(resultJson); } catch { data = { result: resultJson }; }

  const overlay = document.getElementById("result-overlay");
  const body = document.getElementById("result-body");
  document.getElementById("result-title").textContent = title;

  const labels = {
    characters: "Characters",
    characters_no_spaces: "Characters (no spaces)",
    words: "Words",
    lines: "Lines",
    bytes: "Bytes",
    total: "Total",
    rolls: "Rolls",
  };

  body.innerHTML = "";
  for (const [key, value] of Object.entries(data)) {
    const row = document.createElement("div");
    row.className = "result-row";
    row.innerHTML = `
      <span class="result-label">${escHtml(labels[key] || key)}</span>
      <span class="result-value">${escHtml(String(value))}</span>
      <button class="result-copy" title="Copy">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
      </button>
    `;
    row.querySelector(".result-copy").addEventListener("click", () => {
      navigator.clipboard.writeText(String(value));
      showToast("Copied: " + value);
    });
    body.appendChild(row);
  }

  overlay.classList.remove("hidden");

  // Dismiss on clicking backdrop
  overlay.onclick = (e) => {
    if (e.target === overlay) overlay.classList.add("hidden");
  };
}

function resetPalette() {
  searchEl.value = "";
  selectedIndex = 0;
  loadCommands();
  showView("palette");
  searchEl.focus();
}

// ── Settings view ────────────────────────────────────────────────────

async function loadSettings() {
  try {
    settings = await invoke("get_settings");
    hotkeyInput.value = settings.hotkey;
    const enabled = await invoke("get_autostart_enabled");
    settings.launch_on_startup = enabled;
    autostartToggle.checked = enabled;
    autopasteToggle.checked = settings.auto_paste;
  } catch (err) {
    showToast("Failed to load settings", true);
  }
}

function renderCmdList() {
  cmdList.innerHTML = "";
  commands.forEach((cmd, i) => {
    const li = document.createElement("li");
    li.innerHTML = `
      <span class="cmd-name">${escHtml(cmd.name)}</span>
      <span class="action-tag">${escHtml(stepsLabel(cmd.steps))}</span>
    `;
    li.addEventListener("click", () => openCmdModal(i));
    cmdList.appendChild(li);
  });
}

function showView(name) {
  if (name === "settings") {
    paletteView.classList.add("hidden");
    settingsView.classList.remove("hidden");
    cmdModal.classList.add("hidden");
    loadSettings();
    renderCmdList();
  } else {
    settingsView.classList.add("hidden");
    paletteView.classList.remove("hidden");
    cmdModal.classList.add("hidden");
    searchEl.focus();
  }
}

// ── Hotkey recording ─────────────────────────────────────────────────

function startHotkeyRecording() {
  isRecordingHotkey = true;
  hotkeyInput.value = "Press keys...";
  hotkeyInput.classList.add("recording");
  hotkeyRecordBtn.classList.add("recording");
  hotkeyRecordBtn.textContent = "Cancel";
}

function stopHotkeyRecording(value) {
  isRecordingHotkey = false;
  hotkeyInput.classList.remove("recording");
  hotkeyRecordBtn.classList.remove("recording");
  hotkeyRecordBtn.textContent = "Record";
  if (value) hotkeyInput.value = value;
}

function handleHotkeyKeydown(e) {
  if (!isRecordingHotkey) return;
  e.preventDefault();
  e.stopPropagation();
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;

  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Win");

  let keyName = e.key;
  if (keyName === " ") keyName = "Space";
  else if (keyName.length === 1) keyName = keyName.toUpperCase();
  parts.push(keyName);

  const combo = parts.join("+");
  stopHotkeyRecording(combo);
  settings.hotkey = combo;
}

// ── Command hotkey recording ─────────────────────────────────────────

function startCmdHotkeyRecording() {
  isRecordingCmdHotkey = true;
  cmdHotkeyInput.value = "Press keys...";
  cmdHotkeyInput.classList.add("recording");
  cmdHotkeyRecordBtn.textContent = "Cancel";
}

function stopCmdHotkeyRecording(value) {
  isRecordingCmdHotkey = false;
  cmdHotkeyInput.classList.remove("recording");
  cmdHotkeyRecordBtn.textContent = "Record";
  if (value) {
    cmdHotkeyInput.value = value;
    editingCmdHotkey = value;
  }
}

function handleCmdHotkeyKeydown(e) {
  if (!isRecordingCmdHotkey) return;
  e.preventDefault();
  e.stopPropagation();
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;

  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Win");

  let keyName = e.key;
  if (keyName === " ") keyName = "Space";
  else if (keyName.length === 1) keyName = keyName.toUpperCase();
  parts.push(keyName);

  stopCmdHotkeyRecording(parts.join("+"));
}

// ── Pipeline editor ──────────────────────────────────────────────────

function openCmdModal(index) {
  editingCmdIndex = index;
  if (index === -1) {
    cmdModalTitle.textContent = "New Pipeline";
    cmdNameInput.value = "";
    editingSteps = [{ action: "generate_guid" }];
    editingCmdHotkey = null;
    cmdHotkeyInput.value = "";
    cmdTriggerInput.value = "";
    cmdDeleteBtn.classList.add("hidden");
  } else {
    const cmd = commands[index];
    cmdModalTitle.textContent = "Edit Pipeline";
    cmdNameInput.value = cmd.name;
    editingSteps = cmd.steps.map(s => ({ ...s }));
    editingCmdHotkey = cmd.hotkey || null;
    cmdHotkeyInput.value = cmd.hotkey || "";
    cmdTriggerInput.value = cmd.trigger || "";
    cmdDeleteBtn.classList.remove("hidden");
  }
  renderPipelineSteps();
  cmdModal.classList.remove("hidden");
  cmdNameInput.focus();
}

function renderPipelineSteps() {
  pipelineStepsEl.innerHTML = "";
  editingSteps.forEach((step, i) => {
    // Arrow between steps
    if (i > 0) {
      const arrow = document.createElement("div");
      arrow.className = "pipeline-arrow";
      arrow.textContent = "↓";
      pipelineStepsEl.appendChild(arrow);
    }

    const row = document.createElement("div");
    row.className = "pipeline-step";

    const select = document.createElement("select");
    select.innerHTML = buildActionOptions(i === 0);
    select.value = step.action;
    // If the value wasn't found (e.g. generator in non-first slot), default
    if (select.value !== step.action) {
      select.value = select.options[0]?.value || "";
      step.action = select.value;
    }
    select.addEventListener("change", () => {
      step.action = select.value;
      renderPipelineSteps(); // re-render to show/hide key input
    });
    row.appendChild(select);

    // Key input (required for hasKey, optional for keyOptional)
    const meta = actionMeta(step.action);
    if (meta.hasKey || meta.keyOptional) {
      const keyInput = document.createElement("input");
      keyInput.type = "text";
      keyInput.className = "step-key";
      keyInput.placeholder = meta.keyPlaceholder || "key name";
      keyInput.value = step.key || "";
      keyInput.addEventListener("input", () => { step.key = keyInput.value; });
      row.appendChild(keyInput);
    }

    // Remove button (only if more than one step)
    if (editingSteps.length > 1) {
      const removeBtn = document.createElement("button");
      removeBtn.className = "remove-step";
      removeBtn.textContent = "×";
      removeBtn.title = "Remove step";
      removeBtn.addEventListener("click", () => {
        editingSteps.splice(i, 1);
        renderPipelineSteps();
      });
      row.appendChild(removeBtn);
    }

    pipelineStepsEl.appendChild(row);

    // Template textarea for snippet steps
    if (meta.hasTemplate) {
      const wrapper = document.createElement("div");
      wrapper.className = "step-template-wrapper";
      const textarea = document.createElement("textarea");
      textarea.className = "step-template";
      textarea.placeholder = "e.g. SELECT {{columns}} FROM {{table}} WHERE {{condition}}";
      textarea.value = step.template || "";
      textarea.rows = 3;
      textarea.addEventListener("input", () => { step.template = textarea.value; });
      wrapper.appendChild(textarea);
      const vars = extractVariables(textarea.value);
      if (vars.length > 0) {
        const hint = document.createElement("small");
        hint.className = "hint";
        hint.textContent = "Variables: " + vars.join(", ");
        wrapper.appendChild(hint);
      }
      textarea.addEventListener("input", () => {
        step.template = textarea.value;
        const v = extractVariables(textarea.value);
        const existing = wrapper.querySelector(".hint");
        if (v.length > 0) {
          if (existing) existing.textContent = "Variables: " + v.join(", ");
          else {
            const h = document.createElement("small");
            h.className = "hint";
            h.textContent = "Variables: " + v.join(", ");
            wrapper.appendChild(h);
          }
        } else if (existing) existing.remove();
      });
      pipelineStepsEl.appendChild(wrapper);
    }
  });
}

function addStep() {
  editingSteps.push({ action: "uppercase" });
  renderPipelineSteps();
}

async function saveCommand() {
  const name = cmdNameInput.value.trim();
  if (!name) { showToast("Name is required", true); return; }

  // Validate steps
  for (let i = 0; i < editingSteps.length; i++) {
    const s = editingSteps[i];
    const meta = actionMeta(s.action);
    if (meta.hasKey && !s.key?.trim()) {
      showToast(`Step ${i + 1}: key is required for ${meta.label}`, true);
      return;
    }
    if (meta.hasTemplate && !s.template?.trim()) {
      showToast(`Step ${i + 1}: template is required`, true);
      return;
    }
  }

  // Clean up steps (remove fields not needed for this action type)
  const steps = editingSteps.map(s => {
    const meta = actionMeta(s.action);
    const step = { action: s.action };
    if (meta.hasKey) step.key = s.key.trim();
    else if (meta.keyOptional && s.key?.trim()) step.key = s.key.trim();
    if (meta.hasTemplate) step.template = s.template.trim();
    return step;
  });

  const trigger = cmdTriggerInput.value.trim() || null;
  const hotkey = editingCmdHotkey || null;
  const pinned = (editingCmdIndex >= 0) ? commands[editingCmdIndex].pinned : false;
  const cmd = { name, steps, pinned, hotkey, trigger };

  if (editingCmdIndex === -1) {
    commands.push(cmd);
  } else {
    commands[editingCmdIndex] = cmd;
  }

  await invoke("save_commands", { cmds: commands });
  cmdModal.classList.add("hidden");
  renderCmdList();
  showToast("Pipeline saved");
}

async function deleteCommand() {
  if (editingCmdIndex < 0) return;
  commands.splice(editingCmdIndex, 1);
  await invoke("save_commands", { cmds: commands });
  cmdModal.classList.add("hidden");
  renderCmdList();
  showToast("Pipeline deleted");
}

// ── Snippet prompt ───────────────────────────────────────────────────

function extractVariables(template) {
  const matches = template.match(/\{\{(\w+)\}\}/g) || [];
  const seen = new Set();
  return matches
    .map(m => m.slice(2, -2))
    .filter(name => { if (seen.has(name)) return false; seen.add(name); return true; });
}

function startSnippetPrompt(cmd) {
  snippetCmd = cmd;
  // Find the snippet step
  const snippetStep = cmd.steps.find(s => s.action === "snippet");
  const template = snippetStep?.template || "";
  snippetVars = extractVariables(template);
  snippetValues = {};
  snippetVarIndex = 0;

  if (snippetVars.length === 0) {
    // No variables — just execute with the raw template
    finishSnippet(template);
    return;
  }

  paletteView.classList.add("hidden");
  settingsView.classList.add("hidden");
  snippetView.classList.remove("hidden");
  renderSnippetPrompt();
  snippetInputEl.focus();
}

function renderSnippetPrompt() {
  const varName = snippetVars[snippetVarIndex];
  snippetVarNameEl.textContent = varName;
  snippetProgressEl.textContent = `${snippetVarIndex + 1} / ${snippetVars.length}`;
  snippetInputEl.value = snippetValues[varName] || "";
  snippetInputEl.placeholder = `Enter ${varName}...`;
  updateSnippetPreview();
  snippetInputEl.focus();
}

function updateSnippetPreview() {
  const snippetStep = snippetCmd.steps.find(s => s.action === "snippet");
  const template = snippetStep?.template || "";
  const currentVar = snippetVars[snippetVarIndex];
  const currentTyped = snippetInputEl.value;

  // Build preview with colored spans
  let html = "";
  let i = 0;
  const re = /\{\{(\w+)\}\}/g;
  let match;
  while ((match = re.exec(template)) !== null) {
    // Text before this match
    html += escHtml(template.slice(i, match.index));
    const name = match[1];
    if (name === currentVar) {
      html += `<span class="current">${escHtml(currentTyped || `{{${name}}}`)}</span>`;
    } else if (snippetValues[name] !== undefined) {
      html += `<span class="filled">${escHtml(snippetValues[name])}</span>`;
    } else {
      html += `<span class="unfilled">{{${escHtml(name)}}}</span>`;
    }
    i = match.index + match[0].length;
  }
  html += escHtml(template.slice(i));
  snippetPreviewEl.innerHTML = html;
}

function snippetAdvance() {
  const varName = snippetVars[snippetVarIndex];
  snippetValues[varName] = snippetInputEl.value;

  if (snippetVarIndex < snippetVars.length - 1) {
    snippetVarIndex++;
    renderSnippetPrompt();
  } else {
    // All filled — build the result
    const snippetStep = snippetCmd.steps.find(s => s.action === "snippet");
    const template = snippetStep?.template || "";
    let filled = template;
    for (const [name, val] of Object.entries(snippetValues)) {
      filled = filled.replaceAll(`{{${name}}}`, val);
    }
    finishSnippet(filled);
  }
}

function snippetGoBack() {
  if (snippetVarIndex > 0) {
    const varName = snippetVars[snippetVarIndex];
    snippetValues[varName] = snippetInputEl.value;
    snippetVarIndex--;
    renderSnippetPrompt();
  }
}

function cancelSnippet() {
  snippetView.classList.add("hidden");
  paletteView.classList.remove("hidden");
  searchEl.focus();
}

async function finishSnippet(filledText) {
  snippetView.classList.add("hidden");

  // Find steps after the snippet step and run them with the filled text
  const snippetIndex = snippetCmd.steps.findIndex(s => s.action === "snippet");
  const remainingSteps = snippetCmd.steps.slice(snippetIndex + 1);

  try {
    if (remainingSteps.length > 0) {
      await invoke("execute_with_input", { input: filledText, steps: remainingSteps });
    } else {
      // No further steps — just copy the filled text
      await invoke("execute_with_input", { input: filledText, steps: [] });
    }
    showToast("Copied!");
    setTimeout(() => invoke("hide_window"), 350);
  } catch (err) {
    showToast("Error: " + err, true);
    paletteView.classList.remove("hidden");
  }
}

// Snippet keyboard handling
snippetInputEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && e.shiftKey) {
    e.preventDefault();
    snippetGoBack();
  } else if (e.key === "Enter" || e.key === "Tab") {
    e.preventDefault();
    snippetAdvance();
  } else if (e.key === "Escape") {
    e.preventDefault();
    cancelSnippet();
  }
});

snippetInputEl.addEventListener("input", () => {
  updateSnippetPreview();
});

// ── Roll prompt ──────────────────────────────────────────────────────

let rollPromptCmd = null;

function startRollPrompt(cmd) {
  rollPromptCmd = cmd;
  paletteView.classList.add("hidden");
  settingsView.classList.add("hidden");
  snippetView.classList.add("hidden");
  rollPromptView.classList.remove("hidden");
  rollPromptInput.value = "";
  rollPromptInput.focus();
}

function cancelRollPrompt() {
  rollPromptView.classList.add("hidden");
  paletteView.classList.remove("hidden");
  searchEl.focus();
}

async function finishRollPrompt() {
  const notation = rollPromptInput.value.trim();
  if (!notation) { showToast("Enter dice notation", true); return; }

  const steps = rollPromptCmd.steps.map(s =>
    s.action === "roll" && !s.key ? { ...s, key: notation } : s
  );

  rollPromptView.classList.add("hidden");
  paletteView.classList.remove("hidden");

  try {
    const result = await invoke("execute_command", { steps });
    if (isDisplayCommand(rollPromptCmd)) {
      showResultOverlay(result, rollPromptCmd.name);
    } else {
      showToast("Copied!");
      setTimeout(() => invoke("hide_window"), 350);
    }
  } catch (err) {
    showToast("Error: " + err, true);
    searchEl.focus();
  }
}

rollPromptInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    finishRollPrompt();
  } else if (e.key === "Escape") {
    e.preventDefault();
    cancelRollPrompt();
  }
});

// ── Secrets ──────────────────────────────────────────────────────────

async function saveSecret() {
  const key = secretKeyInput.value.trim();
  const value = secretValueInput.value;
  if (!key || !value) { showToast("Both fields required", true); return; }
  try {
    await invoke("store_secret", { key, value });

    // Auto-create a command for this secret if one doesn't already exist
    const exists = commands.some(c =>
      c.steps.length === 1 && c.steps[0].action === "secret" && c.steps[0].key === key
    );
    if (!exists) {
      const label = key.replace(/[_-]/g, " ").replace(/\b\w/g, c => c.toUpperCase());
      commands.push({
        name: "Get " + label,
        steps: [{ action: "secret", key }],
      });
      await invoke("save_commands", { cmds: commands });
      renderCmdList();
    }

    showToast("Secret saved + command created");
    secretKeyInput.value = "";
    secretValueInput.value = "";
    secretForm.classList.add("hidden");
  } catch (err) {
    showToast("Error: " + err, true);
  }
}

// ── Toast & helpers ──────────────────────────────────────────────────

function showToast(msg, isError) {
  toastEl.textContent = msg;
  toastEl.classList.remove("hidden", "error");
  if (isError) toastEl.classList.add("error");
  clearTimeout(showToast._timer);
  showToast._timer = setTimeout(() => toastEl.classList.add("hidden"), 2000);
}

function escHtml(s) {
  const d = document.createElement("div");
  d.textContent = s || "";
  return d.innerHTML;
}

// ── Event listeners ──────────────────────────────────────────────────

searchEl.addEventListener("input", () => {
  selectedIndex = 0;
  renderResults(getFiltered());
});

searchEl.addEventListener("keydown", (e) => {
  const filtered = getFiltered();
  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex = Math.min(selectedIndex + 1, filtered.length - 1);
    updateSelection();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex = Math.max(selectedIndex - 1, 0);
    updateSelection();
  } else if (e.key === "Enter") {
    e.preventDefault();
    executeAt(selectedIndex, filtered);
  } else if (e.key === "Escape") {
    e.preventDefault();
    const overlay = document.getElementById("result-overlay");
    if (!overlay.classList.contains("hidden")) {
      overlay.classList.add("hidden");
    } else {
      invoke("hide_window");
    }
  }
});

settingsBtn.addEventListener("click", () => showView("settings"));
backBtn.addEventListener("click", () => showView("palette"));

hotkeyRecordBtn.addEventListener("click", () => {
  if (isRecordingHotkey) stopHotkeyRecording(settings.hotkey);
  else startHotkeyRecording();
});
document.addEventListener("keydown", handleHotkeyKeydown);

autostartToggle.addEventListener("change", () => {
  settings.launch_on_startup = autostartToggle.checked;
});

autopasteToggle.addEventListener("change", () => {
  settings.auto_paste = autopasteToggle.checked;
});

saveSettingsBtn.addEventListener("click", async () => {
  try {
    await invoke("save_settings", { settings });
    showToast("Settings saved");
    showView("palette");
  } catch (err) {
    showToast("Error: " + err, true);
  }
});

addCmdBtn.addEventListener("click", () => openCmdModal(-1));
addStepBtn.addEventListener("click", addStep);
cmdSaveBtn.addEventListener("click", saveCommand);
cmdCancelBtn.addEventListener("click", () => cmdModal.classList.add("hidden"));
cmdDeleteBtn.addEventListener("click", deleteCommand);

// Command hotkey recording
cmdHotkeyRecordBtn.addEventListener("click", () => {
  if (isRecordingCmdHotkey) stopCmdHotkeyRecording(editingCmdHotkey);
  else startCmdHotkeyRecording();
});
cmdHotkeyClearBtn.addEventListener("click", () => {
  editingCmdHotkey = null;
  cmdHotkeyInput.value = "";
  if (isRecordingCmdHotkey) stopCmdHotkeyRecording(null);
});
document.addEventListener("keydown", handleCmdHotkeyKeydown);

addSecretBtn.addEventListener("click", () => {
  secretForm.classList.toggle("hidden");
  if (!secretForm.classList.contains("hidden")) secretKeyInput.focus();
});
saveSecretBtn.addEventListener("click", saveSecret);
cancelSecretBtn.addEventListener("click", () => {
  secretForm.classList.add("hidden");
  secretKeyInput.value = "";
  secretValueInput.value = "";
});

listen("focus-search", () => resetPalette());

// Execute a command by name (triggered by per-command hotkey from backend)
listen("execute-by-name", async (event) => {
  const name = event.payload;
  await loadCommands();
  const cmd = commands.find(c => c.name === name);
  if (!cmd) return;
  if (hasSnippetStep(cmd)) {
    startSnippetPrompt(cmd);
  } else if (hasRollPromptStep(cmd)) {
    startRollPrompt(cmd);
  } else {
    try {
      const result = await invoke("execute_command", { steps: cmd.steps });
      if (isDisplayCommand(cmd)) {
        showResultOverlay(result, cmd.name);
      } else {
        showToast("Copied!");
        setTimeout(() => invoke("hide_window"), 350);
      }
    } catch (err) {
      showToast("Error: " + err, true);
    }
  }
});

// Trigger fired notification from backend
listen("trigger-fired", (event) => {
  showToast("Auto: " + event.payload);
});

window.addEventListener("DOMContentLoaded", () => {
  loadCommands();
  searchEl.focus();
});
