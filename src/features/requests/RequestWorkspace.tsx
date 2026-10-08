import { save } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Download } from "lucide-react";
import { lazy, Suspense } from "react";
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
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useDraftStore } from "@/features/workspace/draftStore";
import { useResponseStore } from "@/features/workspace/responseStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import { api } from "@/lib/tauri";
import type {
  KeyValue,
  RequestBody,
  RequestDetail,
  ResolvedRequestPreview,
  SendRequestResult,
} from "@/features/types";

const ResponseViewer = lazy(() =>
  import("@/features/requests/ResponseViewer").then((module) => ({
    default: module.ResponseViewer,
  })),
);

// Split into sections that each select only the draft fields they render,
// so a keystroke in one field does not re-render the rest of the workspace.
export function RequestWorkspace() {
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const hasDraft = useDraftStore((state) =>
    activeRequestId ? state.drafts.has(activeRequestId) : false,
  );

  if (!activeRequestId || !hasDraft) {
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
      <UrlBarSection requestId={activeRequestId} />
      <ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
        <ResizablePanel defaultSize="58%" minSize="34%">
          <RequestEditors requestId={activeRequestId} />
        </ResizablePanel>
        <ResizableHandle className="bg-[var(--app-line)]" />
        <ResizablePanel defaultSize="42%" minSize="24%">
          <ResponsePane requestId={activeRequestId} />
        </ResizablePanel>
      </ResizablePanelGroup>
    </section>
  );
}

const NO_UNRESOLVED: string[] = [];
const NO_VALUES: ResolvedRequestPreview["variableValues"] = {};

function useDraftValue<T>(
  requestId: string,
  select: (request: RequestDetail) => T,
): T | undefined {
  return useDraftStore((state) => {
    const draft = state.drafts.get(requestId);
    return draft && select(draft);
  });
}

function UrlBarSection({ requestId }: { requestId: string }) {
  const name = useDraftValue(requestId, (request) => request.name);
  const method = useDraftValue(requestId, (request) => request.method);
  const url = useDraftValue(requestId, (request) => request.url);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  const sendActiveRequest = useWorkspaceStore(
    (state) => state.sendActiveRequest,
  );
  const saveActiveRequest = useWorkspaceStore(
    (state) => state.saveActiveRequest,
  );
  const sending = useWorkspaceStore((state) =>
    state.sendingRequestIds.has(requestId),
  );
  const saving = useWorkspaceStore((state) => state.saving);
  const dirty = useWorkspaceStore(
    (state) =>
      state.tabs.find((tab) => tab.requestId === requestId)?.dirty ?? false,
  );
  const unresolved =
    useDraftStore(
      (state) => state.previews.get(requestId)?.unresolvedVariables,
    ) ?? NO_UNRESOLVED;
  const variableValues =
    useDraftStore((state) => state.previews.get(requestId)?.variableValues) ??
    NO_VALUES;

  if (name === undefined || method === undefined || url === undefined) {
    return null;
  }

  return (
    <div className="border-b border-[var(--app-line)] bg-[var(--app-bg)] px-4 py-3">
      <RequestUrlBar
        request={{ name, method, url }}
        sending={sending}
        saving={saving}
        dirty={dirty}
        unresolvedKeys={unresolved}
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
            {unresolved.map((key) => `{{${key}}}`).join(", ")}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// Radix tabs mount only the active panel, so only that editor subscribes.
function RequestEditors({ requestId }: { requestId: string }) {
  const activeEditorTab = useWorkspaceUiStore(
    (state) => state.workspaceUi.requestEditorTabs[requestId] ?? "params",
  );
  const setRequestEditorTab = useWorkspaceUiStore(
    (state) => state.setRequestEditorTab,
  );

  return (
    <Tabs
      value={activeEditorTab}
      onValueChange={(value) =>
        setRequestEditorTab(requestId, value as typeof activeEditorTab)
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
        <ParamsEditor requestId={requestId} />
      </TabsContent>
      <TabsContent value="headers" className="min-h-0 flex-1 p-0">
        <HeadersEditor requestId={requestId} />
      </TabsContent>
      <TabsContent value="auth" className="min-h-0 flex-1 p-0">
        <AuthSection requestId={requestId} />
      </TabsContent>
      <TabsContent value="body" className="min-h-0 flex-1 p-0">
        <BodySection requestId={requestId} />
      </TabsContent>
      <TabsContent value="variables" className="min-h-0 flex-1 p-0">
        <VariablesSection requestId={requestId} />
      </TabsContent>
    </Tabs>
  );
}

function ParamsEditor({ requestId }: { requestId: string }) {
  const pathParams = useDraftValue(requestId, (request) => request.pathParams);
  const query = useDraftValue(requestId, (request) => request.query);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  if (!pathParams || !query) return null;

  return (
    <ScrollArea className="h-full">
      <div className="space-y-5 p-4">
        <ParameterSection
          title="Path Parameters"
          rows={pathParams}
          onChange={(pathParams) => updateRequest({ pathParams })}
          placeholder="Path parameter"
          empty="No path parameters detected. Use :name in the URL path."
        />
        <ParameterSection
          title="Query Parameters"
          rows={query}
          onChange={(query) => updateRequest({ query })}
          placeholder="Query parameter"
          empty="No query parameters detected. Use ?name=value in the URL."
        />
      </div>
    </ScrollArea>
  );
}

function HeadersEditor({ requestId }: { requestId: string }) {
  const headers = useDraftValue(requestId, (request) => request.headers);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  if (!headers) return null;

  return (
    <ScrollArea className="h-full">
      <div className="p-4">
        <KeyValueTable
          rows={headers}
          onChange={(headers) => updateRequest({ headers })}
          placeholder="Header"
        />
      </div>
    </ScrollArea>
  );
}

function AuthSection({ requestId }: { requestId: string }) {
  const auth = useDraftValue(requestId, (request) => request.auth);
  const inheritedAuth = useDraftStore(
    (state) => state.previews.get(requestId)?.inheritedAuth,
  );
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);

  return (
    <ScrollArea className="h-full">
      <div className="p-4">
        <AuthEditor
          auth={auth ?? null}
          inheritedAuth={inheritedAuth}
          onChange={(auth) => updateRequest({ auth })}
        />
      </div>
    </ScrollArea>
  );
}

function BodySection({ requestId }: { requestId: string }) {
  const body = useDraftValue(requestId, (request) => request.body);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);

  return (
    <BodyEditor
      body={body ?? emptyBody()}
      onChange={(body) => updateRequest({ body })}
    />
  );
}

function VariablesSection({ requestId }: { requestId: string }) {
  const request = useDraftStore((state) => state.drafts.get(requestId));
  // Keyed so a request switch drops the previous request's unsaved edits.
  return request ? <VariableEditor key={requestId} request={request} /> : null;
}

function ResponsePane({ requestId }: { requestId: string }) {
  const response = useResponseStore((state) => state.responses.get(requestId));
  const sending = useWorkspaceStore((state) =>
    state.sendingRequestIds.has(requestId),
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 items-center justify-between border-b border-[var(--app-line)] px-4">
        <div className="flex items-center gap-2">
          <div className="app-mono text-xs font-semibold uppercase tracking-[0.08em] text-[var(--app-dim)]">
            Response
          </div>
          {response ? (
            <div className="text-xs">
              <span className="text-[var(--app-accent)]">
                {response.statusCode}
              </span>
              <span className="ml-2 text-[var(--app-dim)]">
                {response.durationMs} ms
              </span>
              {response.updatedVariables.length ? (
                <span className="ml-2 text-emerald-300">
                  {response.updatedVariables.length} variable
                  {response.updatedVariables.length === 1 ? "" : "s"} saved
                </span>
              ) : null}
              {response.variableWarnings.length ? (
                <span className="ml-2 text-amber-300">
                  {response.variableWarnings.length} variable warning
                  {response.variableWarnings.length === 1 ? "" : "s"}
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
          onClick={() => response && void downloadBody(response)}
        >
          <Download className="size-3.5" />
          Download
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        {response?.bodyFormat === "binary" && !sending ? (
          <BinaryResponse response={response} />
        ) : response ? (
          <Suspense fallback={<ResponsePlaceholder sending={sending} />}>
            <ResponseViewer sending={sending} value={response.body} />
          </Suspense>
        ) : (
          <ResponsePlaceholder sending={sending} />
        )}
      </div>
    </div>
  );
}

function ResponsePlaceholder({ sending }: { sending: boolean }) {
  return (
    <div className="h-full min-h-0 p-3 font-mono text-xs leading-5 text-[var(--app-dim)]">
      {sending ? "Sending request" : "Send a request to see the response."}
    </div>
  );
}

function BinaryResponse({ response }: { response: SendRequestResult }) {
  return (
    <div className="h-full min-h-0 min-w-0 p-3 [overflow-wrap:anywhere] font-mono text-xs leading-5 text-[var(--app-dim)]">
      Binary response ({formatBytes(response.bodyBytes)}
      {response.bodyContentType ? `, ${response.bodyContentType}` : ""}). Use
      Download to save {response.downloadFileName}.
    </div>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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
        <KeyValueTable
          rows={rows}
          onChange={onChange}
          placeholder={placeholder}
        />
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
    graphql: { query: "", variables: "" },
    file: { path: null, contentType: null },
  } satisfies RequestBody;
}

// Saves only the response body.
// Binary bodies never reach the webview, so Rust writes those bytes itself.
async function downloadBody(response: SendRequestResult) {
  const path = await save({ defaultPath: response.downloadFileName });
  if (!path) return;
  try {
    if (response.bodyFormat === "binary") {
      await api.saveResponseBody(response.historyId, path);
    } else {
      await api.saveTextFile(path, response.body);
    }
  } catch (error) {
    useWorkspaceStore.getState().setError(error);
  }
}
