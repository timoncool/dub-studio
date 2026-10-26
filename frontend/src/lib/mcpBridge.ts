/**
 * The page's side of the MCP bridge: an agent connected to the studio's MCP server sees this window and
 * works in it like the user, who watches every step - a picture of the window, its controls, clicks and
 * typing, and the editor's own commands (select a line, edit it, cut and join lines on the timeline, play,
 * restyle, export). The service streams each command here and waits for the answer this page posts back
 * by the command's id; the same stream says what changed behind the window, and the screens read it again.
 *
 * Commands that belong to a screen are answered by that screen through `useBridgeCommand` while it is
 * mounted. What the agent is told is English, as the tools' own descriptions are; what the user sees of it
 * goes through i18n in the components.
 */
import { useEffect, useLayoutEffect, useRef } from "react";
import { domToJpeg } from "modern-screenshot";
import { BASE, WINDOW_ID } from "./api";

export type BridgeArgs = Record<string, unknown>;
type Handler = (args: BridgeArgs) => unknown;

const handlers = new Map<string, Handler>();

/** Answers a command while the calling component is mounted. */
export function useBridgeCommand(command: string, handler: Handler): void {
  const latest = useRef(handler);
  useLayoutEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    const run: Handler = (args) => latest.current(args);
    handlers.set(command, run);
    return () => {
      if (handlers.get(command) === run) handlers.delete(command);
    };
  }, [command]);
}


// ---------------------------------------------------------------- what the agent is told

const MESSAGES = {
  no_ref: (a: BridgeArgs) => `No element ${a.ref} on the page now; call ui_read_page again, refs change when the page does.`,
  no_label: (a: BridgeArgs) => `No control labelled "${a.text}" is visible; call ui_read_page to see what is.`,
  no_target: () => "Name the control by ref (from ui_read_page) or by its label in text.",
  not_text: () => "That element does not take text.",
  not_list: () => "That element is not a list; use ui_click on its options.",
  no_option: (a: BridgeArgs) => `The list has no option ${JSON.stringify(a.value)}; it has ${a.options}.`,
  hidden: () => "The studio's window is hidden - minimised or covered by other windows - so it draws nothing to copy. Ask the user to bring it to the front, or use ui_read_page, which works either way.",
  no_screen: (a: BridgeArgs) => `The window cannot do "${a.command}" on the screen it shows now; open the screen it belongs to first (editor_open for a project's editor).`,
  unknown_field: (a: BridgeArgs) => `${a.command} does not take ${a.field}; it takes ${a.known}.`,
  missing: (a: BridgeArgs) => `${a.command} needs ${a.field}.`,
  bad_value: (a: BridgeArgs) => `${a.field} of ${a.command} must be ${a.expected}.`,
  no_line: (a: BridgeArgs) => `No line ${a.id} in the open project; editor_state or project_get lists them.`,
  no_blur: (a: BridgeArgs) => `No blur box ${a.idx} in the open project.`,
  no_title: (a: BridgeArgs) => `No title ${a.idx} in the open project.`,
  no_project: (a: BridgeArgs) => `No project ${a.pid}: ${a.reason}. projects_list names them.`,
  no_frame: () => "The editor shows no frame now: the project is audio only, the finished video or the characters are shown, or the frame has not loaded; editor_seek first, or use project_frame.",
  busy: () => "The editor is voicing a line now; call again when it is done (editor_state).",
  exporting: () => "The window is already rendering this project; editor_state shows how far it got.",
  shortening: () => "The window is shortening lines now; studio_wait until shorten, then call again.",
  glossary_unsaved: () => "The glossary window holds the user's unsaved edits; ask them to save or close it first.",
  glossary_busy: () => "The glossary window is saving, collecting terms or translating again; call again when it is done.",
  nothing_to_undo: () => "There is nothing to undo in the window.",
  nothing_to_redo: () => "There is nothing to redo in the window.",
  failed: (a: BridgeArgs) => `${a.command} failed: ${a.reason}`,
} as const;

export type BridgeErrorCode = keyof typeof MESSAGES;

/** An error the agent reads as the tool's answer. */
export function bridgeError(code: BridgeErrorCode, params: BridgeArgs = {}): Error {
  return new Error(MESSAGES[code](params));
}

/** The command's arguments, refusing one it does not take: a misspelt field must not pass unseen. */
export function takeArgs(command: string, args: BridgeArgs, known: readonly string[]): BridgeArgs {
  for (const field of Object.keys(args)) {
    if (!known.includes(field)) throw bridgeError("unknown_field", { command, field, known: known.join(", ") || "nothing" });
  }
  return args;
}

/** A number argument, or undefined when it is not given. */
export function numberArg(command: string, args: BridgeArgs, field: string): number | undefined {
  const value = args[field];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isFinite(value)) throw bridgeError("bad_value", { command, field, expected: "a number" });
  return value;
}

/** A text argument, or undefined when it is not given. */
export function textArg(command: string, args: BridgeArgs, field: string): string | undefined {
  const value = args[field];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") throw bridgeError("bad_value", { command, field, expected: "text" });
  return value;
}

/** A failure of the studio (a refused edit, a lost connection) as the command's answer. */
export function failed(command: string, problem: unknown): Error {
  return bridgeError("failed", { command, reason: problem instanceof Error ? problem.message : String(problem) });
}

// ---------------------------------------------------------------- showing the user what the agent does

/** Every command the agent runs in the window, for the page to show it (see BridgeHost). */
export const AGENT_ACTION = "dub:agent-action";

const FLASH = "mcp-agent-target";

/** Marks an element the agent acts on, for a moment, so the user sees where it acts. */
export function flash(element: Element | null | undefined): void {
  if (!element) return;
  element.classList.remove(FLASH);
  void (element as HTMLElement).offsetWidth;
  element.classList.add(FLASH);
  window.setTimeout(() => element.classList.remove(FLASH), 1800);
}

/** Brings the element matching `selector` into view and marks it, once the page has drawn it (within half a second). */
export function showElement(selector: string): void {
  let frames = 30;
  const look = () => {
    const element = document.querySelector(selector);
    if (!element) {
      if (--frames > 0) window.requestAnimationFrame(look);
      return;
    }
    element.scrollIntoView({ block: "center", behavior: "smooth" });
    flash(element);
  };
  window.requestAnimationFrame(look);
}

// ---------------------------------------------------------------- what is on screen

const INTERACTIVE = 'button, a[href], input, textarea, select, summary, [role="button"], [role="tab"], [role="menuitem"], [role="option"], [role="checkbox"], [role="switch"], [role="slider"], [contenteditable="true"], [data-mcp-context]';

function visible(element: Element): boolean {
  // a closed <details> keeps a box for its content, though nothing but its summary is drawn
  const folded = element.closest("details:not([open])");
  if (folded && !element.closest("summary")) return false;
  const box = element.getBoundingClientRect();
  if (box.width === 0 || box.height === 0) return false;
  const style = getComputedStyle(element);
  return style.visibility !== "hidden" && style.display !== "none";
}

/** The name of a lucide icon a button shows instead of words. */
function iconName(element: Element): string {
  const classes = element.querySelector("svg")?.getAttribute("class") ?? "";
  const icon = classes.split(/\s+/).find((name) => name.startsWith("lucide-"));
  return icon ? `${icon.slice("lucide-".length).replace(/-/g, " ")} icon` : "";
}

/** The text written next to a field: a label, a heading, a caption before it. */
function nearbyText(element: Element): string {
  let node: Element | null = element;
  for (let depth = 0; depth < 2 && node; depth += 1) {
    for (let sibling = node.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
      if (sibling.matches(INTERACTIVE) || sibling.querySelector(INTERACTIVE) || sibling.matches("h1, h2") || sibling.querySelector("h1, h2")) continue;
      const text = (sibling.textContent || "").replace(/\s+/g, " ").trim();
      if (text && text.length <= 80) return text;
    }
    node = node.parentElement;
  }
  return "";
}

/** An element's words, its parts apart: a title and its caption in two spans read as two words, not one. */
function textOf(element: Element): string {
  const parts: string[] = [];
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = (node.textContent || "").replace(/\s+/g, " ").trim();
    if (text) parts.push(text);
  }
  return parts.join(" ");
}

/** A <label>'s own words, without the text of the fields it wraps (a list's options, a field's value). */
function labelText(tag: HTMLLabelElement): string {
  const copy = tag.cloneNode(true) as HTMLElement;
  copy.querySelectorAll("select, textarea, input, button, [role='listbox']").forEach((field) => field.remove());
  const parts: string[] = [];
  const walker = document.createTreeWalker(copy, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) parts.push(node.textContent ?? "");
  return parts.join(" ").replace(/\s+/g, " ").trim();
}

export function label(element: Element): string {
  const named = (element.getAttribute("aria-labelledby") ?? "").split(/\s+/).filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? "").join(" ").replace(/\s+/g, " ").trim();
  if (named) return named.slice(0, 80);
  const own = element.getAttribute("aria-label") || "";
  if (own) return own.slice(0, 80);
  // a button reads by its words before its tooltip, as the user sees it
  const title = element.getAttribute("title") || "";
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) {
    const tied = element.labels?.[0] ? labelText(element.labels[0]) : "";
    return (tied || title || nearbyText(element) || element.getAttribute("placeholder") || "").slice(0, 80);
  }
  return (textOf(element) || title || iconName(element) || nearbyText(element)).slice(0, 80);
}

let nextRef = 1;

function refOf(element: Element): string {
  const existing = element.getAttribute("data-mcp-ref");
  if (existing) return existing;
  const ref = `e${nextRef++}`;
  element.setAttribute("data-mcp-ref", ref);
  return ref;
}

/** The dialog on top, when one is open: its controls are what the user can reach. A backdrop without controls is none. */
function openDialog(): Element | null {
  const dialogs = Array.from(document.querySelectorAll('[role="dialog"], [role="alertdialog"], [aria-modal="true"], .fixed.inset-0'))
    .filter((element) => visible(element) && element.querySelector(INTERACTIVE) !== null);
  return dialogs.length ? dialogs[dialogs.length - 1] : null;
}

/** The mark of a field whose value the agent never reads, even while its show button reveals it to the user. */
export const SECRET = "data-mcp-secret";

/**
 * A key or a password: the agent's answers go to its model's provider, so of such a field it learns only
 * whether it is filled.
 */
function secret(element: Element): boolean {
  return (element instanceof HTMLInputElement && element.type === "password") || element.hasAttribute(SECRET);
}

/**
 * Marks the chosen option of every list in a copy of the page: the copy carries a list's value only as its
 * value attribute, which a drawn list ignores, so it would show its first option.
 */
export function showChosenOptions(cloned: Node): void {
  if (!(cloned instanceof Element)) return;
  const lists = cloned instanceof HTMLSelectElement ? [cloned] : Array.from(cloned.querySelectorAll("select"));
  for (const list of lists) {
    const value = list.getAttribute("value");
    if (value === null) continue;
    for (const option of Array.from(list.options)) {
      if (option.value === value) option.setAttribute("selected", "");
      else option.removeAttribute("selected");
    }
  }
}

/** Draws a secret field of a copy of the page masked, as a password field is. */
export function maskSecret(cloned: Node): void {
  if (cloned instanceof HTMLInputElement && cloned.hasAttribute(SECRET)) cloned.setAttribute("type", "password");
}

/** A field's value, as a line of the page shows it. */
function valueOf(element: Element): string | null {
  if (element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")) return element.checked ? "checked" : "unchecked";
  if ((element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) && secret(element)) return element.value ? "filled" : "empty";
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) return JSON.stringify(element.value.slice(0, 160));
  return null;
}

/**
 * Every visible control, one line each, with the ref the other commands take. The controls of a row of a
 * list (a line of the transcript, a blur box, a title, a project) read as one line named by the row.
 */
export function readPage(): string {
  const lines: string[] = [];
  const title = document.querySelector("h1, h2")?.textContent?.trim();
  if (title) lines.push(`Page: ${title}`);
  const dialog = openDialog();
  if (dialog) lines.push("A dialog is open; its controls are listed first, then the page behind it.");
  const scope = dialog ? [dialog, document.body] : [document.body];
  const seen = new Set<Element>();
  const rows = new Map<string, { at: number; parts: string[] }>();
  for (const root of scope) {
    for (const element of Array.from(root.querySelectorAll(INTERACTIVE))) {
      if (seen.has(element) || !visible(element)) continue;
      seen.add(element);
      const tag = element.tagName.toLowerCase();
      const kind = element.getAttribute("role") || (tag === "input" ? `input ${(element as HTMLInputElement).type}` : tag === "summary" ? "section" : tag);
      const context = element.closest("[data-mcp-context]")?.getAttribute("data-mcp-context");
      const pressed = element.getAttribute("aria-checked") ?? element.getAttribute("aria-pressed") ?? element.getAttribute("aria-selected");
      const folding = tag === "summary" ? element.closest("details") : null;
      const state = folding ? (folding.open ? " open" : " closed") : pressed ? (pressed === "true" ? " on" : " off") : "";
      const disabled = (element as HTMLButtonElement).disabled ? " (disabled)" : "";
      if (context) {
        const row = rows.get(context) ?? { at: lines.length, parts: [] };
        if (!rows.has(context)) {
          rows.set(context, row);
          lines.push("");
        }
        const value = valueOf(element);
        const name = element.hasAttribute("data-mcp-context") ? "open" : label(element) || kind;
        row.parts.push(`${refOf(element)} ${name}${value === null ? "" : `=${value}`}${state}${disabled}`);
        continue;
      }
      const parts = [refOf(element), kind, JSON.stringify(label(element))];
      if (element instanceof HTMLInputElement && element.type === "range") {
        parts.push(`value=${element.value}`, `range=${element.min || 0}..${element.max || 100}`, `step=${element.step || 1}`);
      } else if (element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")) {
        parts.push(element.checked ? "checked" : "unchecked");
      } else if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
        if (element.value) parts.push(secret(element) ? "filled" : `value=${JSON.stringify(element.value.slice(0, 160))}`);
        else parts.push("empty", ...(element.placeholder ? [`placeholder=${JSON.stringify(element.placeholder.slice(0, 80))}`] : []));
      }
      if (element instanceof HTMLSelectElement) {
        parts.push(`value=${JSON.stringify(element.value)}`, `options=${JSON.stringify(Array.from(element.options).map((option) => option.value))}`);
      }
      lines.push(parts.join(" ") + state + disabled);
    }
  }
  for (const [context, row] of rows) lines[row.at] = `${context}: ${row.parts.join(", ")}`;
  return lines.join("\n");
}

function find(args: BridgeArgs): HTMLElement {
  const ref = typeof args.ref === "string" ? args.ref : "";
  const text = typeof args.text === "string" ? args.text.trim().toLowerCase() : "";
  if (!ref && !text) throw bridgeError("no_target");
  let element: Element | null = null;
  if (ref) element = document.querySelector(`[data-mcp-ref="${CSS.escape(ref)}"]`);
  if (!element && text) {
    // an open dialog covers the page: a label is looked for among its controls, as the user can reach only those
    const root = openDialog() ?? document.body;
    const candidates = Array.from(root.querySelectorAll(INTERACTIVE)).filter(visible);
    element = candidates.find((candidate) => label(candidate).toLowerCase() === text) ?? candidates.find((candidate) => label(candidate).toLowerCase().includes(text)) ?? null;
  }
  if (!element) throw ref ? bridgeError("no_ref", { ref }) : bridgeError("no_label", { text: args.text });
  return element as HTMLElement;
}

/** Sets a value the way typing does, so React sees it. */
function setValue(element: HTMLElement, value: string): void {
  if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
    const prototype = element instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
  } else if (element.isContentEditable) {
    element.textContent = value;
    element.dispatchEvent(new InputEvent("input", { bubbles: true }));
  } else {
    throw bridgeError("not_text");
  }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 250));

/** A window in the background gets no focus events from focus() and blur(), so the bridge sends them: a field that
 * saves on blur saves, and a list that opens on focus opens, as with the user's own click. */
function focusIn(element: HTMLElement): void {
  element.focus();
  if (!document.hasFocus()) element.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
}

function focusOut(element: HTMLElement): void {
  element.blur();
  if (!document.hasFocus()) element.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
}

// ---------------------------------------------------------------- what the page logged

const logged: Array<{ level: string; text: string; at: string }> = [];

function remember(level: string, parts: unknown[]): void {
  const text = parts.map((part) => (part instanceof Error ? `${part.name}: ${part.message}` : typeof part === "string" ? part : JSON.stringify(part))).join(" ");
  logged.push({ level, text: text.slice(0, 2000), at: new Date().toISOString() });
  if (logged.length > 100) logged.shift();
}

/** Keeps the page's errors and warnings for an agent to read. */
function watchConsole(): void {
  for (const level of ["error", "warn"] as const) {
    const original = console[level].bind(console);
    console[level] = (...parts: unknown[]) => {
      remember(level, parts);
      original(...parts);
    };
  }
  window.addEventListener("error", (event) => remember("error", [event.message, `${event.filename}:${event.lineno}`]));
  window.addEventListener("unhandledrejection", (event) => remember("error", ["unhandled rejection", event.reason]));
}

/** A picture fetched from the studio, at most `maxWidth` wide, as a JPEG the agent sees. */
export async function pictureOf(url: string, maxWidth: number): Promise<{ image: string; mime: string; width: number; height: number }> {
  const response = await fetch(url, { cache: "force-cache" });
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  const bitmap = await createImageBitmap(await response.blob());
  const scale = Math.min(1, maxWidth / bitmap.width);
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("the page cannot draw a canvas");
  context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return { image: canvas.toDataURL("image/jpeg", 0.9).replace(/^data:image\/jpeg;base64,/, ""), mime: "image/jpeg", width: canvas.width, height: canvas.height };
}

/** The line an agent reads with a frame of the editor. */
export function frameText(seconds: number, width: number, height: number): string {
  return `The editor's frame at ${seconds.toFixed(2)} s, ${width}x${height}.`;
}

const builtIn: Record<string, Handler> = {
  async screenshot(args) {
    takeArgs("ui_screenshot", args, ["max_width"]);
    // a window that is minimised or covered draws no frames, and the copy waits for one
    if (document.visibilityState === "hidden") throw bridgeError("hidden");
    const scale = Math.min(1, (numberArg("ui_screenshot", args, "max_width") ?? 1600) / window.innerWidth);
    const data = await domToJpeg(document.documentElement, { scale, quality: 0.9, width: window.innerWidth, height: window.innerHeight, backgroundColor: getComputedStyle(document.body).backgroundColor, onCloneEachNode: maskSecret, onCloneNode: showChosenOptions });
    return { image: data.replace(/^data:image\/jpeg;base64,/, ""), mime: "image/jpeg", text: `${window.innerWidth}x${window.innerHeight} window` };
  },
  read_page: (args) => {
    takeArgs("ui_read_page", args, []);
    return { text: readPage() };
  },
  console: (args) => {
    takeArgs("ui_console", args, []);
    return { text: logged.length ? logged.map((entry) => `${entry.at} ${entry.level}: ${entry.text}`).join("\n") : "Nothing logged." };
  },
  async click(args) {
    takeArgs("ui_click", args, ["ref", "text"]);
    const element = find(args);
    // named before the click: a button of a row can be renamed by its own click
    const name = label(element) || element.tagName.toLowerCase();
    const row = element.closest("[data-mcp-context]")?.getAttribute("data-mcp-context");
    element.scrollIntoView({ block: "center" });
    flash(element);
    // an input opens its list on focus (the language pickers), as the user's click does
    if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) focusIn(element);
    element.click();
    await settle();
    return { text: `Clicked ${name}${row ? ` in ${row}` : ""}.` };
  },
  async type(args) {
    takeArgs("ui_type", args, ["ref", "text", "value", "submit"]);
    if (typeof args.value !== "string") throw bridgeError("missing", { command: "ui_type", field: "value" });
    const element = find(args);
    element.scrollIntoView({ block: "center" });
    flash(element);
    focusIn(element);
    setValue(element, args.value);
    if (args.submit) element.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    focusOut(element);
    await settle();
    return { text: "Typed." };
  },
  async select(args) {
    takeArgs("ui_select", args, ["ref", "text", "value"]);
    if (typeof args.value !== "string") throw bridgeError("missing", { command: "ui_select", field: "value" });
    const element = find(args);
    if (!(element instanceof HTMLSelectElement)) throw bridgeError("not_list");
    const value = args.value;
    const options = Array.from(element.options).map((option) => option.value);
    if (!options.includes(value)) throw bridgeError("no_option", { value, options: JSON.stringify(options) });
    element.scrollIntoView({ block: "center" });
    flash(element);
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    return { text: `Selected ${element.value}.` };
  },
  async press_key(args) {
    takeArgs("ui_press_key", args, ["key"]);
    if (typeof args.key !== "string" || !args.key.trim()) throw bridgeError("missing", { command: "ui_press_key", field: "key" });
    const target = (document.activeElement as HTMLElement | null) ?? document.body;
    const combo = args.key;
    // Ctrl+K, Shift+Tab: modifiers before the key, joined by +
    const parts = combo.split("+").map((part) => part.trim()).filter(Boolean);
    const key = parts.length > 1 ? parts[parts.length - 1] : combo;
    const held = parts.slice(0, -1).map((part) => part.toLowerCase());
    const init = {
      key: key.length === 1 ? key.toLowerCase() : key,
      code: key === " " || key.toLowerCase() === "space" ? "Space" : key.length === 1 ? `Key${key.toUpperCase()}` : key,
      bubbles: true,
      cancelable: true,
      ctrlKey: held.includes("ctrl") || held.includes("control"),
      shiftKey: held.includes("shift"),
      altKey: held.includes("alt"),
      metaKey: held.includes("meta") || held.includes("cmd") || held.includes("win"),
    };
    if (init.key.toLowerCase() === "space") init.key = " ";
    target.dispatchEvent(new KeyboardEvent("keydown", init));
    target.dispatchEvent(new KeyboardEvent("keyup", init));
    await settle();
    return { text: `Pressed ${combo}.` };
  },
  async scroll(args) {
    takeArgs("ui_scroll", args, ["direction", "amount", "ref", "text"]);
    const amount = Number(args.amount ?? 600) * (args.direction === "up" ? -1 : 1);
    const element = args.ref || args.text ? find(args) : null;
    if (element) {
      element.scrollIntoView({ block: "center" });
      flash(element);
    } else {
      (document.querySelector("[data-kb-scroll]") ?? document.scrollingElement ?? document.body).scrollBy({ top: amount });
    }
    await settle();
    return { text: "Scrolled." };
  },
};

/** The commands every screen of the page answers itself. */
export const BUILT_IN_COMMANDS = Object.keys(builtIn);

/** Commands that only look: the user is not shown them as the agent's actions. */
const LOOKING = new Set(["read_page", "console", "editor_state"]);

// ---------------------------------------------------------------- what changed behind the window

export const PROJECT_CHANGED = "dub:project-changed";
export const PROJECTS_CHANGED = "dub:projects-changed";
export const SETTINGS_CHANGED = "dub:settings-changed";
export const VOICES_CHANGED = "dub:voices-changed";
export const CASTING_CHANGED = "dub:casting-changed";
export const JOBS_CHANGED = "dub:jobs-changed";
export const JOURNAL_CHANGED = "dub:journal-changed";

/** A notice of the studio: what changed, which project, its revision, who changed it, and a job's facts. */
export type ChangeNotice = { changed: string; pid?: string | null; rev?: number; by?: string; job?: boolean; job_id?: string; kind?: string; project_id?: string | null };

/** Whether a change was made by this window itself. */
export const isMine = (notice: ChangeNotice): boolean => notice.by === `window:${WINDOW_ID}`;

const EVENTS: Record<string, string[]> = {
  project: [PROJECT_CHANGED],
  projects: [PROJECTS_CHANGED],
  settings: [SETTINGS_CHANGED],
  voices: [VOICES_CHANGED],
  casting: [CASTING_CHANGED],
  casting_library: [SETTINGS_CHANGED],
  jobs: [JOBS_CHANGED],
  journal: [JOURNAL_CHANGED],
};
const EVERYTHING = [PROJECT_CHANGED, PROJECTS_CHANGED, SETTINGS_CHANGED, VOICES_CHANGED, CASTING_CHANGED, JOURNAL_CHANGED];

function changed(notice: ChangeNotice): void {
  // a job this window started is followed by the window itself; its own list and settings changes it already shows
  if (isMine(notice) && notice.changed !== "project") return;
  for (const name of EVENTS[notice.changed] ?? EVERYTHING) window.dispatchEvent(new CustomEvent(name, { detail: notice }));
}

// ---------------------------------------------------------------- the connection

let started = false;

export function startBridge(): void {
  if (started || typeof window === "undefined" || typeof EventSource === "undefined") return;
  started = true;
  watchConsole();
  let reconnecting = false;
  // the number the service gave this window, for the focus notices below
  let current: number | null = null;
  // an agent's command goes to the window the person turned to, not the one opened last
  const reportFocus = () => {
    if (current === null || document.visibilityState !== "visible" || !document.hasFocus()) return;
    void fetch(`${BASE}/mcp/window/focus`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ window: current }) })
      .catch((problem) => console.error("[ERROR] the studio did not learn which window is in front:", problem));
  };
  window.addEventListener("focus", reportFocus);
  document.addEventListener("visibilitychange", reportFocus);
  const connect = () => {
    const events = new EventSource(`${BASE}/mcp/window`);
    // the service names this window first; a command for another window is not ours
    let ours: number | null = null;
    events.onmessage = async (message) => {
      let data: ChangeNotice & { window?: number; id?: string; command?: string; args?: BridgeArgs };
      try {
        data = JSON.parse(message.data);
      } catch (problem) {
        console.error("[ERROR] the studio sent the window a message that is not JSON:", message.data, problem);
        return;
      }
      if (data.changed) {
        changed(data);
        return;
      }
      if (data.id === undefined) {
        ours = data.window ?? null;
        current = ours;
        reportFocus();
        // what changed while the stream was down is read now
        if (reconnecting) changed({ changed: "everything" });
        return;
      }
      if (data.window !== ours || !data.command) return;
      const { id, command } = data;
      const handler = handlers.get(command) ?? builtIn[command];
      let body: { id: string; result?: unknown; error?: string };
      if (!handler) {
        body = { id, error: bridgeError("no_screen", { command }).message };
      } else {
        if (!LOOKING.has(command)) window.dispatchEvent(new CustomEvent(AGENT_ACTION, { detail: { command, args: data.args ?? {} } }));
        try {
          body = { id, result: (await handler(data.args ?? {})) ?? null };
        } catch (problem) {
          body = { id, error: problem instanceof Error ? problem.message : String(problem) };
        }
      }
      await fetch(`${BASE}/mcp/window/result`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) })
        .catch((problem) => console.error("[ERROR] the answer to the agent did not reach the studio:", problem));
    };
    // the service restarting closes the stream; the page subscribes again
    events.onerror = () => {
      events.close();
      reconnecting = true;
      setTimeout(connect, 2000);
    };
  };
  connect();
}
