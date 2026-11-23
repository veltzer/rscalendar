use std::collections::HashMap;

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::cli::{DeleteArgs, EventEditArgs, OutputOptions, UpdateArgs, UpsertArgs};
use crate::client::{GoogleCalendarClient, resolve_calendar_id};
use crate::config::Config;
use crate::helpers::{build_event_patch_payload, parse_event_time, prompt_select};
use crate::models::print_event;

pub async fn cmd_event_create(
    client: &GoogleCalendarClient,
    args: &UpsertArgs,
    config: &Config,
    out: &OutputOptions,
) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    let start = parse_event_time(&args.start, false)?;
    let end = parse_event_time(&args.end, true)?;
    let event = client
        .create_event(
            calendar_id,
            &args.summary,
            &start,
            &end,
            args.description.as_deref(),
            args.location.as_deref(),
        )
        .await?;
    if !out.quiet {
        println!("Created event:");
    }
    print_event(&event, out.show_builtin, out.json);
    Ok(())
}

pub async fn cmd_event_update(
    client: &GoogleCalendarClient,
    args: &UpdateArgs,
    config: &Config,
    out: &OutputOptions,
) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    let payload = build_event_patch_payload(
        args.summary.as_deref(),
        args.start.as_deref(),
        args.end.as_deref(),
        args.description.as_deref(),
        args.location.as_deref(),
    )?;
    let event = client
        .update_event(calendar_id, &args.event_id, &payload)
        .await?;
    if !out.quiet {
        println!("Updated event:");
    }
    print_event(&event, out.show_builtin, out.json);
    Ok(())
}

pub async fn cmd_event_delete(
    client: &GoogleCalendarClient,
    args: &DeleteArgs,
    config: &Config,
    out: &OutputOptions,
) -> Result<()> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), config)?;
    client.delete_event(calendar_id, &args.event_id).await?;
    if !out.quiet {
        println!("Deleted event '{}'.", args.event_id);
    }
    Ok(())
}

pub async fn cmd_event_edit(
    client: &GoogleCalendarClient,
    args: &EventEditArgs,
    config: &Config,
    out: &OutputOptions,
) -> Result<()> {
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
        let summary = effective_text(&changed_fields, "summary", Some(event.summary_or_default()))
            .unwrap_or_default();
        let start = effective_time(&changed_fields, "start", event.start_str());
        let end = effective_time(&changed_fields, "end", event.end_str());
        let description =
            effective_text(&changed_fields, "description", event.description.as_deref());
        let location = effective_text(&changed_fields, "location", event.location.as_deref());

        print_event_state(
            &summary,
            &start,
            &end,
            location.as_deref(),
            description.as_deref(),
            &current_props,
        );

        // Build menu
        let (menu_items, menu_actions) = build_menu(properties, &current_props);

        let selection = dialoguer::Select::new()
            .with_prompt("Action")
            .items(&menu_items)
            .default(0)
            .interact()?;

        let action = menu_actions[selection];
        let item_text = &menu_items[selection];

        match action {
            "summary" | "start" | "end" | "description" | "location" => {
                let (field, initial) = match action {
                    "summary" => (TextField::Summary, summary.as_str()),
                    "start" => (TextField::Start, start.as_str()),
                    "end" => (TextField::End, end.as_str()),
                    "description" => (TextField::Description, description.as_deref().unwrap_or("")),
                    "location" => (TextField::Location, location.as_deref().unwrap_or("")),
                    other => unreachable!("unexpected text action {other}"),
                };
                edit_text_field(&mut changed_fields, field, initial)?;
            }
            "prop_add" | "prop_change" => {
                // Extract key name from menu text like "add property 'course'" or "change property 'course'"
                let key = extract_property_key(item_text);
                if let Some(props) = properties
                    && let Some(values) = props.get(&key)
                {
                    let prompt = format!("Select value for '{key}'");
                    if let Some(value) = prompt_select(&prompt, values)? {
                        current_props.insert(key, value);
                        props_changed = true;
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
                let saved = save_changes(
                    client,
                    calendar_id,
                    &args.event_id,
                    &changed_fields,
                    props_changed.then_some((&current_props, deleted_prop_keys.as_slice())),
                )
                .await?;
                if !out.quiet {
                    println!("{}", if saved { "Event saved." } else { "No changes." });
                }
                return Ok(());
            }
            "discard" => {
                if !out.quiet {
                    println!("Discarded.");
                }
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}

fn extract_property_key(menu_text: &str) -> String {
    // Extract key from text like "add property 'course'" or "change property 'type'"
    if let Some(start) = menu_text.find('\'')
        && let Some(end) = menu_text[start + 1..].find('\'')
    {
        return menu_text[start + 1..start + 1 + end].to_string();
    }
    String::new()
}

fn print_event_state(
    summary: &str,
    start: &str,
    end: &str,
    location: Option<&str>,
    description: Option<&str>,
    props: &HashMap<String, String>,
) {
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
    if !props.is_empty() {
        eprintln!("---");
        let mut keys: Vec<_> = props.keys().collect();
        keys.sort();
        for key in &keys {
            eprintln!("{key}: {}", props[*key]);
        }
    }
}

/// Build the interactive menu: display texts and the matching action names.
fn build_menu(
    properties: Option<&HashMap<String, Vec<String>>>,
    current_props: &HashMap<String, String>,
) -> (Vec<String>, Vec<&'static str>) {
    let mut menu_items: Vec<String> = vec![
        "edit summary".to_string(),
        "edit start".to_string(),
        "edit end".to_string(),
        "edit description".to_string(),
        "edit location".to_string(),
    ];
    let mut menu_actions: Vec<&'static str> =
        vec!["summary", "start", "end", "description", "location"];

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
    (menu_items, menu_actions)
}

/// An editable text field of an event.
#[derive(Clone, Copy)]
enum TextField {
    Summary,
    Start,
    End,
    Description,
    Location,
}

impl TextField {
    /// The Google Calendar JSON key of the field.
    const fn key(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Start => "start",
            Self::End => "end",
            Self::Description => "description",
            Self::Location => "location",
        }
    }

    const fn prompt(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Start => "Start (RFC3339 or YYYY-MM-DD)",
            Self::End => "End (RFC3339 or YYYY-MM-DD)",
            Self::Description => "Description",
            Self::Location => "Location",
        }
    }
}

/// Prompt for a new value of one text field and record it in `changed_fields`.
fn edit_text_field(
    changed_fields: &mut Map<String, Value>,
    field: TextField,
    initial: &str,
) -> Result<()> {
    let input = dialoguer::Input::<String>::new()
        .with_prompt(field.prompt())
        .with_initial_text(initial);
    let value = match field {
        TextField::Summary => json!(input.interact_text()?),
        TextField::Description | TextField::Location => {
            json!(input.allow_empty(true).interact_text()?)
        }
        TextField::Start | TextField::End => {
            let new_val = input.interact_text()?;
            serde_json::to_value(parse_event_time(&new_val, matches!(field, TextField::End))?)?
        }
    };
    changed_fields.insert(field.key().to_string(), value);
    Ok(())
}

/// Send field and property changes to the server.
/// Returns whether anything was changed.
async fn save_changes(
    client: &GoogleCalendarClient,
    calendar_id: &str,
    event_id: &str,
    changed_fields: &Map<String, Value>,
    props: Option<(&HashMap<String, String>, &[String])>,
) -> Result<bool> {
    let has_field_changes = !changed_fields.is_empty();
    if has_field_changes {
        client
            .update_event(calendar_id, event_id, changed_fields)
            .await?;
    }
    if let Some((current_props, deleted_prop_keys)) = props {
        client
            .patch_event_properties_with_deletes(
                calendar_id,
                event_id,
                current_props,
                deleted_prop_keys,
            )
            .await?;
    }
    Ok(has_field_changes || props.is_some())
}

/// The pending text of a field: the edited value if any, else the original.
fn effective_text(
    changed_fields: &Map<String, Value>,
    key: &str,
    original: Option<&str>,
) -> Option<String> {
    changed_fields
        .get(key)
        .and_then(Value::as_str)
        .or(original)
        .map(str::to_string)
}

/// The pending time of a field: the edited value if any, else the original.
fn effective_time(changed_fields: &Map<String, Value>, key: &str, original: String) -> String {
    changed_fields
        .get(key)
        .and_then(|v| v.get("dateTime").or(v.get("date")))
        .and_then(Value::as_str)
        .map_or(original, str::to_string)
}
