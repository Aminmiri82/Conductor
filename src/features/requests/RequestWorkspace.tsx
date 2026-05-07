import { save } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, Download } from "lucide-react";
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

export function RequestWorkspace() {
  const request = useWorkspaceStore((state) => state.activeRequest);
  const updateRequest = useWorkspaceStore((state) => state.updateRequest);
  const sendActiveRequest = useWorkspaceStore((state) => state.sendActiveRequest);
  const saveActiveRequest = useWorkspaceStore((state) => state.saveActiveRequest);
  const sending = useWorkspaceStore((state) => state.sending);
  const saving = useWorkspaceStore((state) => state.saving);
  const lastSavedAt = useWorkspaceStore((state) => state.lastSavedAt);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const activeTab = useWorkspaceStore((state) =>
    state.tabs.find((tab) => tab.requestId === state.activeRequestId),
  );
  const response = useWorkspaceStore((state) => state.response);
  const preview = useWorkspaceStore((state) => state.resolvedPreview);

  if (!request) {
    return (
      <section className="flex h-full items-center justify-center bg-[#101014]">
        <div className="max-w-sm text-center">
          <div className="text-sm font-medium">No request selected</div>
          <div className="mt-2 text-xs leading-5 text-muted-foreground">
            Import a collection and choose a request from the sidebar.
          </div>
        </div>
      </section>
    );
  }

  const unresolved = preview?.unresolvedVariables ?? [];

  return (
    <section className="flex h-full min-h-0 flex-col bg-[#101014]">
      <RequestTabsBar />
      <div className="border-b border-border/70 bg-[#141319] px-3 py-2">
        <RequestUrlBar
          request={request}
          sending={sending}
          saving={saving}
          dirty={Boolean(activeRequestId && activeTab?.dirty)}
          lastSavedAt={lastSavedAt}
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
          <Tabs defaultValue="params" className="flex h-full min-h-0 flex-col">
            <div className="border-b border-border/60 px-3 pt-2">
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
                <div className="p-3">
                  <KeyValueTable
                    rows={request.query}
                    onChange={(query) => updateRequest({ query })}
                    placeholder="Query parameter"
                  />
                </div>
              </ScrollArea>
            </TabsContent>
            <TabsContent value="headers" className="min-h-0 flex-1 p-0">
              <ScrollArea className="h-full">
                <div className="p-3">
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
                <div className="p-3">
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
        <ResizableHandle className="bg-border/70" />
        <ResizablePanel defaultSize="42%" minSize="24%">
          <div className="flex h-full min-h-0 flex-col">
            <div className="flex h-10 items-center justify-between border-b border-border/60 px-3">
              <div className="flex items-center gap-2">
                <div className="text-xs font-medium uppercase tracking-[0.08em] text-muted-foreground">
                  Response
                </div>
                {response ? (
                  <div className="text-xs">
                    <span className="text-violet-300">{response.statusCode}</span>
                    <span className="ml-2 text-muted-foreground">
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

function emptyBody() {
  return {
    mode: "none",
    raw: "",
    rawLanguage: "json",
    formData: [],
    urlencoded: [],
  };
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
