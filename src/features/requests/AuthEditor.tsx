import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { AuthConfig } from "@/features/types";

export function AuthEditor({
  auth,
  inheritedAuth,
  onChange,
}: {
  auth: AuthConfig;
  inheritedAuth?: AuthConfig | null;
  onChange: (auth: AuthConfig) => void;
}) {
  const selectedType = auth.authType;

  return (
    <div className="max-w-2xl space-y-3">
      <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
        <label className="text-xs text-muted-foreground">Type</label>
        <Select
          value={selectedType}
          onValueChange={(authType) => {
            if (authType === "inherit") {
              onChange({ authType });
              return;
            }
            onChange({ ...emptyAuth(authType), ...auth, authType });
          }}
        >
          <SelectTrigger className="h-8 border-border/70 bg-background/40 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="inherit">Inherit from parent</SelectItem>
            <SelectItem value="noauth">No auth</SelectItem>
            <SelectItem value="bearer">Bearer token</SelectItem>
            <SelectItem value="basic">Basic auth</SelectItem>
            <SelectItem value="apikey">API key</SelectItem>
          </SelectContent>
        </Select>
      </div>
      {selectedType === "inherit" ? (
        <div className="rounded-md border border-border/70 bg-background/30 p-3 text-xs text-muted-foreground">
          {inheritedAuth ? (
            <>
              Inherited <span className="text-foreground">{labelFor(inheritedAuth)}</span>
            </>
          ) : (
            "No parent auth is configured."
          )}
        </div>
      ) : null}
      {selectedType === "bearer" ? (
        <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
          <label className="text-xs text-muted-foreground">Token</label>
          <Input
            className="h-8 border-border/70 bg-background/40 font-mono text-xs"
            value={auth.token ?? ""}
            placeholder="{{accessToken}}"
            onChange={(event) => onChange({ ...auth, token: event.target.value })}
          />
        </div>
      ) : null}
      {selectedType === "basic" ? (
        <>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Username</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.username ?? ""}
              onChange={(event) => onChange({ ...auth, username: event.target.value })}
            />
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Password</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.password ?? ""}
              type="password"
              onChange={(event) => onChange({ ...auth, password: event.target.value })}
            />
          </div>
        </>
      ) : null}
      {selectedType === "apikey" ? (
        <>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Add to</label>
            <Select
              value={auth.addTo ?? "header"}
              onValueChange={(addTo) => onChange({ ...auth, addTo })}
            >
              <SelectTrigger className="h-8 border-border/70 bg-background/40 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="header">Header</SelectItem>
                <SelectItem value="query">Query param</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Key</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.key ?? ""}
              placeholder="Authorization"
              onChange={(event) => onChange({ ...auth, key: event.target.value })}
            />
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Value</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.value ?? ""}
              placeholder="Bearer {{token}}"
              onChange={(event) => onChange({ ...auth, value: event.target.value })}
            />
          </div>
        </>
      ) : null}
    </div>
  );
}

function emptyAuth(authType: string): AuthConfig {
  if (authType === "apikey") {
    return { authType, key: "Authorization", value: "", addTo: "header" };
  }
  return { authType };
}

function labelFor(auth: AuthConfig) {
  if (auth.authType === "bearer") return "Bearer token";
  if (auth.authType === "basic") return "Basic auth";
  if (auth.authType === "apikey") return `API key (${auth.addTo ?? "header"})`;
  if (auth.authType === "noauth") return "No auth";
  return auth.authType;
}
