import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
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
import type {
  RequestDetail,
  ResolvedRequestPreview,
  UrlDisplayMode,
} from "@/features/types";
import { parseVariableTokens } from "@/features/variables/variableTokens";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";

// `null` is a secret, whose value Rust does not send. A name that is not a
// key has no value from the last resolve (it was typed since).
type VariableValues = Partial<ResolvedRequestPreview["variableValues"]>;

const methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function RequestUrlBar({
  request,
  sending,
  saving,
  dirty,
  unresolvedKeys,
  variableValues,
  onChange,
  onSend,
  onSave,
}: {
  request: Pick<RequestDetail, "name" | "method" | "url">;
  sending: boolean;
  saving: boolean;
  dirty: boolean;
  unresolvedKeys: string[];
  variableValues: VariableValues;
  onChange: (patch: Partial<RequestDetail>) => void;
  onSend: () => void;
  onSave: () => void;
}) {
  const urlMode = useWorkspaceUiStore(
    (state) => state.workspaceUi.urlDisplayMode,
  );

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
          className="method-field app-mono h-8 w-[104px] border text-xs font-bold"
          data-method={request.method}
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
        variableValues={variableValues}
        onChange={(url) => onChange({ url })}
      />
      <Button
        variant="ghost"
        size="icon"
        className={`size-8 shrink-0 border hover:text-[var(--app-text)] ${
          dirty
            ? "border-[var(--app-accent)] text-[var(--app-accent)]"
            : "border-[var(--app-line)] text-[var(--app-dim)]"
        }`}
        style={{ borderRadius: "var(--app-radius)" }}
        onClick={onSave}
        title={saving ? "Saving request" : dirty ? "Unsaved changes" : "Saved"}
        disabled={saving}
      >
        <Save className={`size-4 ${saving ? "opacity-60" : ""}`} />
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
  variableValues,
  onChange,
}: {
  value: string;
  mode: UrlDisplayMode;
  unresolvedKeys: string[];
  variableValues: VariableValues;
  onChange: (value: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [editing, setEditing] = useState(mode === "flat");
  const tokens = useMemo(() => parseVariableTokens(value), [value]);
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
      className="app-mono flex h-8 min-w-0 flex-1 items-center overflow-visible border bg-[var(--app-panel-2)] px-2.5 text-left text-xs"
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
              resolvedValue={valueOf(variableValues, token.name)}
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
  resolvedValue,
}: {
  name: string;
  mode: UrlDisplayMode;
  unresolved: boolean;
  resolvedValue?: string | null;
}) {
  if (mode === "hybrid") {
    return (
      <span className="group mx-0.5 inline-flex align-middle">
        <span className="group-hover:hidden">
          <span className="text-[var(--app-dim)]">{"{{"}</span>
          <span
            className={
              unresolved ? "text-amber-300" : "text-[var(--app-accent)]"
            }
          >
            {name}
          </span>
          <span className="text-[var(--app-dim)]">{"}}"}</span>
        </span>
        <span className="hidden group-hover:inline-flex">
          <VariableChip
            name={name}
            unresolved={unresolved}
            resolvedValue={resolvedValue}
          />
        </span>
      </span>
    );
  }

  if (mode === "syntax") {
    return (
      <span className="group/var relative">
        <span className="text-[var(--app-dim)]">{"{{"}</span>
        <span
          className={unresolved ? "text-amber-300" : "text-[var(--app-accent)]"}
        >
          {name}
        </span>
        <span className="text-[var(--app-dim)]">{"}}"}</span>
        <VariableTooltip
          name={name}
          unresolved={unresolved}
          resolvedValue={resolvedValue}
        />
      </span>
    );
  }

  return (
    <VariableChip
      name={name}
      unresolved={unresolved}
      resolvedValue={resolvedValue}
    />
  );
}

function VariableChip({
  name,
  unresolved,
  resolvedValue,
}: {
  name: string;
  unresolved: boolean;
  resolvedValue?: string | null;
}) {
  return (
    <span
      className="group/var relative mx-0.5 inline-flex items-center gap-1 border px-1.5 py-0.5 font-medium"
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
      <VariableTooltip
        name={name}
        unresolved={unresolved}
        resolvedValue={resolvedValue}
      />
    </span>
  );
}

function VariableTooltip({
  name,
  unresolved,
  resolvedValue,
}: {
  name: string;
  unresolved: boolean;
  resolvedValue?: string | null;
}) {
  return (
    <span className="pointer-events-none absolute left-0 top-[calc(100%+6px)] z-50 hidden max-w-[520px] whitespace-nowrap border border-[var(--app-line)] bg-[var(--app-panel)] px-2 py-1 text-[11px] font-normal text-[var(--app-text)] shadow-lg group-hover/var:block">
      <span className="text-[var(--app-dim)]">{name}</span>
      <span className="px-1 text-[var(--app-dim)]">=</span>
      <span>{unresolved ? "undefined" : describeValue(resolvedValue)}</span>
    </span>
  );
}

function describeValue(value: string | null | undefined) {
  if (value === undefined) return "resolved value unavailable";
  if (value === null) return "secret";
  return value === "" ? "(empty)" : value;
}

// A plain object also answers for names like `toString`, so only a string or
// null counts as a value.
function valueOf(values: VariableValues, name: string) {
  const value = values[name];
  return typeof value === "string" || value === null ? value : undefined;
}
