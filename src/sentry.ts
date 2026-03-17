import * as Sentry from "@sentry/react";

const SENTRY_DSN = "https://examplePublicKey@o0.ingest.sentry.io/0";

let initialized = false;

export function initSentry() {
  if (initialized) return;

  Sentry.init({
    dsn: SENTRY_DSN,
    environment: import.meta.env.DEV ? "development" : "production",
    // Strip PII: no user data sent by default
    sendDefaultPii: false,
    // Reasonable sample rate
    tracesSampleRate: 0.2,
    beforeSend(event) {
      // Strip any PII from breadcrumbs/extra data
      if (event.extra) {
        delete event.extra["student_data"];
        delete event.extra["document_content"];
        delete event.extra["oauth_token"];
      }
      return event;
    },
  });

  initialized = true;
}

export function isSentryInitialized(): boolean {
  return initialized;
}

export { Sentry };
