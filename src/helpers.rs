use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, NaiveDate};
use serde_json::{Map, Value, json};

use crate::models::EventDateTime;

pub fn build_event_patch_payload(
    summary: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    description: Option<&str>,
    location: Option<&str>,
) -> Result<Map<String, Value>> {
    let mut payload = Map::new();

    if let Some(summary) = summary {
        payload.insert("summary".to_string(), json!(summary));
    }

    if let Some(start) = start {
        payload.insert(
            "start".to_string(),
            serde_json::to_value(parse_event_time(start, false)?)?,
        );
    }

    if let Some(end) = end {
        payload.insert(
            "end".to_string(),
            serde_json::to_value(parse_event_time(end, true)?)?,
        );
    }

    if let Some(description) = description {
        payload.insert("description".to_string(), json!(description));
    }

    if let Some(location) = location {
        payload.insert("location".to_string(), json!(location));
    }

    Ok(payload)
}

/// Parse a user-supplied time string into an EventDateTime.
///
/// Accepts either RFC3339 (datetime with timezone) or YYYY-MM-DD (date-only).
/// For date-only inputs, no timezone conversion is applied. This is intentional:
/// Google Calendar treats date-only events as timezone-agnostic all-day events,
/// so the bare date string is correct regardless of the user's local timezone.
pub fn parse_event_time(input: &str, end_of_all_day_event: bool) -> Result<EventDateTime> {
    if let Ok(date_time) = DateTime::parse_from_rfc3339(input) {
        return Ok(EventDateTime {
            date_time: Some(date_time.to_rfc3339()),
            date: None,
        });
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .with_context(|| format!("failed to parse '{input}' as RFC3339 or YYYY-MM-DD"))?;

    let date = if end_of_all_day_event {
        date.checked_add_signed(Duration::days(1))
            .ok_or_else(|| anyhow!("date overflow while adjusting all-day event end date"))?
    } else {
        date
    };

    Ok(EventDateTime {
        date_time: None,
        date: Some(date.format("%Y-%m-%d").to_string()),
    })
}

pub fn prompt_select(prompt: &str, options: &[String]) -> Result<Option<String>> {
    let selection = dialoguer::FuzzySelect::new()
        .with_prompt(prompt)
        .items(options)
        .interact_opt()?;
    Ok(selection.map(|i| options[i].clone()))
}

pub fn prompt_yes_no_quit(message: &str) -> Result<Option<bool>> {
    let items = &["yes", "no", "quit"];
    let selection = dialoguer::Select::new()
        .with_prompt(message)
        .items(items)
        .default(0)
        .interact()?;
    match selection {
        0 => Ok(Some(true)),
        1 => Ok(Some(false)),
        _ => Ok(None),
    }
}

pub fn prompt_confirm(message: &str) -> Result<bool> {
    Ok(dialoguer::Confirm::new()
        .with_prompt(message)
        .default(false)
        .interact()?)
}

#[cfg(test)]
mod tests {
    use super::{build_event_patch_payload, parse_event_time};

    #[test]
    fn parses_rfc3339_time() {
        let event_time = parse_event_time("2026-03-24T12:30:00+02:00", false).unwrap();
        assert_eq!(
            event_time.date_time.as_deref(),
            Some("2026-03-24T12:30:00+02:00")
        );
        assert_eq!(event_time.date, None);
    }

    #[test]
    fn adjusts_end_date_for_all_day_event() {
        let event_time = parse_event_time("2026-03-24", true).unwrap();
        assert_eq!(event_time.date_time, None);
        assert_eq!(event_time.date.as_deref(), Some("2026-03-25"));
    }

    #[test]
    fn builds_sparse_patch_payload() {
        let payload = build_event_patch_payload(
            Some("New summary"),
            None,
            Some("2026-03-24"),
            None,
            Some("Office"),
        )
        .unwrap();

        assert_eq!(payload["summary"], "New summary");
        assert_eq!(payload["location"], "Office");
        assert_eq!(payload["end"]["date"], "2026-03-25");
        assert!(payload.get("start").is_none());
        assert!(payload.get("description").is_none());
    }
}
