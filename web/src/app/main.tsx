import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./ErrorBoundary";
import { initializePreferences, persistence } from "../state/config";
import { startRuntime } from "../runtime/start";
import "@radix-ui/themes/styles.css";
import "../styles/global.css";

initializePreferences();
createRoot(document.getElementById("overlay")!).render(
  <ErrorBoundary>
    <App />
  </ErrorBoundary>,
);
startRuntime(document.getElementById("arena") as HTMLCanvasElement);
window.addEventListener("pagehide", () => persistence.flush());
