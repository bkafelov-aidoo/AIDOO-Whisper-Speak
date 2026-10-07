import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import Overlay from "./Overlay";
import LiveDiagnosticsWindow from "./LiveDiagnosticsWindow";
import "./styles.css";

const requestedWindow = new URLSearchParams(window.location.search).get("window");
const isOverlay = requestedWindow === "overlay";
const isLiveDiagnostics = requestedWindow === "live-diagnostics";
const windowKind = isOverlay ? "overlay" : isLiveDiagnostics ? "live-diagnostics" : "main";
document.documentElement.dataset.window = windowKind;
document.body.dataset.window = windowKind;

createRoot(document.getElementById("root")!).render(
  <StrictMode>{isOverlay ? <Overlay /> : isLiveDiagnostics ? <LiveDiagnosticsWindow /> : <App />}</StrictMode>,
);
