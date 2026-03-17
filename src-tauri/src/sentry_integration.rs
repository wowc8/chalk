use sentry::ClientInitGuard;

/// Placeholder DSN — replace with your real Sentry DSN in production.
const SENTRY_DSN: &str = "https://examplePublicKey@o0.ingest.sentry.io/0";

/// Initialize Sentry if the user has given crash-reporting consent.
/// Returns `Some(guard)` if initialized, `None` if consent was not given.
/// The guard must be held for the lifetime of the application.
pub fn init_if_consented(consent_given: bool) -> Option<ClientInitGuard> {
    if !consent_given {
        tracing::info!("Sentry disabled: user has not given crash-reporting consent");
        return None;
    }

    let guard = sentry::init((
        SENTRY_DSN,
        sentry::ClientOptions {
            release: Some(std::borrow::Cow::Borrowed(env!("CARGO_PKG_VERSION"))),
            environment: if cfg!(debug_assertions) {
                Some("development".into())
            } else {
                Some("production".into())
            },
            // Strip PII: do not send user IPs, disable default PII.
            send_default_pii: false,
            // Auto-capture panics via the panic integration (included by default).
            auto_session_tracking: true,
            sample_rate: 1.0,
            ..Default::default()
        },
    ));

    tracing::info!("Sentry initialized for crash reporting");
    Some(guard)
}

/// Capture a user-submitted feedback report via Sentry.
pub fn send_user_report(message: &str) {
    sentry::capture_message(message, sentry::Level::Info);
    tracing::info!("User report sent to Sentry");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_without_consent_returns_none() {
        let guard = init_if_consented(false);
        assert!(guard.is_none());
    }

    #[test]
    fn test_init_with_consent_returns_guard() {
        // With a dummy DSN, Sentry will init but won't send anything
        let guard = init_if_consented(true);
        assert!(guard.is_some());
        // Drop the guard immediately so it doesn't interfere with other tests
        drop(guard);
    }
}
