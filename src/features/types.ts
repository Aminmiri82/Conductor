export type KeyValue = {
  key: string;
  value: string;
  enabled: boolean;
};

export type EntityId = number;

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
  id: EntityId;
  collectionId: EntityId;
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
  activeCollectionId?: EntityId;
  activeEnvironmentId?: EntityId | null;
  appTheme: AppTheme;
  accentColor: string;
  urlDisplayMode: UrlDisplayMode;
  settingsTab: SettingsTab;
  requestEditorTabs: Record<string, RequestEditorTab>;
};

export type CollectionSummary = {
  id: EntityId;
  name: string;
  source: string;
  updatedAt: string;
};

export type EnvironmentSummary = {
  id: EntityId;
  name: string;
  updatedAt: string;
};

export type CollectionNode = {
  id: EntityId;
  collectionId: EntityId;
  parentId?: EntityId | null;
  position: number;
  kind: "folder" | "request";
  name: string;
  requestId?: EntityId | null;
  method?: string | null;
  children: CollectionNode[];
};

export type CreateRequestResult = {
  requestId: EntityId;
  nodeId: EntityId;
};

export type DuplicateRequestResult = CreateRequestResult;

export type VariableEntry = {
  scope: "global" | "collection" | "environment";
  collectionId?: EntityId | null;
  environmentId?: EntityId | null;
  key: string;
  value: string;
  initialValue?: string | null;
  enabled: boolean;
  sensitive: boolean;
  variableType?: string | null;
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
  historyId: EntityId;
  statusCode: number;
  statusText: string;
  durationMs: number;
  headers: ResponseHeader[];
  body: string;
  bodyBytes: number;
  bodyContentType?: string | null;
  bodyFormat: "json" | "text" | string;
  updatedVariables: KeyValue[];
  variableWarnings: string[];
  unresolvedVariables: UnresolvedVariable[];
};

export type DatabaseStatus = {
  path: string;
  schemaVersion: number;
  collectionCount: number;
  requestCount: number;
  learnedRowidEnabled: boolean;
  learnedRowidTableMovetoCalls: number;
  learnedRowidAttempted: number;
  learnedRowidFallback: number;
  learnedRowidExactFirstProbe: number;
  learnedRowidComparisons: number;
  learnedRowidModelCount: number;
  learnedRowidModelSegments: number;
  learnedRowidModelBytes: number;
  learnedRowidModelPredictions: number;
  learnedRowidPredictedSlotAverage: number;
  learnedRowidPredictionErrorAverage: number;
  learnedRowidPredictionErrorMax: number;
};

export type LearnedRowidStatsSnapshot = {
  tableMovetoCalls: number;
  attempted: number;
  fallback: number;
  exactFirstProbe: number;
  comparisons: number;
  modelCount: number;
  modelSegments: number;
  modelBytes: number;
  modelPredictions: number;
  predictedSlotAverage: number;
  predictionErrorAverage: number;
  predictionErrorMax: number;
};

export type LearnedRowidBenchmarkRun = {
  enabled: boolean;
  lookupCount: number;
  elapsedMicros: number;
  comparisonsPerLookup: number;
  stats: LearnedRowidStatsSnapshot;
};

export type LearnedRowidBenchmarkScenario = {
  name: string;
  rowCount: number;
  disabled: LearnedRowidBenchmarkRun;
  enabled: LearnedRowidBenchmarkRun;
  comparisonDelta: number;
  comparisonDeltaPercent: number;
};

export type LearnedRowidBenchmark = {
  scenarios: LearnedRowidBenchmarkScenario[];
  status: DatabaseStatus;
};
