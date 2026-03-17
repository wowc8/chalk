use crate::database::Database;

const CONSENT_KEY: &str = "crash_reporting_consent";
const CONSENT_SHOWN_KEY: &str = "privacy_consent_shown";

/// Check if the privacy consent dialog has been shown before.
pub fn has_seen_consent(db: &Database) -> bool {
    db.get_setting(CONSENT_SHOWN_KEY)
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Check if the user has opted into crash reporting.
pub fn is_crash_reporting_enabled(db: &Database) -> bool {
    db.get_setting(CONSENT_KEY)
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Record that the consent dialog was shown and save the user's choice.
pub fn save_consent(db: &Database, consented: bool) -> Result<(), String> {
    db.set_setting(CONSENT_SHOWN_KEY, "true")
        .map_err(|e| e.to_string())?;
    db.set_setting(CONSENT_KEY, if consented { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_flow() {
        let db = Database::open_in_memory().unwrap();

        // Initially: no consent shown, reporting disabled
        assert!(!has_seen_consent(&db));
        assert!(!is_crash_reporting_enabled(&db));

        // User opts in
        save_consent(&db, true).unwrap();
        assert!(has_seen_consent(&db));
        assert!(is_crash_reporting_enabled(&db));

        // User changes mind and opts out
        save_consent(&db, false).unwrap();
        assert!(has_seen_consent(&db));
        assert!(!is_crash_reporting_enabled(&db));
    }
}
