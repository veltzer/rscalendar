use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::cli::{MoveEventsArgs, OutputOptions};
use crate::client::GoogleCalendarClient;
use crate::helpers::prompt_yes_no_quit;
use crate::models::{CalendarEvent, CalendarListEntry};

pub async fn cmd_move_events(
    client: &GoogleCalendarClient,
    args: &MoveEventsArgs,
    out: &OutputOptions,
) -> Result<()> {
    let calendars = client.list_calendars().await?;

    let source_id = find_calendar_id(&calendars, &args.source, "source")?;
    let target_id = find_calendar_id(&calendars, &args.target, "target")?;

    if !out.quiet {
        println!("Moving events from '{}' to '{}'", args.source, args.target);
    }
    if !out.quiet {
        if !args.all {
            println!("interactive mode: y=move, n=skip, q=quit");
        }
        println!();
    }

    let events = client.list_all_events(&source_id).await?;
    if events.is_empty() {
        if !out.quiet {
            println!("No events found in '{}'.", args.source);
        }
        return Ok(());
    }

    if !out.quiet {
        println!("Found {} event(s)\n", events.len());
    }

    let mut moved = 0u32;
    let mut skipped = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let Some(event_id) = &event.id else {
            eprintln!("skipping event with no id: {summary}");
            skipped += 1;
            continue;
        };

        if !args.all {
            let prompt = format!("Move '{summary}' ({start})?");
            match prompt_yes_no_quit(&prompt)? {
                Some(true) => {}
                Some(false) => {
                    skipped += 1;
                    continue;
                }
                None => {
                    if !out.quiet {
                        println!("\nQuit. {moved} event(s) moved, {skipped} skipped.");
                    }
                    return Ok(());
                }
            }
        }

        if args.dry_run {
            if !out.quiet {
                println!("[dry-run] would move: {summary} ({start})");
            }
        } else {
            let payload = move_payload(event, summary)?;

            client.insert_event_raw(&target_id, &payload).await?;
            client.delete_event(&source_id, event_id).await?;
            if !out.quiet {
                println!("moved: {summary} ({start})");
            }
        }
        moved += 1;
    }

    if !out.quiet {
        if args.dry_run {
            println!("\nDry run complete. {moved} event(s) would be moved, {skipped} skipped.");
        } else {
            println!(
                "\nDone. {moved} event(s) moved to '{}', {skipped} skipped.",
                args.target
            );
        }
    }

    Ok(())
}

fn find_calendar_id(calendars: &[CalendarListEntry], name: &str, role: &str) -> Result<String> {
    let cal = calendars
        .iter()
        .find(|c| c.summary.as_deref() == Some(name))
        .context(format!("no calendar named '{name}' found"))?;
    Ok(cal
        .id
        .as_deref()
        .context(format!("{role} calendar has no id"))?
        .to_string())
}

fn move_payload(event: &CalendarEvent, summary: &str) -> Result<Value> {
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
    Ok(payload)
}
