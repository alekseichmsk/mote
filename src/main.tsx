import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import SettingsApp from "./SettingsApp";
import "./styles.css";

let isSettings = false;
try {
  isSettings = getCurrentWindow().label === "settings";
} catch {
  isSettings = false;
}

if (!isSettings) {
  // Disable Chromium/WebView context menu on the overlay pet only.
  document.addEventListener("contextmenu", (e) => e.preventDefault());
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isSettings ? <SettingsApp /> : <App />}</React.StrictMode>,
);
