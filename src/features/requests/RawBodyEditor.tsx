import { useEffect, useRef } from "react";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";

const editorTheme = EditorView.theme({
  "&": { height: "100%", background: "transparent" },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": { fontFamily: "inherit", lineHeight: "inherit" },
  ".cm-content": { padding: "12px", caretColor: "var(--app-accent)" },
  ".cm-line": { padding: "0" },
  ".cm-cursor": { borderLeftColor: "var(--app-accent)" },
});

// Mod-Enter sends the request (see app/hotkeys.ts). CodeMirror's default
// binding would insert a blank line into the body first, and the send would
// carry it.
const editorKeymap = defaultKeymap.filter(
  (binding) => binding.key !== "Mod-Enter",
);

// The raw body editor. A textarea lays out every line of its text on each
// keystroke, which for a 1 MB body takes hundreds of milliseconds and
// megabytes per key; CodeMirror only renders the lines on screen.
//
// Uncontrolled: it starts from `initial` and reports each edit. The caller
// keys it by request, so the text is never another request's.
export function RawBodyEditor({
  initial,
  onChange,
}: {
  initial: string;
  onChange: (raw: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const onChangeRef = useRef(onChange);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    if (!containerRef.current) return;
    const view = new EditorView({
      parent: containerRef.current,
      state: EditorState.create({
        doc: initial,
        extensions: [
          history(),
          keymap.of([...editorKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({
            spellcheck: "false",
            autocorrect: "off",
            autocapitalize: "off",
            "data-body-editor": "",
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              onChangeRef.current(update.state.doc.toString());
            }
          }),
          editorTheme,
        ],
      }),
    });
    return () => view.destroy();
    // Uncontrolled: `initial` is only the starting text.
    // oxlint-disable-next-line react/exhaustive-deps
  }, []);

  return (
    <div
      ref={containerRef}
      className="min-h-0 flex-1 font-mono text-xs leading-5"
    />
  );
}
