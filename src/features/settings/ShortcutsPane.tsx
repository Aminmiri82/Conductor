import { SectionTitle } from "@/features/settings/settingsShared";

export function ShortcutsPane() {
  const groups: { title: string; rows: [string, string[]][] }[] = [
    {
      title: "Navigation",
      rows: [
        ["Toggle sidebar", ["⌘", "B"]],
        ["Open request", ["⌘", "O"]],
        ["Open settings", ["⌘", ","]],
        ["Focus URL", ["⌘", "L"]],
      ],
    },
    {
      title: "Requests",
      rows: [
        ["New request", ["⌘", "N"]],
        ["Send", ["⌘", "↵"]],
        ["Save", ["⌘", "S"]],
        ["Duplicate tab", ["⌘", "D"]],
        ["Close tab", ["⌘", "W"]],
      ],
    },
  ];

  return (
    <div>
      <SectionTitle title="Keyboard Shortcuts" sub="Core app shortcuts." />
      {groups.map((group) => (
        <div key={group.title} className="mb-5">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.09em] text-[var(--app-dim)]">
            {group.title}
          </div>
          <div
            className="overflow-hidden border"
            style={{
              borderColor: "var(--app-line)",
              borderRadius: "var(--app-radius-lg)",
            }}
          >
            {group.rows.map(([label, keys], index) => (
              <div
                key={label}
                className="flex h-10 items-center border-b border-[var(--app-line)] px-3 last:border-b-0"
              >
                <div className="flex-1 text-sm">{label}</div>
                <div className="flex gap-1">
                  {keys.map((key) => (
                    <kbd
                      key={key}
                      className="app-mono min-w-6 rounded border border-[var(--app-line)] bg-[var(--app-panel-2)] px-1.5 py-0.5 text-center text-[11px] text-[var(--app-text)]"
                    >
                      {key}
                    </kbd>
                  ))}
                </div>
                <span className="sr-only">{index}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
