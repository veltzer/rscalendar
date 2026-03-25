use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::NaiveDate;

use crate::cli::{CalendarNameArgs, ListArgs, ListFormat, OutputOptions};
use crate::client::GoogleCalendarClient;
use crate::config::Config;
use crate::fetch_events;
use crate::models::{EventDateTime, print_calendar, print_event};

// --- Filter helpers ---

/// Parse a date string (RFC3339 or YYYY-MM-DD) into a chrono DateTime<chrono::FixedOffset>.
/// For YYYY-MM-DD, midnight UTC is used.
pub fn parse_filter_date(s: &str) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }
    let nd = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("cannot parse '{s}' as RFC3339 or YYYY-MM-DD"))?;
    let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
    Ok(chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
        ndt,
        chrono::FixedOffset::east_opt(0).unwrap(),
    ))
}

/// Extract an event's start time as a DateTime for comparison.
pub fn event_start_to_datetime(edt: &EventDateTime) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    if let Some(ref dt_str) = edt.date_time {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
            return Some(dt);
        }
    }
    if let Some(ref d_str) = edt.date {
        if let Ok(nd) = NaiveDate::parse_from_str(d_str, "%Y-%m-%d") {
            let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
            return Some(chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
                ndt,
                chrono::FixedOffset::east_opt(0).unwrap(),
            ));
        }
    }
    None
}

// --- List command handler ---

pub async fn cmd_list(client: &GoogleCalendarClient, args: &ListArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let (_, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;

    // Parse filter dates once
    let starts_after = args.starts_after.as_deref().map(parse_filter_date)
        .transpose().context("invalid --starts-after date")?;
    let starts_before = args.starts_before.as_deref().map(parse_filter_date)
        .transpose().context("invalid --starts-before date")?;
    let search_lower = args.search.as_ref().map(|s| s.to_lowercase());
    let has_property = args.has_property.as_deref().map(|s| {
        if let Some((k, v)) = s.split_once('=') {
            (k.to_string(), Some(v.to_string()))
        } else {
            (s.to_string(), None)
        }
    });

    let filtered: Vec<_> = events.iter().filter(|event| {
        // --starts-after filter
        if let Some(ref after) = starts_after {
            if let Some(start) = &event.start {
                let event_dt = event_start_to_datetime(start);
                if let Some(evt) = event_dt {
                    if evt <= *after {
                        return false;
                    }
                }
            }
        }
        // --starts-before filter
        if let Some(ref before) = starts_before {
            if let Some(start) = &event.start {
                let event_dt = event_start_to_datetime(start);
                if let Some(evt) = event_dt {
                    if evt >= *before {
                        return false;
                    }
                }
            }
        }
        // --search filter
        if let Some(ref pattern) = search_lower {
            let summary_match = event.summary.as_ref()
                .is_some_and(|s| s.to_lowercase().contains(pattern));
            let desc_match = event.description.as_ref()
                .is_some_and(|d| d.to_lowercase().contains(pattern));
            if !summary_match && !desc_match {
                return false;
            }
        }
        // --has-property filter
        if let Some((ref key, ref val)) = has_property {
            let shared = event.extended_properties.as_ref()
                .and_then(|p| p.shared.as_ref());
            match val {
                Some(expected) => {
                    if !shared.is_some_and(|s| s.get(key).is_some_and(|v| v == expected)) {
                        return false;
                    }
                }
                None => {
                    if !shared.is_some_and(|s| s.contains_key(key)) {
                        return false;
                    }
                }
            }
        }
        true
    }).collect();

    if args.count {
        println!("{}", filtered.len());
    } else if filtered.is_empty() {
        if !out.quiet { println!("No events found."); }
    } else {
        match args.format {
            ListFormat::Table => {
                println!("{:<30} {:<12} {:<12} {:<15} {:<15}",
                    "SUMMARY", "START", "END", "TYPE", "CLIENT");
                println!("{}", "-".repeat(84));
                for event in &filtered {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let end = event.end_str();
                    let shared = event.extended_properties.as_ref()
                        .and_then(|p| p.shared.as_ref());
                    let type_val = shared.and_then(|s| s.get("type"))
                        .map(|s| s.as_str()).unwrap_or("");
                    let client_val = shared.and_then(|s| s.get("client"))
                        .map(|s| s.as_str()).unwrap_or("");
                    // Truncate long values
                    let trunc = |s: &str, max: usize| -> String {
                        if s.len() > max { format!("{}...", &s[..max-3]) } else { s.to_string() }
                    };
                    println!("{:<30} {:<12} {:<12} {:<15} {:<15}",
                        trunc(summary, 30),
                        trunc(&start, 12),
                        trunc(&end, 12),
                        trunc(type_val, 15),
                        trunc(client_val, 15));
                }
            }
            ListFormat::Default => {
                for event in &filtered {
                    print_event(event, out.show_builtin, out.json);
                }
            }
        }
    }

    Ok(())
}

// --- ListCalendars handler ---

pub async fn cmd_list_calendars(client: &GoogleCalendarClient, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    if calendars.is_empty() {
        if !out.quiet { println!("No calendars found."); }
    } else {
        for cal in &calendars {
            print_calendar(cal, out.json);
        }
    }
    Ok(())
}

// --- Stats handler ---

pub async fn cmd_stats(client: &GoogleCalendarClient, args: &CalendarNameArgs, config: &Config, _out: &OutputOptions) -> Result<()> {
    let (_, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;

    println!("Total events: {}", events.len());

    let mut by_type: BTreeMap<String, u32> = BTreeMap::new();
    let mut by_client: BTreeMap<String, u32> = BTreeMap::new();
    let mut by_month: BTreeMap<String, u32> = BTreeMap::new();

    for event in &events {
        let shared = event.extended_properties.as_ref()
            .and_then(|p| p.shared.as_ref());
        let type_val = shared.and_then(|s| s.get("type"))
            .cloned().unwrap_or_else(|| "(no type)".to_string());
        let client_val = shared.and_then(|s| s.get("client"))
            .cloned().unwrap_or_else(|| "(no client)".to_string());

        *by_type.entry(type_val).or_insert(0) += 1;
        *by_client.entry(client_val).or_insert(0) += 1;

        if let Some(start) = &event.start {
            let month_str = if let Some(ref dt_str) = start.date_time {
                dt_str.get(..7).map(|s| s.to_string())
            } else if let Some(ref d_str) = start.date {
                d_str.get(..7).map(|s| s.to_string())
            } else {
                None
            };
            if let Some(m) = month_str {
                *by_month.entry(m).or_insert(0) += 1;
            }
        }
    }

    println!("\nEvents by type:");
    for (k, v) in &by_type {
        println!("{k}: {v}");
    }

    println!("\nEvents by client:");
    for (k, v) in &by_client {
        println!("{k}: {v}");
    }

    println!("\nEvents by month:");
    for (k, v) in &by_month {
        println!("{k}: {v}");
    }

    Ok(())
}
