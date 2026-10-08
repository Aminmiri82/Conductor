import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ApiKeyLocation, AuthConfig, AuthType } from "@/features/types";
import { pickOption } from "@/lib/options";

// No `auth` at all means the request inherits it from its parent.
const authChoices = [
  ["inherit", "Inherit from parent"],
  ["noauth", "No auth"],
  ["bearer", "Bearer token"],
  ["basic", "Basic auth"],
  ["apikey", "API key"],
] as const satisfies readonly (readonly [AuthType | "inherit", string])[];

const apiKeyLocations = [
  ["header", "Header"],
  ["query", "Query param"],
] as const satisfies readonly (readonly [ApiKeyLocation, string])[];

export function AuthEditor({
  auth,
  inheritedAuth,
  onChange,
}: {
  auth: AuthConfig | null;
  inheritedAuth?: AuthConfig | null;
  onChange: (auth: AuthConfig | null) => void;
}) {
  const selectedType = auth?.authType ?? "inherit";

  return (
    <div className="max-w-2xl space-y-3">
      <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
        <label className="text-xs text-muted-foreground">Type</label>
        <Select
          value={selectedType}
          onValueChange={(value) => {
            const choice = pickOption(authChoices, value);
            if (choice === "inherit") onChange(null);
            else if (choice) onChange(switchAuthType(auth, choice));
          }}
        >
          <SelectTrigger className="h-8 border-border/70 bg-background/40 text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {authChoices.map(([id, label]) => (
              <SelectItem key={id} value={id}>
                {label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {selectedType === "inherit" ? (
        <div className="rounded-md border border-border/70 bg-background/30 p-3 text-xs text-muted-foreground">
          {inheritedAuth ? (
            <>
              Inherited{" "}
              <span className="text-foreground">{labelFor(inheritedAuth)}</span>
            </>
          ) : (
            "No parent auth is configured."
          )}
        </div>
      ) : null}
      {auth?.authType === "bearer" ? (
        <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
          <label className="text-xs text-muted-foreground">Token</label>
          <Input
            className="h-8 border-border/70 bg-background/40 font-mono text-xs"
            value={auth.token ?? ""}
            placeholder="{{accessToken}}"
            onChange={(event) =>
              onChange({ ...auth, token: event.target.value })
            }
          />
        </div>
      ) : null}
      {auth?.authType === "basic" ? (
        <>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Username</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.username ?? ""}
              onChange={(event) =>
                onChange({ ...auth, username: event.target.value })
              }
            />
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Password</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.password ?? ""}
              type="password"
              onChange={(event) =>
                onChange({ ...auth, password: event.target.value })
              }
            />
          </div>
        </>
      ) : null}
      {auth?.authType === "apikey" ? (
        <>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Add to</label>
            <Select
              value={auth.addTo ?? "header"}
              onValueChange={(value) => {
                const addTo = pickOption(apiKeyLocations, value);
                if (addTo) onChange({ ...auth, addTo });
              }}
            >
              <SelectTrigger className="h-8 border-border/70 bg-background/40 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {apiKeyLocations.map(([id, label]) => (
                  <SelectItem key={id} value={id}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Key</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.key ?? ""}
              placeholder="Authorization"
              onChange={(event) =>
                onChange({ ...auth, key: event.target.value })
              }
            />
          </div>
          <div className="grid grid-cols-[140px_minmax(0,1fr)] items-center gap-3">
            <label className="text-xs text-muted-foreground">Value</label>
            <Input
              className="h-8 border-border/70 bg-background/40 font-mono text-xs"
              value={auth.value ?? ""}
              placeholder="Bearer {{token}}"
              onChange={(event) =>
                onChange({ ...auth, value: event.target.value })
              }
            />
          </div>
        </>
      ) : null}
    </div>
  );
}

// Keeps values typed under other types. Rust sends unset fields as null, so
// defaults fill per field instead of being spread underneath.
function switchAuthType(
  auth: AuthConfig | null,
  authType: AuthType,
): AuthConfig {
  const next = { ...auth, authType };
  if (authType === "apikey") {
    next.key ??= "Authorization";
    next.value ??= "";
    next.addTo ??= "header";
  }
  return next;
}

function labelFor(auth: AuthConfig) {
  switch (auth.authType) {
    case "bearer":
      return "Bearer token";
    case "basic":
      return "Basic auth";
    case "apikey":
      return `API key (${auth.addTo ?? "header"})`;
    case "noauth":
      return "No auth";
    case "unsupported":
      return "an unsupported auth type";
  }
}
