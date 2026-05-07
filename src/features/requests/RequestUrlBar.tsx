import { Save, SendHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { RequestDetail } from "@/features/types";

const methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function RequestUrlBar({
  request,
  sending,
  saving,
  dirty,
  lastSavedAt,
  onChange,
  onSend,
  onSave,
}: {
  request: RequestDetail;
  sending: boolean;
  saving: boolean;
  dirty: boolean;
  lastSavedAt?: number;
  onChange: (patch: Partial<RequestDetail>) => void;
  onSend: () => void;
  onSave: () => void;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      <Input
        className="h-8 w-48 border-border/70 bg-background/40 text-sm font-medium"
        value={request.name}
        onChange={(event) => onChange({ name: event.target.value })}
      />
      <Select
        value={request.method}
        onValueChange={(method) => onChange({ method })}
      >
        <SelectTrigger className="h-8 w-[104px] border-border/70 bg-background/40 font-mono text-xs">
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
      <Input
        data-url-input
        className="h-8 min-w-0 flex-1 border-border/70 bg-background/40 font-mono text-xs"
        value={request.url}
        spellCheck={false}
        onChange={(event) => onChange({ url: event.target.value })}
      />
      <Button
        variant="ghost"
        size="icon"
        className="size-8 shrink-0"
        onClick={onSave}
        title="Save request"
      >
        <Save className="size-4" />
      </Button>
      <div className="w-20 text-xs text-muted-foreground">
        {saving ? (
          "Saving..."
        ) : dirty ? (
          <span className="text-violet-200">Unsaved</span>
        ) : lastSavedAt ? (
          <span className="text-emerald-300">Saved</span>
        ) : (
          "Saved"
        )}
      </div>
      <Button
        className="h-8 shrink-0 gap-1.5 bg-violet-500 px-3 text-xs text-white hover:bg-violet-400"
        onClick={onSend}
        disabled={sending}
      >
        <SendHorizontal className="size-3.5" />
        Send
      </Button>
    </div>
  );
}
