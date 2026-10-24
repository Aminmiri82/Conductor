/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" in perf harness builds (scripts/perf/harness.ts). */
  readonly VITE_PERF_HARNESS?: string;
}
