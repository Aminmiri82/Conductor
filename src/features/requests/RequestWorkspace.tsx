import { save } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Download } from "lucide-react";
import { useMemo } from "react";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { RequestUrlBar } from "@/features/requests/RequestUrlBar";
import { KeyValueTable } from "@/features/requests/KeyValueTable";
import { BodyEditor } from "@/features/requests/BodyEditor";
import { AuthEditor } from "@/features/requests/AuthEditor";
import { VariableEditor } from "@/features/variables/VariableEditor";
import { RequestTabsBar } from "@/features/requests/RequestTabsBar";
import { ResponseViewer } from "@/features/requests/ResponseViewer";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { api } from "@/lib/tauri";
import type { KeyValue } from "@/features/types";

export function RequestWorkspace() {
  const request = useWorkspaceStore((state) =>
    state.activeRequestId
      ? state.requestDraftsById[state.activeRequestId]
      : undefined,
  );
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  const sendActiveRequest = useWorkspaceStore((state) => state.sendActiveRequest);
  const saveActiveRequest = useWorkspaceStore((state) => state.saveActiveRequest);
  const sending = useWorkspaceStore((state) => state.sending);
  const saving = useWorkspaceStore((state) => state.saving);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const activeTab = useWorkspaceStore((state) =>
    state.tabs.find((tab) => tab.requestId === state.activeRequestId),
  );
  const activeEditorTab = useWorkspaceStore((state) =>
    request ? (state.workspaceUi.requestEditorTabs[request.id] ?? "params") : "params",
  );
  const setRequestEditorTab = useWorkspaceStore(
    (state) => state.setRequestEditorTab,
  );
  const response = useWorkspaceStore((state) => state.response);
  const preview = useWorkspaceStore((state) => state.resolvedPreview);
  const unresolved = preview?.unresolvedVariables ?? [];
  const variableValues = useMemo(
    () => (request ? resolveUrlVariableValues(request.url, preview?.url) : {}),
    [preview?.url, request],
  );

  if (!request) {
    return (
      <section className="flex h-full items-center justify-center bg-[var(--app-bg)]">
        <div className="max-w-sm text-center">
          <div className="text-sm font-medium">No request selected</div>
          <div className="mt-2 text-xs leading-5 text-[var(--app-dim)]">
            Import a collection and choose a request from the sidebar.
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="flex h-full min-h-0 flex-col bg-[var(--app-bg)]">
      <RequestTabsBar />
      <div className="border-b border-[var(--app-line)] bg-[var(--app-bg)] px-4 py-3">
        <RequestUrlBar
          request={request}
          sending={sending}
          saving={saving}
          dirty={Boolean(activeRequestId && activeTab?.dirty)}
          unresolvedKeys={unresolved.map((item) => item.key)}
          variableValues={variableValues}
          onChange={updateRequest}
          onSend={() => void sendActiveRequest()}
          onSave={() => void saveActiveRequest()}
        />
        {unresolved.length ? (
          <div className="mt-2 flex items-start gap-2 rounded-md border border-amber-400/20 bg-amber-400/10 px-2 py-1.5 text-xs text-amber-200">
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
            <div className="min-w-0">
              <span className="font-medium">Unresolved variables: </span>
              {unresolved.map((item) => `{{${item.key}}}`).join(", ")}
            </div>
          </div>
        ) : null}
      </div>

      <ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
        <ResizablePanel defaultSize="58%" minSize="34%">
          <Tabs
            value={activeEditorTab}
            onValueChange={(value) =>
              setRequestEditorTab(request.id, value as typeof activeEditorTab)
            }
            className="flex h-full min-h-0 flex-col"
          >
            <div className="border-b border-[var(--app-line)] px-4 pt-3">
              <TabsList className="h-8 bg-transparent p-0">
                <TabsTrigger value="params">Params</TabsTrigger>
                <TabsTrigger value="headers">Headers</TabsTrigger>
                <TabsTrigger value="auth">Auth</TabsTrigger>
                <TabsTrigger value="body">Body</TabsTrigger>
                <TabsTrigger value="variables">Variables</TabsTrigger>
              </TabsList>
            </div>
            <TabsContent value="params" className="min-h-0 flex-1 p-0">
              <ScrollArea className="h-full">
                <div className="space-y-5 p-4">
                  <ParameterSection
                    title="Path Parameters"
                    rows={request.pathParams}
                    onChange={(pathParams) => updateRequest({ pathParams })}
                    placeholder="Path parameter"
                    empty="No path parameters detected. Use :name in the URL path."
                  />
                  <ParameterSection
                    title="Query Parameters"
                    rows={request.query}
                    onChange={(query) => updateRequest({ query })}
                    placeholder="Query parameter"
                    empty="No query parameters detected. Use ?name=value in the URL."
                  />
                </div>
              </ScrollArea>
            </TabsContent>
            <TabsContent value="headers" className="min-h-0 flex-1 p-0">
              <ScrollArea className="h-full">
                <div className="p-4">
                  <KeyValueTable
                    rows={request.headers}
                    onChange={(headers) => updateRequest({ headers })}
                    placeholder="Header"
                  />
                </div>
              </ScrollArea>
            </TabsContent>
            <TabsContent value="auth" className="min-h-0 flex-1 p-0">
              <ScrollArea className="h-full">
                <div className="p-4">
                  <AuthEditor
                    auth={request.auth ?? { authType: "inherit" }}
                    inheritedAuth={request.inheritedAuth}
                    onChange={(auth) =>
                      updateRequest({ auth: auth.authType === "inherit" ? null : auth })
                    }
                  />
                </div>
              </ScrollArea>
            </TabsContent>
            <TabsContent value="body" className="min-h-0 flex-1 p-0">
              <BodyEditor
                body={request.body ?? emptyBody()}
                onChange={(body) => updateRequest({ body })}
              />
            </TabsContent>
            <TabsContent value="variables" className="min-h-0 flex-1 p-0">
              <VariableEditor request={request} />
            </TabsContent>
          </Tabs>
        </ResizablePanel>
        <ResizableHandle className="bg-[var(--app-line)]" />
        <ResizablePanel defaultSize="42%" minSize="24%">
          <div className="flex h-full min-h-0 flex-col">
            <div className="flex h-10 items-center justify-between border-b border-[var(--app-line)] px-4">
              <div className="flex items-center gap-2">
                <div className="app-mono text-xs font-semibold uppercase tracking-[0.08em] text-[var(--app-dim)]">
                  Response
                </div>
                {response ? (
                  <div className="text-xs">
                    <span className="text-[var(--app-accent)]">{response.statusCode}</span>
                    <span className="ml-2 text-[var(--app-dim)]">
                      {response.durationMs} ms
                    </span>
                    {response.updatedVariables.length ? (
                      <span className="ml-2 text-emerald-300">
                        {response.updatedVariables.length} variable
                        {response.updatedVariables.length === 1 ? "" : "s"} saved
                      </span>
                    ) : null}
                  </div>
                ) : null}
              </div>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 gap-1.5 text-xs"
                disabled={!response}
                onClick={() => response && void downloadJson(response)}
              >
                <Download className="size-3.5" />
                JSON
              </Button>
            </div>
            <div className="min-h-0 flex-1">
              <ResponseViewer
                sending={sending}
                value={response ? (response.bodyJson ?? response.bodyText) : undefined}
              />
            </div>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </section>
  );
}

function ParameterSection({
  title,
  rows,
  onChange,
  placeholder,
  empty,
}: {
  title: string;
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  placeholder: string;
  empty: string;
}) {
  return (
    <section>
      <div className="app-mono mb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-[var(--app-dim)]">
        {title}
      </div>
      {rows.length ? (
        <KeyValueTable rows={rows} onChange={onChange} placeholder={placeholder} />
      ) : (
        <div
          className="border px-3 py-3 text-xs text-[var(--app-dim)]"
          style={{
            borderColor: "var(--app-line)",
            borderRadius: "var(--app-radius-lg)",
          }}
        >
          {empty}
        </div>
      )}
    </section>
  );
}

function emptyBody() {
  return {
    mode: "none",
    raw: "",
    rawLanguage: "json",
    formData: [],
    urlencoded: [],
  };
}

function resolveUrlVariableValues(rawUrl: string, resolvedUrl: string | undefined) {
  if (!resolvedUrl) return {};
  const tokens = parseUrlTokens(rawUrl);
  const variableNames = tokens
    .filter((token): token is { kind: "variable"; name: string } => token.kind === "variable")
    .map((token) => token.name);
  if (!variableNames.length) return {};

  const pattern = new RegExp(
    `^${tokens
      .map((token) =>
        token.kind === "text" ? escapeRegExp(token.value) : "(.*?)",
      )
      .join("")}$`,
  );
  const match = resolvedUrl.match(pattern);
  if (!match) return {};

  return Object.fromEntries(
    variableNames.map((name, index) => [name, match[index + 1] ?? ""]),
  );
}

function parseUrlTokens(url: string): Array<
  { kind: "text"; value: string } | { kind: "variable"; name: string }
> {
  const tokens: Array<
    { kind: "text"; value: string } | { kind: "variable"; name: string }
  > = [];
  const pattern = /\{\{([^}]+)\}\}/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(url))) {
    if (match.index > lastIndex) {
      tokens.push({ kind: "text", value: url.slice(lastIndex, match.index) });
    }
    tokens.push({ kind: "variable", name: match[1] });
    lastIndex = match.index + match[0].length;
  }
  if (lastIndex < url.length) {
    tokens.push({ kind: "text", value: url.slice(lastIndex) });
  }
  return tokens;
}

function escapeRegExp(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function downloadJson(value: unknown) {
  const path = await save({
    defaultPath: "conductor-response.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) return;
  const content = JSON.stringify(value, null, 2);
  await api.saveTextFile(path, content);
}
