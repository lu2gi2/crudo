use chrono::Local;

/// Formats the current local system time:
/// Format: HH:MM AM/PM
/// Example: "02:50 PM"
pub fn current_system_time() -> String {
    let now = Local::now();
    now.format("%I:%M %p").to_string()
}

/// Formats the current local system date:
/// Format: DD/MM/YYYY
/// Example: "10/09/2026"
pub fn current_system_date() -> String {
    let now = Local::now();
    now.format("%d/%m/%Y").to_string()
}

/// Formats the current local system time and date according to the specification:
/// Time format: HH:MM AM/PM
/// Date format: DD/MM/YYYY
/// Example: "11:07 AM 10/09/2026"
pub fn current_system_time_formatted() -> String {
    let now = Local::now();
    now.format("%I:%M %p %d/%m/%Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_format_structure() {
        let formatted = current_system_time_formatted();
        let parts: Vec<&str> = formatted.split_whitespace().collect();
        assert_eq!(
            parts.len(),
            3,
            "Expected 3 whitespace-separated parts: HH:MM, AM/PM, DD/MM/YYYY"
        );
        assert!(parts[0].contains(':'), "First part should be HH:MM");
        assert!(
            parts[1] == "AM" || parts[1] == "PM",
            "Second part should be AM or PM"
        );
        assert_eq!(
            parts[2].matches('/').count(),
            2,
            "Third part should have two slashes: DD/MM/YYYY"
        );
    }

    #[test]
    fn test_time_and_date_standalone_formats() {
        let t = current_system_time();
        assert!(t.contains(':'), "Time must contain colon");
        assert!(
            t.ends_with("AM") || t.ends_with("PM"),
            "Time must end with AM or PM"
        );

        let d = current_system_date();
        assert_eq!(
            d.matches('/').count(),
            2,
            "Date must have format DD/MM/YYYY"
        );
    }
}
