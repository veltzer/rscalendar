use anyhow::{Context, Result};
use serde_json::json;

use crate::cli::{CalendarClearArgs, CalendarCopyArgs, OutputOptions};
use crate::client::{GoogleCalendarClient, resolve_calendar_id};
use crate::config::Config;
use crate::fetch_events;
use crate::helpers::prompt_yes_no_quit;

pub async fn cmd_calendar_create(client: &GoogleCalendarClient, name: &str, out: &OutputOptions) -> Result<()> {
    let calendar = client.create_calendar(name).await?;
    let id = calendar["id"].as_str().unwrap_or("<unknown>");
    if !out.quiet {
        println!("Created public calendar '{name}'");
        println!("  id: {id}");
    }
    Ok(())
}

pub async fn cmd_calendar_delete(client: &GoogleCalendarClient, name: &str, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, Some(name), config)?;
    use std::io::{BufRead, Write};
    eprint!("Delete calendar '{name}'? This cannot be undone. [y/n]: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    match line.trim().to_lowercase().as_str() {
        "y" | "yes" => {
            client.delete_calendar(calendar_id).await?;
            if !out.quiet { println!("Deleted calendar '{name}'."); }
        }
        _ => {
            if !out.quiet { println!("Aborted."); }
        }
    }
    Ok(())
}

pub async fn cmd_calendar_rename(client: &GoogleCalendarClient, name: &str, new_name: &str, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, Some(name), config)?;
    client.rename_calendar(calendar_id, new_name).await?;
    if !out.quiet { println!("Renamed calendar '{name}' to '{new_name}'."); }
    Ok(())
}

pub async fn cmd_calendar_clear(client: &GoogleCalendarClient, args: &CalendarClearArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    if !out.quiet {
        println!("Found {} event(s)", events.len());
        if !args.all {
            println!("  interactive mode: y=delete, n=skip, q=quit");
        }
        println!();
    }

    let mut deleted = 0u32;
    let mut skipped = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => {
                eprintln!("  skipping event with no id: {summary}");
                skipped += 1;
                continue;
            }
        };

        if !args.all {
            let prompt = format!("  Delete '{summary}' ({start})?");
            match prompt_yes_no_quit(&prompt)? {
                Some(true) => {}
                Some(false) => { skipped += 1; continue; }
                None => {
                    if !out.quiet { println!("\nQuit. {deleted} event(s) deleted, {skipped} skipped."); }
                    return Ok(());
                }
            }
        }

        client.delete_event(&calendar_id, event_id).await?;
        if !out.quiet { println!("  deleted: {summary} ({start})"); }
        deleted += 1;
    }

    if !out.quiet {
        println!("\nDone. {deleted} event(s) deleted, {skipped} skipped.");
    }

    Ok(())
}

pub async fn cmd_calendar_copy(client: &GoogleCalendarClient, args: &CalendarCopyArgs, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;

    let source_cal = calendars.iter()
        .find(|c| c.summary.as_deref() == Some(&args.source))
        .context(format!("no calendar named '{}' found", args.source))?;
    let source_id = source_cal.id.as_deref().context("source calendar has no id")?.to_string();

    let target_cal = calendars.iter()
        .find(|c| c.summary.as_deref() == Some(&args.target))
        .context(format!("no calendar named '{}' found", args.target))?;
    let target_id = target_cal.id.as_deref().context("target calendar has no id")?.to_string();

    if !out.quiet { println!("Copying events from '{}' to '{}'\n", args.source, args.target); }

    let events = client.list_all_events(&source_id).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found in '{}'.", args.source); }
        return Ok(());
    }

    if !out.quiet { println!("Found {} event(s)\n", events.len()); }

    let mut copied = 0u32;
    let mut skipped = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        if event.id.is_none() {
            eprintln!("  skipping event with no id: {summary}");
            skipped += 1;
            continue;
        }

        if args.dry_run {
            if !out.quiet { println!("  [dry-run] would copy: {summary} ({start})"); }
            copied += 1;
        } else {
            let mut payload = json!({ "summary": summary });
            if let Some(start) = &event.start {
                payload["start"] = serde_json::to_value(start)?;
            }
            if let Some(end) = &event.end {
                payload["end"] = serde_json::to_value(end)?;
            }
            if let Some(desc) = &event.description {
                payload["description"] = json!(desc);
            }
            if let Some(loc) = &event.location {
                payload["location"] = json!(loc);
            }
            if let Some(props) = &event.extended_properties {
                payload["extendedProperties"] = serde_json::to_value(props)?;
            }

            client.insert_event_raw(&target_id, &payload).await?;
            if !out.quiet { println!("  copied: {summary} ({start})"); }
            copied += 1;
        }
    }

    if !out.quiet {
        if args.dry_run {
            println!("\nDry run complete. {copied} event(s) would be copied, {skipped} skipped.");
        } else {
            println!("\nDone. {copied} event(s) copied to '{}', {skipped} skipped.", args.target);
        }
    }

    Ok(())
}
