import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { api } from "@/lib/tauri";
import type { RequestDetail, VariableEntry } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function VariableEditor({ request }: { request: RequestDetail }) {
  const [scope, setScope] = useState<"request" | "collection" | "global">("request");
  const [variables, setVariables] = useState<VariableEntry[]>([]);
  const [savedAt, setSavedAt] = useState<number>();
  const [saving, setSaving] = useState(false);
  const resolveActiveRequest = useWorkspaceStore((state) => state.resolveActiveRequest);

  const scopeId =
    scope === "request"
      ? request.id
      : scope === "collection"
        ? request.collectionId
        : "global";

  useEffect(() => {
    let mounted = true;
    void api.listVariables(scope, scopeId).then((items) => {
      if (mounted) setVariables(items);
    });
    return () => {
      mounted = false;
    };
  }, [scope, scopeId]);

  async function save() {
    setSaving(true);
    await api.saveVariables(
      scope,
      scopeId,
      variables.map((variable) => ({ ...variable, scopeKind: scope, scopeId })),
    );
    setSaving(false);
    setSavedAt(Date.now());
    await resolveActiveRequest();
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 items-center justify-between border-b border-border/50 px-3">
        <div className="flex rounded-md border border-border/70 bg-background/30 p-0.5">
          {(["request", "collection", "global"] as const).map((item) => (
            <button
              key={item}
              className={`h-6 rounded px-2 text-xs capitalize ${
                scope === item
                  ? "bg-violet-500 text-white"
                  : "text-muted-foreground hover:text-foreground"
              }`}
              onClick={() => setScope(item)}
            >
              {item}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 text-right text-xs text-muted-foreground">
            {saving ? "Saving..." : savedAt ? "Saved" : ""}
          </span>
          <Button className="h-7 gap-1.5 bg-violet-500 px-2 text-xs text-white" onClick={save}>
            <Save className="size-3.5" />
            Save
          </Button>
        </div>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="p-3">
          <div className="overflow-hidden rounded-md border border-border/70">
            <div className="grid h-8 grid-cols-[34px_minmax(140px,0.8fr)_minmax(160px,1.2fr)_90px] items-center border-b border-border/70 bg-muted/20 px-1 text-xs text-muted-foreground">
              <div />
              <div>Key</div>
              <div>Value</div>
              <div>Sensitive</div>
            </div>
            {variables.map((variable, index) => (
              <div
                key={index}
                className="grid grid-cols-[34px_minmax(140px,0.8fr)_minmax(160px,1.2fr)_90px] items-center border-b border-border/40 px-1 last:border-b-0"
              >
                <input
                  type="checkbox"
                  className="mx-auto size-3 accent-violet-400"
                  checked={variable.enabled}
                  onChange={(event) =>
                    update(index, { enabled: event.target.checked })
                  }
                />
                <Input
                  className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
                  value={variable.key}
                  onChange={(event) => update(index, { key: event.target.value })}
                />
                <Input
                  className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
                  value={variable.value}
                  type={variable.sensitive ? "password" : "text"}
                  onChange={(event) =>
                    update(index, { value: event.target.value })
                  }
                />
                <input
                  type="checkbox"
                  className="mx-auto size-3 accent-violet-400"
                  checked={variable.sensitive}
                  onChange={(event) =>
                    update(index, { sensitive: event.target.checked })
                  }
                />
              </div>
            ))}
            <div className="p-1">
              <Button
                variant="ghost"
                size="sm"
                className="h-7 text-xs text-muted-foreground"
                onClick={() =>
                  setVariables([
                    ...variables,
                    {
                      scopeKind: scope,
                      scopeId,
                      key: "",
                      value: "",
                      enabled: true,
                      sensitive: false,
                    },
                  ])
                }
              >
                Add variable
              </Button>
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );

  function update(index: number, patch: Partial<VariableEntry>) {
    setVariables((current) =>
      current.map((variable, i) =>
        i === index ? { ...variable, ...patch } : variable,
      ),
    );
  }
}
