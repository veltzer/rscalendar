use anyhow::Result;

use crate::cli::{DeleteArgs, OutputOptions, UpdateArgs, UpsertArgs};
use crate::client::{GoogleCalendarClient, resolve_calendar_id};
use crate::config::Config;
use crate::helpers::{build_event_patch_payload, parse_event_time};
use crate::models::print_event;

pub async fn cmd_event_create(client: &GoogleCalendarClient, args: &UpsertArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    let start = parse_event_time(&args.start, false)?;
    let end = parse_event_time(&args.end, true)?;
    let event = client.create_event(calendar_id, &args.summary, &start, &end, args.description.as_deref(), args.location.as_deref()).await?;
    if !out.quiet { println!("Created event:"); }
    print_event(&event, out.show_builtin, out.json);
    Ok(())
}

pub async fn cmd_event_update(client: &GoogleCalendarClient, args: &UpdateArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    let payload = build_event_patch_payload(
        args.summary.as_deref(),
        args.start.as_deref(),
        args.end.as_deref(),
        args.description.as_deref(),
        args.location.as_deref(),
    )?;
    let event = client.update_event(calendar_id, &args.event_id, &payload).await?;
    if !out.quiet { println!("Updated event:"); }
    print_event(&event, out.show_builtin, out.json);
    Ok(())
}

pub async fn cmd_event_delete(client: &GoogleCalendarClient, args: &DeleteArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    client.delete_event(calendar_id, &args.event_id).await?;
    if !out.quiet { println!("Deleted event '{}'.", args.event_id); }
    Ok(())
}
