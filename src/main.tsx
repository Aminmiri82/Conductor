import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/app/App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// Perf harness builds only (`pnpm perf:run`). Vite replaces the flag with a
// constant, so every other build drops this branch and its chunk.
if (import.meta.env.VITE_PERF_HARNESS === "1") {
  void import("@/app/perfScenario").then(({ runPerfScenario }) =>
    runPerfScenario(),
  );
}
