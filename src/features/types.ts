export type KeyValue = {
  key: string;
  value: string;
  enabled: boolean;
};

export type BodyField = KeyValue & {
  fieldType: "text" | "file" | string;
  filePath?: string | null;
  contentType?: string | null;
};

export type RequestBody = {
  mode: "none" | "raw" | "formdata" | "urlencoded" | "graphql" | "file" | string;
  raw: string;
  rawLanguage?: string | null;
  formData: BodyField[];
  urlencoded: KeyValue[];
  graphql?: {
    query: string;
    variables: string;
  } | null;
  file?: {
    path?: string | null;
    contentType?: string | null;
  } | null;
};

export type AuthConfig = {
  authType: "inherit" | "noauth" | "bearer" | "basic" | "apikey" | string;
  token?: string | null;
  username?: string | null;
  password?: string | null;
  key?: string | null;
  value?: string | null;
  addTo?: string | null;
};

export type RequestDetail = {
  id: string;
  collectionId: string;
  name: string;
  method: string;
  url: string;
  headers: KeyValue[];
  query: KeyValue[];
  pathParams: KeyValue[];
  auth?: AuthConfig | null;
  inheritedAuth?: AuthConfig | null;
  effectiveAuth?: AuthConfig | null;
  body?: RequestBody | null;
  preRequestScript?: unknown;
  testScript?: unknown;
  updatedAt: string;
};

export type RequestEditorTab = "params" | "headers" | "auth" | "body" | "variables";

export type AppTheme = "softpro" | "conductor" | "brutalist";

export type UrlDisplayMode = "flat" | "syntax" | "chip" | "hybrid";

export type SettingsTab = "appearance" | "variables" | "shortcuts" | "about";

export type WorkspaceUiState = {
  activeCollectionId?: string;
  appTheme: AppTheme;
  accentColor: string;
  urlDisplayMode: UrlDisplayMode;
  settingsTab: SettingsTab;
  requestEditorTabs: Record<string, RequestEditorTab>;
};

export type CollectionSummary = {
  id: string;
  name: string;
  source: string;
  updatedAt: string;
};

export type CollectionNode = {
  id: string;
  collectionId: string;
  parentId?: string | null;
  position: number;
  kind: "folder" | "request";
  name: string;
  requestId?: string | null;
  method?: string | null;
  children: CollectionNode[];
};

export type VariableEntry = {
  scope: "global" | "collection";
  collectionId?: string | null;
  key: string;
  value: string;
  enabled: boolean;
  sensitive: boolean;
};

export type UnresolvedVariable = {
  key: string;
  locations: string[];
};

export type ResponseHeader = {
  key: string;
  value: string;
};

export type ResolvedRequestPreview = {
  url: string;
  headers: KeyValue[];
  query: KeyValue[];
  body?: RequestBody | null;
  unresolvedVariables: UnresolvedVariable[];
};

export type SendRequestResult = {
  historyId: string;
  statusCode: number;
  statusText: string;
  durationMs: number;
  headers: ResponseHeader[];
  body: string;
  bodyBytes: number;
  bodyContentType?: string | null;
  bodyFormat: "json" | "text" | string;
  updatedVariables: KeyValue[];
  unresolvedVariables: UnresolvedVariable[];
};
