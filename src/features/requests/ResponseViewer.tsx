import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Search, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

type JsonRow = {
  id: string;
  depth: number;
  key?: string;
  kind: "open" | "close" | "primitive" | "text";
  value?: unknown;
  summary?: string;
  closing?: string;
  foldable?: boolean;
};

export function ResponseViewer({
  value,
  sending,
}: {
  value?: unknown;
  sending: boolean;
}) {
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeMatch, setActiveMatch] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const rowRefs = useRef<Record<string, HTMLDivElement | null>>({});

  const rows = useMemo(() => buildRows(value, collapsed), [collapsed, value]);
  const matches = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return [];
    return rows
      .filter((row) => rowText(row).toLowerCase().includes(normalized))
      .map((row) => row.id);
  }, [query, rows]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setSearchOpen(true);
        requestAnimationFrame(() => inputRef.current?.focus());
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!matches.length) {
      setActiveMatch(0);
      return;
    }
    const next = Math.min(activeMatch, matches.length - 1);
    setActiveMatch(next);
    rowRefs.current[matches[next]]?.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
  }, [activeMatch, matches]);

  if (sending) {
    return (
      <div className="p-3 font-mono text-xs leading-5 text-muted-foreground">
        Sending request
      </div>
    );
  }

  if (value === undefined) {
    return (
      <div className="p-3 font-mono text-xs leading-5 text-muted-foreground">
        Send a request to see the response.
      </div>
    );
  }

  const activeMatchId = matches[activeMatch];

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      {searchOpen ? (
        <div className="absolute right-2 top-2 z-10 flex h-8 w-[360px] items-center gap-1 rounded-md border border-border/70 bg-[#141319]/95 px-1.5 shadow-md">
          <Search className="size-3.5 text-muted-foreground" />
          <Input
            ref={inputRef}
            className="h-6 flex-1 border-0 bg-transparent px-1 font-mono text-xs shadow-none focus-visible:ring-0"
            value={query}
            placeholder="Find in response"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setSearchOpen(false);
              }
            }}
          />
          <div className="w-12 text-center text-[11px] text-muted-foreground">
            {query ? `${matches.length ? activeMatch + 1 : 0}/${matches.length}` : ""}
          </div>
          <Button variant="ghost" size="icon" className="size-7" onClick={() => goToMatch(-1)}>
            <ChevronDown className="size-3.5 rotate-180" />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" onClick={() => goToMatch(1)}>
            <ChevronDown className="size-3.5" />
          </Button>
          <Button variant="ghost" size="icon" className="size-7" onClick={() => setSearchOpen(false)}>
            <X className="size-3.5" />
          </Button>
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="min-h-full w-max min-w-full py-2 font-mono text-xs leading-5">
          {rows.map((row) => (
            <div
              key={row.id}
              ref={(element) => {
                rowRefs.current[row.id] = element;
              }}
              className={cn(
                "flex min-h-5 items-start whitespace-pre px-3",
                row.id === activeMatchId && "bg-violet-400/15",
              )}
              style={{ paddingLeft: 12 + row.depth * 18 }}
            >
              {row.foldable ? (
                <button
                  className="mr-1 mt-0.5 grid size-4 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent"
                  onClick={() => toggle(row.id)}
                >
                  {collapsed.has(row.id) ? (
                    <ChevronRight className="size-3" />
                  ) : (
                    <ChevronDown className="size-3" />
                  )}
                </button>
              ) : (
                <span className="mr-1 size-4 shrink-0" />
              )}
              <JsonRowContent row={row} query={query} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );

  function toggle(id: string) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function goToMatch(delta: number) {
    if (!matches.length) return;
    setActiveMatch((current) => (current + delta + matches.length) % matches.length);
  }
}

function JsonRowContent({ row, query }: { row: JsonRow; query: string }) {
  if (row.kind === "close") {
    return <span className="text-muted-foreground">{row.closing}</span>;
  }
  if (row.kind === "text") {
    return <Highlighted text={String(row.value ?? "")} query={query} className="text-muted-foreground" />;
  }

  return (
    <span className="min-w-0">
      {row.key ? (
        <>
          <Highlighted text={`"${row.key}"`} query={query} className="text-violet-200" />
          <span className="text-muted-foreground">: </span>
        </>
      ) : null}
      {row.kind === "open" ? (
        <Highlighted text={row.summary ?? ""} query={query} className="text-muted-foreground" />
      ) : (
        <PrimitiveValue value={row.value} query={query} />
      )}
    </span>
  );
}

function PrimitiveValue({ value, query }: { value: unknown; query: string }) {
  if (typeof value === "string") {
    return <Highlighted text={JSON.stringify(value)} query={query} className="text-emerald-300" />;
  }
  if (typeof value === "number") {
    return <Highlighted text={String(value)} query={query} className="text-sky-300" />;
  }
  if (typeof value === "boolean") {
    return <Highlighted text={String(value)} query={query} className="text-amber-300" />;
  }
  if (value === null) {
    return <Highlighted text="null" query={query} className="text-muted-foreground" />;
  }
  return <Highlighted text={String(value)} query={query} className="text-muted-foreground" />;
}

function Highlighted({
  text,
  query,
  className,
}: {
  text: string;
  query: string;
  className: string;
}) {
  const normalized = query.trim();
  if (!normalized) return <span className={className}>{text}</span>;
  const index = text.toLowerCase().indexOf(normalized.toLowerCase());
  if (index === -1) return <span className={className}>{text}</span>;

  return (
    <span className={className}>
      {text.slice(0, index)}
      <mark className="rounded bg-violet-400/35 text-foreground">
        {text.slice(index, index + normalized.length)}
      </mark>
      {text.slice(index + normalized.length)}
    </span>
  );
}

function buildRows(value: unknown, collapsed: Set<string>) {
  if (!isJsonValue(value)) {
    return [{ id: "text", depth: 0, kind: "text", value: String(value) } satisfies JsonRow];
  }
  const rows: JsonRow[] = [];
  appendRows(rows, value, "root", 0);
  return rows;

  function appendRows(rows: JsonRow[], current: unknown, id: string, depth: number, key?: string) {
    if (Array.isArray(current)) {
      rows.push({
        id,
        depth,
        key,
        kind: "open",
        summary: collapsed.has(id) ? `[ ... ${current.length} ]` : "[",
        foldable: true,
      });
      if (!collapsed.has(id)) {
        current.forEach((item, index) => appendRows(rows, item, `${id}.${index}`, depth + 1));
        rows.push({ id: `${id}.close`, depth, kind: "close", closing: "]" });
      }
      return;
    }
    if (current && typeof current === "object") {
      const entries = Object.entries(current as Record<string, unknown>);
      rows.push({
        id,
        depth,
        key,
        kind: "open",
        summary: collapsed.has(id) ? `{ ... ${entries.length} }` : "{",
        foldable: true,
      });
      if (!collapsed.has(id)) {
        entries.forEach(([childKey, childValue]) =>
          appendRows(rows, childValue, `${id}.${childKey}`, depth + 1, childKey),
        );
        rows.push({ id: `${id}.close`, depth, kind: "close", closing: "}" });
      }
      return;
    }
    rows.push({ id, depth, key, kind: "primitive", value: current });
  }
}

function isJsonValue(value: unknown) {
  return value === null || ["string", "number", "boolean", "object"].includes(typeof value);
}

function rowText(row: JsonRow) {
  if (row.kind === "close") return row.closing ?? "";
  if (row.kind === "open") return `${row.key ?? ""} ${row.summary ?? ""}`;
  return `${row.key ?? ""} ${String(row.value ?? "")}`;
}
