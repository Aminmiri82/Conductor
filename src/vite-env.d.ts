/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "1" in perf harness builds (scripts/perf-harness.mjs). */
  readonly VITE_PERF_HARNESS?: string;
}
