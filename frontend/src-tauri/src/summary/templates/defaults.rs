/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// Retrospective (Agile) template
pub const RETROSPECTIVE: &str = include_str!("../../../templates/retrospective.json");

/// Project sync / status update template
pub const PROJECT_SYNC: &str = include_str!("../../../templates/project_sync.json");

/// Psychiatric session note template
pub const PSYCHIATRIC_SESSION: &str = include_str!("../../../templates/psychatric_session.json");

/// Client / sales meeting template
pub const SALES_MARKETING: &str = include_str!("../../../templates/sales_marketing_client_call.json");

/// Audit report template for detailed analysis of long meetings
pub const AUDIT_REPORT: &str = include_str!("../../../templates/audit_report.json");

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("daily_standup", DAILY_STANDUP),
        ("standard_meeting", STANDARD_MEETING),
        ("retrospective", RETROSPECTIVE),
        ("project_sync", PROJECT_SYNC),
        ("psychatric_session", PSYCHIATRIC_SESSION),
        ("sales_marketing_client_call", SALES_MARKETING),
        ("audit_report", AUDIT_REPORT),
    ]
}

/// Get a built-in template by identifier
///
/// # Arguments
/// * `id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// The template JSON content if found, None otherwise
pub fn get_builtin_template(id: &str) -> Option<&'static str> {
    match id {
        "daily_standup" => Some(DAILY_STANDUP),
        "standard_meeting" => Some(STANDARD_MEETING),
        "retrospective" => Some(RETROSPECTIVE),
        "project_sync" => Some(PROJECT_SYNC),
        "psychatric_session" => Some(PSYCHIATRIC_SESSION),
        "sales_marketing_client_call" => Some(SALES_MARKETING),
        "audit_report" => Some(AUDIT_REPORT),
        _ => None,
    }
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    vec![
        "daily_standup",
        "standard_meeting",
        "retrospective",
        "project_sync",
        "psychatric_session",
        "sales_marketing_client_call",
        "audit_report",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_templates_valid_json() {
        for (id, content) in get_builtin_templates() {
            let result = serde_json::from_str::<serde_json::Value>(content);
            assert!(
                result.is_ok(),
                "Built-in template '{}' contains invalid JSON: {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("daily_standup").is_some());
        assert!(get_builtin_template("standard_meeting").is_some());
        assert!(get_builtin_template("retrospective").is_some());
        assert!(get_builtin_template("project_sync").is_some());
        assert!(get_builtin_template("psychatric_session").is_some());
        assert!(get_builtin_template("sales_marketing_client_call").is_some());
        assert!(get_builtin_template("audit_report").is_some());
        assert!(get_builtin_template("nonexistent").is_none());
    }
}
