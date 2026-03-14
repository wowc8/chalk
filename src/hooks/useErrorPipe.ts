import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Pipes unhandled frontend errors and unhandled promise rejections
 * to the Rust backend structured logging system.
 */
export function useErrorPipe() {
  useEffect(() => {
    const handleError = (event: ErrorEvent) => {
      invoke("log_frontend_error", {
        message: event.message,
        source: event.filename ?? null,
        line: event.lineno ?? null,
      }).catch(() => {});
    };

    const handleRejection = (event: PromiseRejectionEvent) => {
      const message =
        event.reason instanceof Error
          ? event.reason.message
          : String(event.reason);
      invoke("log_frontend_error", {
        message,
        source: null,
        line: null,
      }).catch(() => {});
    };

    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleRejection);

    return () => {
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleRejection);
    };
  }, []);
}
