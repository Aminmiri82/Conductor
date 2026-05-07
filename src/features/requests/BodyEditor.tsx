import { open } from "@tauri-apps/plugin-dialog";
import { FileUp, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import type { BodyField, RequestBody } from "@/features/types";
import { KeyValueTable } from "@/features/requests/KeyValueTable";

export function BodyEditor({
  body,
  onChange,
}: {
  body: RequestBody;
  onChange: (body: RequestBody) => void;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 items-center gap-2 border-b border-border/50 px-3">
        <Select value={body.mode} onValueChange={(mode) => onChange({ ...body, mode })}>
          <SelectTrigger className="h-8 w-44 border-border/70 bg-background/40 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="none">No body</SelectItem>
            <SelectItem value="raw">Raw</SelectItem>
            <SelectItem value="formdata">Form data</SelectItem>
            <SelectItem value="urlencoded">URL encoded</SelectItem>
          </SelectContent>
        </Select>
        {body.mode === "raw" ? (
          <Select
            value={body.rawLanguage ?? "json"}
            onValueChange={(rawLanguage) => onChange({ ...body, rawLanguage })}
          >
            <SelectTrigger className="h-8 w-32 border-border/70 bg-background/40 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="json">JSON</SelectItem>
              <SelectItem value="text">Text</SelectItem>
              <SelectItem value="xml">XML</SelectItem>
            </SelectContent>
          </Select>
        ) : null}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        {body.mode === "raw" ? (
          <Textarea
            className="min-h-[420px] resize-none rounded-none border-0 bg-transparent p-3 font-mono text-xs leading-5 shadow-none focus-visible:ring-0"
            value={body.raw}
            spellCheck={false}
            onChange={(event) => onChange({ ...body, raw: event.target.value })}
          />
        ) : null}
        {body.mode === "formdata" ? (
          <div className="p-3">
            <FormDataEditor
              rows={body.formData}
              onChange={(formData) => onChange({ ...body, formData })}
            />
          </div>
        ) : null}
        {body.mode === "urlencoded" ? (
          <div className="p-3">
            <KeyValueTable
              rows={body.urlencoded}
              onChange={(urlencoded) => onChange({ ...body, urlencoded })}
              placeholder="Field"
            />
          </div>
        ) : null}
        {body.mode === "none" ? (
          <div className="p-8 text-center text-xs text-muted-foreground">
            This request does not send a body.
          </div>
        ) : null}
      </ScrollArea>
    </div>
  );
}

function FormDataEditor({
  rows,
  onChange,
}: {
  rows: BodyField[];
  onChange: (rows: BodyField[]) => void;
}) {
  function update(index: number, patch: Partial<BodyField>) {
    onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  async function chooseFile(index: number) {
    const selected = await open({ multiple: false });
    if (typeof selected === "string") {
      update(index, { filePath: selected });
    }
  }

  return (
    <div className="overflow-hidden rounded-md border border-border/70">
      <div className="grid h-8 grid-cols-[34px_minmax(110px,0.7fr)_96px_minmax(180px,1.3fr)_34px] items-center border-b border-border/70 bg-muted/20 px-1 text-xs text-muted-foreground">
        <div />
        <div>Key</div>
        <div>Type</div>
        <div>Value</div>
        <div />
      </div>
      {rows.map((row, index) => (
        <div
          key={index}
          className="grid grid-cols-[34px_minmax(110px,0.7fr)_96px_minmax(180px,1.3fr)_34px] items-center border-b border-border/40 px-1 last:border-b-0"
        >
          <input
            type="checkbox"
            className="mx-auto size-3 accent-violet-400"
            checked={row.enabled}
            onChange={(event) => update(index, { enabled: event.target.checked })}
          />
          <Input
            className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
            value={row.key}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <Select
            value={row.fieldType || "text"}
            onValueChange={(fieldType) =>
              update(index, {
                fieldType,
                value: fieldType === "file" ? "" : row.value,
                filePath: null,
              })
            }
          >
            <SelectTrigger className="h-7 border-border/50 bg-background/30 text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="text">Text</SelectItem>
              <SelectItem value="file">File</SelectItem>
            </SelectContent>
          </Select>
          {row.fieldType === "file" ? (
            <Button
              variant="ghost"
              className="h-8 justify-start truncate px-2 font-mono text-xs text-muted-foreground"
              onClick={() => void chooseFile(index)}
            >
              <FileUp className="mr-2 size-3.5 shrink-0" />
              <span className="truncate">{row.filePath || "Choose file"}</span>
            </Button>
          ) : (
            <Input
              className="h-8 rounded-none border-0 bg-transparent font-mono text-xs shadow-none focus-visible:ring-0"
              value={row.value}
              onChange={(event) => update(index, { value: event.target.value })}
            />
          )}
          <Button
            variant="ghost"
            size="icon"
            className="size-7 text-muted-foreground"
            onClick={() => onChange(rows.filter((_, i) => i !== index))}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      ))}
      <div className="p-1">
        <Button
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 text-xs text-muted-foreground"
          onClick={() =>
            onChange([
              ...rows,
              { key: "", value: "", enabled: true, fieldType: "text", filePath: null },
            ])
          }
        >
          <Plus className="size-3.5" />
          Add field
        </Button>
      </div>
    </div>
  );
}
