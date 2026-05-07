import { ChangeEvent, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  FileJson,
  Folder,
  FolderOpen,
  Import,
  MoreHorizontal,
  Plus,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { cn } from "@/lib/utils";
import type { CollectionNode } from "@/features/types";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

export function CollectionSidebar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const collections = useWorkspaceStore((state) => state.collections);
  const tree = useWorkspaceStore((state) => state.tree);
  const activeCollectionId = useWorkspaceStore((state) => state.activeCollectionId);
  const activeRequestId = useWorkspaceStore((state) => state.activeRequestId);
  const selectCollection = useWorkspaceStore((state) => state.selectCollection);
  const selectRequest = useWorkspaceStore((state) => state.selectRequest);
  const importCollection = useWorkspaceStore((state) => state.importCollection);
  const createRequestIn = useWorkspaceStore((state) => state.createRequestIn);
  const createFolderIn = useWorkspaceStore((state) => state.createFolderIn);
  const duplicateRequest = useWorkspaceStore((state) => state.duplicateRequest);
  const deleteRequest = useWorkspaceStore((state) => state.deleteRequest);
  const deleteNode = useWorkspaceStore((state) => state.deleteNode);
  const moveNode = useWorkspaceStore((state) => state.moveNode);
  const folders = flattenFolders(tree);

  async function onFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    await importCollection(text);
    event.target.value = "";
  }

  return (
    <aside className="flex h-full min-h-0 flex-col border-r border-border/40 bg-[#151419]">
      <div className="space-y-2 border-b border-border/60 p-3">
        <div className="flex items-center justify-between">
          <div className="text-xs font-medium uppercase tracking-[0.08em] text-muted-foreground">
            Collections
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="size-7"
            onClick={() => inputRef.current?.click()}
            title="Import Postman collection"
          >
            <Import className="size-4" />
          </Button>
          <input
            ref={inputRef}
            className="hidden"
            type="file"
            accept=".json,application/json"
            onChange={onFileChange}
          />
        </div>
        <div className="flex items-center gap-2">
          <Select
            value={activeCollectionId}
            onValueChange={(value) => void selectCollection(value)}
          >
            <SelectTrigger className="h-8 min-w-0 flex-1 border-border/70 bg-background/45 text-xs">
              <SelectValue placeholder="No collection" />
            </SelectTrigger>
            <SelectContent>
              {collections.map((collection) => (
                <SelectItem key={collection.id} value={collection.id}>
                  {collection.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 shrink-0"
                disabled={!activeCollectionId}
                title="Add to collection"
              >
                <Plus className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-40">
              <DropdownMenuItem onSelect={() => void createRequestIn(null, tree.length)}>
                Add request
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void createFolderIn(null, tree.length)}>
                Add folder
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="p-2">
          {tree.length ? (
            tree.map((node) => (
              <TreeNode
                key={node.id}
                node={node}
                depth={0}
                activeRequestId={activeRequestId}
                onSelectRequest={selectRequest}
                onCreateRequest={createRequestIn}
                onCreateFolder={createFolderIn}
                onDuplicateRequest={duplicateRequest}
                onDeleteRequest={deleteRequest}
                onDeleteNode={deleteNode}
                onMoveNode={moveNode}
                rootTree={tree}
                folders={folders}
              />
            ))
          ) : (
            <div className="px-2 py-8 text-center text-xs text-muted-foreground">
              Import a Postman collection to begin.
            </div>
          )}
        </div>
      </ScrollArea>
    </aside>
  );
}

function TreeNode({
  node,
  depth,
  activeRequestId,
  onSelectRequest,
  onCreateRequest,
  onCreateFolder,
  onDuplicateRequest,
  onDeleteRequest,
  onDeleteNode,
  onMoveNode,
  rootTree,
  folders,
}: {
  node: CollectionNode;
  depth: number;
  activeRequestId?: string;
  onSelectRequest: (requestId: string) => Promise<void>;
  onCreateRequest: (parentId: string | null | undefined, position: number) => Promise<void>;
  onCreateFolder: (parentId: string | null | undefined, position: number) => Promise<void>;
  onDuplicateRequest: (requestId: string) => Promise<void>;
  onDeleteRequest: (requestId: string) => Promise<void>;
  onDeleteNode: (node: CollectionNode) => Promise<void>;
  onMoveNode: (
    node: CollectionNode,
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  rootTree: CollectionNode[];
  folders: CollectionNode[];
}) {
  const [open, setOpen] = useState(depth < 1);
  const isFolder = node.kind === "folder";
  const isActive = node.requestId === activeRequestId;
  const siblings = siblingsForNode(rootTree, node);
  const parent = node.parentId ? findNode(rootTree, node.parentId) : undefined;
  const moveTargets = folders.filter(
    (folder) => folder.id !== node.id && !containsNode(folder, node.id),
  );

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            role="button"
            tabIndex={0}
            className={cn(
              "group flex h-7 w-full items-center gap-1.5 rounded-md pr-1 text-left text-xs text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground",
              isActive && "bg-primary/15 text-foreground ring-1 ring-primary/25",
            )}
            style={{ paddingLeft: 8 + depth * 13 }}
            onClick={() => {
              if (isFolder) {
                setOpen((value) => !value);
              } else if (node.requestId) {
                void onSelectRequest(node.requestId);
              }
            }}
          >
            {isFolder ? (
              open ? (
                <ChevronDown className="size-3.5 shrink-0" />
              ) : (
                <ChevronRight className="size-3.5 shrink-0" />
              )
            ) : (
              <span className="w-3.5 shrink-0" />
            )}
            {isFolder ? (
              open ? (
                <FolderOpen className="size-3.5 shrink-0 text-muted-foreground" />
              ) : (
                <Folder className="size-3.5 shrink-0 text-muted-foreground" />
              )
            ) : (
              <FileJson className="size-3.5 shrink-0 text-violet-300/80" />
            )}
            {!isFolder ? (
              <span className="w-10 shrink-0 rounded bg-violet-400/10 px-1 py-0.5 text-center font-mono text-[10px] leading-none text-violet-200">
                {node.method ?? "GET"}
              </span>
            ) : null}
            <span className="min-w-0 flex-1 truncate">{node.name}</span>
            {!isFolder && node.requestId ? (
              <RequestActionsMenu
                requestId={node.requestId}
                onDuplicateRequest={onDuplicateRequest}
                onDeleteRequest={onDeleteRequest}
                node={node}
                onMoveNode={onMoveNode}
                folders={moveTargets}
              />
            ) : null}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-44">
          {isFolder ? (
            <>
              <ContextMenuItem
                onSelect={() =>
                  void onCreateRequest(node.id, node.children.length)
                }
              >
                Add request
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() =>
                  void onCreateFolder(node.id, node.children.length)
                }
              >
                Add folder
              </ContextMenuItem>
              <ContextMenuSeparator />
              <MoveMenuItems
                node={node}
                siblings={siblings}
                parent={parent}
                folders={moveTargets}
                onMoveNode={onMoveNode}
              />
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteNode(node)}
              >
                Delete folder
              </ContextMenuItem>
            </>
          ) : node.requestId ? (
            <>
              <MoveMenuItems
                node={node}
                siblings={siblings}
                parent={parent}
                folders={moveTargets}
                onMoveNode={onMoveNode}
              />
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={() => void onDuplicateRequest(node.requestId!)}>
                Duplicate request
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                variant="destructive"
                onSelect={() => void onDeleteRequest(node.requestId!)}
              >
                Delete request
              </ContextMenuItem>
            </>
          ) : null}
        </ContextMenuContent>
      </ContextMenu>
      {isFolder && open
        ? node.children.map((child) => (
            <TreeNode
              key={child.id}
              node={child}
              depth={depth + 1}
              activeRequestId={activeRequestId}
              onSelectRequest={onSelectRequest}
              onCreateRequest={onCreateRequest}
              onCreateFolder={onCreateFolder}
              onDuplicateRequest={onDuplicateRequest}
              onDeleteRequest={onDeleteRequest}
              onDeleteNode={onDeleteNode}
              onMoveNode={onMoveNode}
              rootTree={rootTree}
              folders={folders}
            />
          ))
        : null}
    </div>
  );
}

function RequestActionsMenu({
  node,
  requestId,
  onDuplicateRequest,
  onDeleteRequest,
  onMoveNode,
  folders,
}: {
  node: CollectionNode;
  requestId: string;
  onDuplicateRequest: (requestId: string) => Promise<void>;
  onDeleteRequest: (requestId: string) => Promise<void>;
  onMoveNode: (
    node: CollectionNode,
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
  folders: CollectionNode[];
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          className="grid size-5 shrink-0 place-items-center rounded opacity-0 hover:bg-accent group-hover:opacity-100"
          onClick={(event) => {
            event.stopPropagation();
          }}
          title="Request actions"
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        {folders.length ? (
          <>
            {folders.slice(0, 8).map((folder) => (
              <DropdownMenuItem
                key={folder.id}
                onSelect={() => void onMoveNode(node, folder.id, folder.children.length)}
              >
                Move into {folder.name}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
          </>
        ) : null}
        <DropdownMenuItem onSelect={() => void onDuplicateRequest(requestId)}>
          Duplicate request
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          variant="destructive"
          onSelect={() => void onDeleteRequest(requestId)}
        >
          Delete request
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function MoveMenuItems({
  node,
  siblings,
  parent,
  folders,
  onMoveNode,
}: {
  node: CollectionNode;
  siblings: CollectionNode[];
  parent?: CollectionNode;
  folders: CollectionNode[];
  onMoveNode: (
    node: CollectionNode,
    parentId: string | null | undefined,
    position: number,
  ) => Promise<void>;
}) {
  return (
    <>
      <ContextMenuItem
        disabled={node.position <= 0}
        onSelect={() => void onMoveNode(node, node.parentId, node.position - 1)}
      >
        Move up
      </ContextMenuItem>
      <ContextMenuItem
        disabled={node.position >= siblings.length - 1}
        onSelect={() => void onMoveNode(node, node.parentId, node.position + 1)}
      >
        Move down
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!parent}
        onSelect={() =>
          void onMoveNode(node, parent?.parentId, (parent?.position ?? 0) + 1)
        }
      >
        Move out
      </ContextMenuItem>
      {folders.length ? (
        <ContextMenuSub>
          <ContextMenuSubTrigger>Move into folder</ContextMenuSubTrigger>
          <ContextMenuSubContent className="w-48">
            {folders.map((folder) => (
              <ContextMenuItem
                key={folder.id}
                onSelect={() => void onMoveNode(node, folder.id, folder.children.length)}
              >
                {folder.name}
              </ContextMenuItem>
            ))}
          </ContextMenuSubContent>
        </ContextMenuSub>
      ) : null}
    </>
  );
}

function flattenFolders(nodes: CollectionNode[]): CollectionNode[] {
  return nodes.flatMap((node) => [
    ...(node.kind === "folder" ? [node] : []),
    ...flattenFolders(node.children),
  ]);
}

function findNode(nodes: CollectionNode[], id: string): CollectionNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node;
    const child = findNode(node.children, id);
    if (child) return child;
  }
  return undefined;
}

function siblingsForNode(nodes: CollectionNode[], node: CollectionNode): CollectionNode[] {
  if (!node.parentId) return nodes;
  return findNode(nodes, node.parentId)?.children ?? [];
}

function containsNode(node: CollectionNode, id: string): boolean {
  return node.children.some((child) => child.id === id || containsNode(child, id));
}
