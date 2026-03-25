use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};

use crate::cli::{DeleteArgs, EventEditArgs, OutputOptions, UpdateArgs, UpsertArgs};
use crate::client::{GoogleCalendarClient, resolve_calendar_id};
use crate::config::Config;
use crate::helpers::{build_event_patch_payload, parse_event_time, prompt_select};
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

pub async fn cmd_event_edit(client: &GoogleCalendarClient, args: &EventEditArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    let event = client.get_event(calendar_id, &args.event_id).await?;

    let properties = config.properties.as_ref();

    let mut changed_fields: Map<String, Value> = Map::new();
    let mut current_props = event.shared_properties();
    let mut deleted_prop_keys: Vec<String> = Vec::new();
    let mut props_changed = false;

    loop {
        // Show current state
        let summary = changed_fields.get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or(event.summary_or_default());
        let start = changed_fields.get("start")
            .and_then(|v| v.get("dateTime").or(v.get("date")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| event.start_str());
        let end = changed_fields.get("end")
            .and_then(|v| v.get("dateTime").or(v.get("date")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| event.end_str());
        let description = changed_fields.get("description")
            .and_then(|v| v.as_str())
            .or(event.description.as_deref());
        let location = changed_fields.get("location")
            .and_then(|v| v.as_str())
            .or(event.location.as_deref());

        eprintln!();
        eprintln!("{summary}");
        eprintln!("start: {start}");
        eprintln!("end: {end}");
        if let Some(loc) = location {
            eprintln!("location: {loc}");
        }
        if let Some(desc) = description {
            eprintln!("description: {desc}");
        }
        if !current_props.is_empty() {
            eprintln!("---");
            let mut keys: Vec<_> = current_props.keys().collect();
            keys.sort();
            for key in &keys {
                eprintln!("{key}: {}", current_props[*key]);
            }
        }

        // Build menu
        let mut menu_items: Vec<String> = vec![
            "edit summary".to_string(),
            "edit start".to_string(),
            "edit end".to_string(),
            "edit description".to_string(),
            "edit location".to_string(),
        ];
        let mut menu_actions: Vec<&str> = vec![
            "summary", "start", "end", "description", "location",
        ];

        // Property actions
        if let Some(props) = properties {
            let mut sorted_keys: Vec<_> = props.keys().collect();
            sorted_keys.sort();
            for key in &sorted_keys {
                if current_props.contains_key(*key) {
                    menu_items.push(format!("change property '{key}'"));
                    menu_actions.push("prop_change");
                    menu_items.push(format!("delete property '{key}'"));
                    menu_actions.push("prop_delete");
                } else {
                    menu_items.push(format!("add property '{key}'"));
                    menu_actions.push("prop_add");
                }
            }
        }

        menu_items.push("save and quit".to_string());
        menu_actions.push("save");
        menu_items.push("quit without saving".to_string());
        menu_actions.push("discard");

        let selection = dialoguer::Select::new()
            .with_prompt("Action")
            .items(&menu_items)
            .default(0)
            .interact()?;

        let action = menu_actions[selection];
        let item_text = &menu_items[selection];

        match action {
            "summary" => {
                let current = changed_fields.get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or(event.summary_or_default());
                let new_val: String = dialoguer::Input::new()
                    .with_prompt("Summary")
                    .with_initial_text(current)
                    .interact_text()?;
                changed_fields.insert("summary".to_string(), json!(new_val));
            }
            "start" => {
                let new_val: String = dialoguer::Input::new()
                    .with_prompt("Start (RFC3339 or YYYY-MM-DD)")
                    .with_initial_text(&start)
                    .interact_text()?;
                let parsed = parse_event_time(&new_val, false)?;
                changed_fields.insert("start".to_string(), serde_json::to_value(&parsed)?);
            }
            "end" => {
                let new_val: String = dialoguer::Input::new()
                    .with_prompt("End (RFC3339 or YYYY-MM-DD)")
                    .with_initial_text(&end)
                    .interact_text()?;
                let parsed = parse_event_time(&new_val, true)?;
                changed_fields.insert("end".to_string(), serde_json::to_value(&parsed)?);
            }
            "description" => {
                let current = description.unwrap_or("");
                let new_val: String = dialoguer::Input::new()
                    .with_prompt("Description")
                    .with_initial_text(current)
                    .allow_empty(true)
                    .interact_text()?;
                changed_fields.insert("description".to_string(), json!(new_val));
            }
            "location" => {
                let current = location.unwrap_or("");
                let new_val: String = dialoguer::Input::new()
                    .with_prompt("Location")
                    .with_initial_text(current)
                    .allow_empty(true)
                    .interact_text()?;
                changed_fields.insert("location".to_string(), json!(new_val));
            }
            "prop_add" | "prop_change" => {
                // Extract key name from menu text like "add property 'course'" or "change property 'course'"
                let key = extract_property_key(item_text);
                if let Some(props) = properties {
                    if let Some(values) = props.get(&key) {
                        let prompt = format!("Select value for '{key}'");
                        if let Some(value) = prompt_select(&prompt, values)? {
                            current_props.insert(key, value);
                            props_changed = true;
                        }
                    }
                }
            }
            "prop_delete" => {
                let key = extract_property_key(item_text);
                current_props.remove(&key);
                deleted_prop_keys.push(key);
                props_changed = true;
            }
            "save" => {
                let has_field_changes = !changed_fields.is_empty();
                if has_field_changes {
                    client.update_event(calendar_id, &args.event_id, &changed_fields).await?;
                }
                if props_changed {
                    client.patch_event_properties_with_deletes(calendar_id, &args.event_id, &current_props, &deleted_prop_keys).await?;
                }
                if has_field_changes || props_changed {
                    if !out.quiet { println!("Event saved."); }
                } else {
                    if !out.quiet { println!("No changes."); }
                }
                return Ok(());
            }
            "discard" => {
                if !out.quiet { println!("Discarded."); }
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}

fn extract_property_key(menu_text: &str) -> String {
    // Extract key from text like "add property 'course'" or "change property 'type'"
    if let Some(start) = menu_text.find('\'') {
        if let Some(end) = menu_text[start+1..].find('\'') {
            return menu_text[start+1..start+1+end].to_string();
        }
    }
    String::new()
}
