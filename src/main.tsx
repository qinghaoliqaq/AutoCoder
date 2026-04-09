import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Toaster } from "sonner";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
      <Toaster
        position="bottom-right"
        theme="system"
        richColors
        closeButton
        toastOptions={{
          className: 'text-sm',
          duration: 5000,
        }}
      />
    </ErrorBoundary>
  </React.StrictMode>,
);
