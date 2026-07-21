import { useEffect, useMemo, useRef } from "react";
import { EditorState } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  type ViewUpdate,
} from "@codemirror/view";
import {
  HighlightStyle,
  bracketMatching,
  foldGutter,
  syntaxHighlighting,
} from "@codemirror/language";
import { json } from "@codemirror/lang-json";
import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  search,
  searchKeymap,
  setSearchQuery,
} from "@codemirror/search";
import { tags as t } from "@lezer/highlight";

const jsonHighlightStyle = HighlightStyle.define([
  { tag: t.string, color: "rgb(110 231 183)" },
  { tag: t.number, color: "rgb(125 211 252)" },
  { tag: [t.bool, t.null], color: "rgb(252 211 77)" },
  { tag: t.propertyName, color: "rgb(221 214 254)" },
  {
    tag: [t.brace, t.bracket, t.punctuation, t.separator],
    color: "var(--app-dim)",
  },
]);

function createSearchPanel(view: EditorView) {
  const dom = document.createElement("div");
  dom.className = "cm-search cm-panel";

  const icon = document.createElement("span");
  icon.className = "cm-search-icon";
  icon.textContent = "⌕";

  const input = document.createElement("input");
  input.className = "cm-textfield";
  input.placeholder = "Find in response";
  input.setAttribute("main-field", "true");
  input.setAttribute("name", "search");
  input.spellcheck = false;
  input.autocomplete = "off";

  const counter = document.createElement("span");
  counter.className = "cm-search-counter";

  const prevBtn = document.createElement("button");
  prevBtn.type = "button";
  prevBtn.className = "cm-search-btn";
  prevBtn.setAttribute("aria-label", "Previous match");
  prevBtn.textContent = "↑";

  const nextBtn = document.createElement("button");
  nextBtn.type = "button";
  nextBtn.className = "cm-search-btn";
  nextBtn.setAttribute("aria-label", "Next match");
  nextBtn.textContent = "↓";

  const closeBtn = document.createElement("button");
  closeBtn.type = "button";
  closeBtn.className = "cm-search-btn";
  closeBtn.setAttribute("aria-label", "Close");
  closeBtn.textContent = "×";

  dom.append(icon, input, counter, prevBtn, nextBtn, closeBtn);

  input.addEventListener("input", () => {
    view.dispatch({
      effects: setSearchQuery.of(new SearchQuery({ search: input.value })),
    });
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) findPrevious(view);
      else findNext(view);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeSearchPanel(view);
    }
  });
  prevBtn.addEventListener("click", () => findPrevious(view));
  nextBtn.addEventListener("click", () => findNext(view));
  closeBtn.addEventListener("click", () => closeSearchPanel(view));

  function refreshCounter() {
    const query = getSearchQuery(view.state);
    if (!query.search || !query.valid) {
      counter.textContent = "";
      return;
    }
    const sel = view.state.selection.main;
    const cursor = query.getCursor(view.state);
    const cap = 9999;
    let total = 0;
    let active = 0;
    while (total < cap) {
      const match = cursor.next();
      if (match.done) break;
      total++;
      if (match.value.from === sel.from && match.value.to === sel.to) {
        active = total;
      }
    }
    counter.textContent = total
      ? total >= cap
        ? `${active || "?"}/${cap}+`
        : `${active || 1}/${total}`
      : "0/0";
  }

  return {
    dom,
    top: true,
    mount() {
      input.focus();
      input.select();
      refreshCounter();
    },
    update(update: ViewUpdate) {
      const queryChanged = update.transactions.some((tr) =>
        tr.effects.some((effect) => effect.is(setSearchQuery)),
      );
      if (queryChanged) {
        const q = getSearchQuery(update.state);
        if (input.value !== q.search) input.value = q.search;
      }
      if (queryChanged || update.docChanged || update.selectionSet) {
        refreshCounter();
      }
    },
  };
}

const editorTheme = EditorView.theme(
  {
    "&": {
      position: "relative",
      height: "100%",
      background: "transparent",
      color: "var(--app-text)",
      fontSize: "12px",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": {
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, 'Cascadia Mono', 'Roboto Mono', Consolas, 'Liberation Mono', 'Courier New', monospace",
      lineHeight: "1.5",
    },
    ".cm-content": {
      padding: "8px 0",
      caretColor: "var(--app-accent)",
    },
    ".cm-gutters": {
      background: "transparent",
      borderRight: "1px solid var(--app-line)",
      color: "var(--app-dim)",
    },
    ".cm-activeLineGutter, .cm-activeLine": { background: "transparent" },
    ".cm-foldPlaceholder": {
      background: "color-mix(in oklab, var(--app-accent) 14%, transparent)",
      border:
        "1px solid color-mix(in oklab, var(--app-accent) 38%, transparent)",
      color: "var(--app-accent)",
      borderRadius: "4px",
      padding: "0 4px",
      margin: "0 2px",
    },
    ".cm-selectionBackground, ::selection": {
      background:
        "color-mix(in oklab, var(--app-accent) 28%, transparent) !important",
    },
    ".cm-cursor": { borderLeftColor: "var(--app-accent)" },
    ".cm-searchMatch": {
      background: "color-mix(in oklab, var(--app-accent) 28%, transparent)",
      borderRadius: "2px",
    },
    ".cm-searchMatch-selected": {
      background: "color-mix(in oklab, var(--app-accent) 55%, transparent)",
    },
    ".cm-panels": {
      position: "absolute",
      top: "8px",
      right: "8px",
      left: "auto",
      width: "auto",
      background: "transparent",
      border: "none",
      zIndex: "10",
    },
    ".cm-panels.cm-panels-top": {
      borderBottom: "none",
    },
    ".cm-panel.cm-search": {
      display: "flex",
      alignItems: "center",
      gap: "2px",
      padding: "3px 4px 3px 8px",
      background: "color-mix(in srgb, var(--app-panel) 95%, transparent)",
      border: "1px solid var(--app-line)",
      borderRadius: "6px",
      backdropFilter: "blur(6px)",
      boxShadow: "0 4px 12px rgb(0 0 0 / 0.25)",
      font: "inherit",
    },
    ".cm-panel.cm-search .cm-search-icon": {
      color: "var(--app-dim)",
      fontSize: "13px",
      lineHeight: "1",
      paddingRight: "4px",
    },
    ".cm-panel.cm-search input.cm-textfield": {
      width: "200px",
      height: "24px",
      padding: "0 4px",
      margin: "0",
      background: "transparent",
      border: "none",
      color: "var(--app-text)",
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, 'Cascadia Mono', 'Roboto Mono', Consolas, monospace",
      fontSize: "12px",
      outline: "none",
    },
    ".cm-panel.cm-search input.cm-textfield::placeholder": {
      color: "var(--app-dim)",
    },
    ".cm-panel.cm-search .cm-search-counter": {
      minWidth: "44px",
      fontSize: "11px",
      fontVariantNumeric: "tabular-nums",
      color: "var(--app-dim)",
      textAlign: "center",
      padding: "0 4px",
    },
    ".cm-panel.cm-search .cm-search-btn": {
      display: "grid",
      placeItems: "center",
      width: "24px",
      height: "24px",
      borderRadius: "4px",
      background: "transparent",
      border: "none",
      color: "var(--app-dim)",
      cursor: "pointer",
      fontSize: "13px",
      lineHeight: "1",
      padding: "0",
    },
    ".cm-panel.cm-search .cm-search-btn:hover": {
      background: "color-mix(in oklab, var(--app-text) 8%, transparent)",
      color: "var(--app-text)",
    },
  },
  { dark: true },
);

export function ResponseViewer({
  value,
  sending,
}: {
  value?: unknown;
  sending: boolean;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  const text = useMemo(() => stringify(value), [value]);

  useEffect(() => {
    if (!containerRef.current) return;
    const view = new EditorView({
      parent: containerRef.current,
      state: EditorState.create({
        doc: text,
        extensions: [
          EditorState.readOnly.of(true),
          EditorView.contentAttributes.of({ spellcheck: "false" }),
          lineNumbers(),
          foldGutter(),
          bracketMatching(),
          json(),
          syntaxHighlighting(jsonHighlightStyle),
          search({ top: true, createPanel: createSearchPanel }),
          keymap.of(searchKeymap),
          editorTheme,
          EditorView.lineWrapping,
        ],
      }),
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    if (view.state.doc.toString() === text) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: text },
    });
  }, [text]);

  return (
    <div className="relative h-full min-h-0">
      <div ref={containerRef} className="h-full min-h-0" />
      {sending ? (
        <Overlay>Sending request</Overlay>
      ) : value === undefined ? (
        <Overlay>Send a request to see the response.</Overlay>
      ) : null}
    </div>
  );
}

function Overlay({ children }: { children: React.ReactNode }) {
  return (
    <div className="pointer-events-none absolute inset-0 grid place-items-start p-3 font-mono text-xs leading-5 text-[var(--app-dim)]">
      {children}
    </div>
  );
}

function stringify(value: unknown): string {
  if (value === undefined) return "";
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}
