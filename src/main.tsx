import React from "react";
import ReactDOM from "react-dom/client";
import { SentryErrorBoundary } from "./components/SentryErrorBoundary";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SentryErrorBoundary>
      <App />
    </SentryErrorBoundary>
  </React.StrictMode>,
);
