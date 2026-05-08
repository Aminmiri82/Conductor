import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Loader2,
  Save,
  SendHorizontal,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { RequestDetail, UrlDisplayMode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

const methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function RequestUrlBar({
  request,
  sending,
  saving,
  dirty,
  unresolvedKeys,
  onChange,
  onSend,
  onSave,
}: {
  request: RequestDetail;
  sending: boolean;
  saving: boolean;
  dirty: boolean;
  unresolvedKeys: string[];
  onChange: (patch: Partial<RequestDetail>) => void;
  onSend: () => void;
  onSave: () => void;
}) {
  const urlMode = useWorkspaceStore((state) => state.workspaceUi.urlDisplayMode);

  return (
    <div className="flex min-w-0 items-center gap-2">
      <Input
        className="h-8 w-52 border-[var(--app-line)] bg-transparent text-sm font-semibold shadow-none focus-visible:ring-[var(--app-accent)]"
        value={request.name}
        onChange={(event) => onChange({ name: event.target.value })}
      />
      <Select
        value={request.method}
        onValueChange={(method) => onChange({ method })}
      >
        <SelectTrigger
          className="app-mono h-8 w-[104px] border text-xs font-bold"
          style={{
            borderColor: methodColor(request.method).border,
            background: methodColor(request.method).background,
            color: methodColor(request.method).foreground,
          }}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {methods.map((method) => (
            <SelectItem key={method} value={method}>
              {method}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <VariableUrlInput
        value={request.url}
        mode={urlMode}
        unresolvedKeys={unresolvedKeys}
        onChange={(url) => onChange({ url })}
      />
      <Button
        variant="ghost"
        size="icon"
        className={`size-8 shrink-0 border hover:text-[var(--app-text)] ${
          dirty
            ? "border-[var(--app-accent)] text-[var(--app-accent)]"
            : "border-[var(--app-line)] text-emerald-300"
        }`}
        style={{ borderRadius: "var(--app-radius)" }}
        onClick={onSave}
        title={saving ? "Saving request" : dirty ? "Unsaved changes" : "Saved"}
      >
        {saving ? (
          <Loader2 className="size-4 animate-spin" />
        ) : dirty ? (
          <Save className="size-4" />
        ) : (
          <Check className="size-4" />
        )}
      </Button>
      <Button
        className="h-8 shrink-0 gap-1.5 bg-[var(--app-accent)] px-3 text-xs font-bold uppercase tracking-[0.04em] text-[var(--app-accent-fg)] hover:bg-[var(--app-accent)]/90"
        onClick={onSend}
        disabled={sending}
        style={{ borderRadius: "var(--app-radius)" }}
      >
        <SendHorizontal className="size-3.5" />
        {sending ? "Sending" : "Send"}
      </Button>
    </div>
  );
}

function VariableUrlInput({
  value,
  mode,
  unresolvedKeys,
  onChange,
}: {
  value: string;
  mode: UrlDisplayMode;
  unresolvedKeys: string[];
  onChange: (value: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [editing, setEditing] = useState(mode === "flat");
  const tokens = useMemo(() => parseUrlTokens(value), [value]);
  const unresolved = useMemo(() => new Set(unresolvedKeys), [unresolvedKeys]);

  useEffect(() => {
    if (mode === "flat") setEditing(true);
    else setEditing(false);
  }, [mode]);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  if (editing || mode === "flat") {
    return (
      <Input
        ref={inputRef}
        data-url-input
        className="app-mono h-8 min-w-0 flex-1 border-[var(--app-line)] bg-[var(--app-panel-2)] text-xs shadow-none focus-visible:ring-[var(--app-accent)]"
        value={value}
        spellCheck={false}
        onBlur={() => {
          if (mode !== "flat") setEditing(false);
        }}
        onChange={(event) => onChange(event.target.value)}
      />
    );
  }

  return (
    <button
      data-url-input
      className="app-mono flex h-8 min-w-0 flex-1 items-center overflow-hidden border bg-[var(--app-panel-2)] px-2.5 text-left text-xs"
      style={{
        borderColor: "var(--app-line)",
        borderRadius: "var(--app-radius)",
      }}
      onClick={() => setEditing(true)}
      type="button"
    >
      <div className="min-w-0 flex-1 truncate">
        {tokens.map((token, index) =>
          token.kind === "text" ? (
            <span key={index} className="text-[var(--app-text)]">
              {token.value}
            </span>
          ) : (
            <VariableToken
              key={index}
              mode={mode}
              name={token.name}
              unresolved={unresolved.has(token.name)}
            />
          ),
        )}
      </div>
    </button>
  );
}

function VariableToken({
  name,
  mode,
  unresolved,
}: {
  name: string;
  mode: UrlDisplayMode;
  unresolved: boolean;
}) {
  if (mode === "hybrid") {
    return (
      <span className="group mx-0.5 inline-flex align-middle">
        <span className="group-hover:hidden">
          <span className="text-[var(--app-dim)]">{"{{"}</span>
          <span
            className={unresolved ? "text-amber-300" : "text-[var(--app-accent)]"}
          >
            {name}
          </span>
          <span className="text-[var(--app-dim)]">{"}}"}</span>
        </span>
        <span className="hidden group-hover:inline-flex">
          <VariableChip name={name} unresolved={unresolved} />
        </span>
      </span>
    );
  }

  if (mode === "syntax") {
    return (
      <span>
        <span className="text-[var(--app-dim)]">{"{{"}</span>
        <span className={unresolved ? "text-amber-300" : "text-[var(--app-accent)]"}>
          {name}
        </span>
        <span className="text-[var(--app-dim)]">{"}}"}</span>
      </span>
    );
  }

  return <VariableChip name={name} unresolved={unresolved} />;
}

function VariableChip({
  name,
  unresolved,
}: {
  name: string;
  unresolved: boolean;
}) {
  return (
    <span
      className="mx-0.5 inline-flex items-center gap-1 border px-1.5 py-0.5 font-medium"
      style={{
        borderRadius: "var(--app-radius)",
        borderColor: unresolved
          ? "rgb(252 211 77 / 0.45)"
          : "color-mix(in oklab, var(--app-accent) 38%, transparent)",
        background: unresolved
          ? "rgb(252 211 77 / 0.1)"
          : "color-mix(in oklab, var(--app-accent) 14%, transparent)",
        color: unresolved ? "#fcd34d" : "var(--app-accent)",
      }}
    >
      {unresolved ? (
        <AlertTriangle className="size-3" />
      ) : (
        <CheckCircle2 className="size-3" />
      )}
      {name}
    </span>
  );
}

function parseUrlTokens(url: string): Array<
  | { kind: "text"; value: string }
  | { kind: "variable"; value: string; name: string }
> {
  const tokens: Array<
    | { kind: "text"; value: string }
    | { kind: "variable"; value: string; name: string }
  > = [];
  const pattern = /\{\{([^}]+)\}\}/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(url))) {
    if (match.index > lastIndex) {
      tokens.push({ kind: "text", value: url.slice(lastIndex, match.index) });
    }
    tokens.push({ kind: "variable", value: match[0], name: match[1] });
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < url.length) {
    tokens.push({ kind: "text", value: url.slice(lastIndex) });
  }
  return tokens.length ? tokens : [{ kind: "text", value: "" }];
}

function methodColor(method: string) {
  const normalized = method.toUpperCase();
  if (normalized === "GET") {
    return {
      foreground: "var(--app-get)",
      background: "var(--app-get-bg)",
      border: "color-mix(in oklab, var(--app-get) 34%, transparent)",
    };
  }
  if (normalized === "POST") {
    return {
      foreground: "var(--app-post)",
      background: "var(--app-post-bg)",
      border: "color-mix(in oklab, var(--app-post) 34%, transparent)",
    };
  }
  if (normalized === "PUT") {
    return {
      foreground: "var(--app-put)",
      background: "var(--app-put-bg)",
      border: "color-mix(in oklab, var(--app-put) 34%, transparent)",
    };
  }
  if (normalized === "DELETE") {
    return {
      foreground: "var(--app-delete)",
      background: "var(--app-delete-bg)",
      border: "color-mix(in oklab, var(--app-delete) 34%, transparent)",
    };
  }
  if (normalized === "PATCH") {
    return {
      foreground: "var(--app-patch)",
      background: "var(--app-patch-bg)",
      border: "color-mix(in oklab, var(--app-patch) 34%, transparent)",
    };
  }
  return {
    foreground: "var(--app-text)",
    background: "var(--app-panel-2)",
    border: "var(--app-line)",
  };
}
