use anyhow::{Context, Result, bail};

use crate::cli::{CalendarNameArgs, OutputOptions, PropertiesAddArgs, PropertiesDeleteArgs, PropertiesRenameArgs, PropertiesSetValueArgs};
use crate::client::GoogleCalendarClient;
use crate::config::Config;
use crate::fetch_events;
use crate::helpers::{prompt_select, prompt_yes_no_quit};

pub async fn cmd_properties_add(client: &GoogleCalendarClient, args: &PropertiesAddArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    if args.key.is_some() != args.value.is_some() {
        bail!("--key and --value must be used together");
    }

    let properties = config.properties.as_ref()
        .context("no [properties] section in config.toml")?;
    if properties.is_empty() {
        bail!("no properties defined in [properties] section of config.toml");
    }

    if let (Some(key), Some(value)) = (&args.key, &args.value) {
        let allowed = properties.get(key)
            .with_context(|| format!("key '{key}' is not defined in [properties] in config.toml"))?;
        if !allowed.contains(value) {
            bail!("value '{value}' is not allowed for key '{key}'. Allowed: {}", allowed.join(", "));
        }
    }

    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let sorted_keys: Vec<&String> = {
        let mut keys: Vec<_> = properties.keys().collect();
        keys.sort();
        keys
    };

    if !out.quiet { println!("Adding properties to {} event(s)\n", events.len()); }

    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut skipped_no_id = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => { skipped_no_id += 1; continue; }
        };

        let existing = event.shared_properties();

        if let (Some(key), Some(value)) = (&args.key, &args.value) {
            if existing.get(key).is_some_and(|v| v == value) {
                skipped += 1;
                continue;
            }

            if !args.all {
                let prompt = format!("Set {key}={value} on '{summary}' ({start})?");
                match prompt_yes_no_quit(&prompt)? {
                    Some(true) => {}
                    Some(false) => { skipped += 1; continue; }
                    None => {
                        if !out.quiet { println!("\nQuit. {updated} updated, {skipped} skipped."); }
                        return Ok(());
                    }
                }
            }

            let mut new_props = existing;
            new_props.insert(key.clone(), value.clone());
            client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
            if !out.quiet { println!("set on: {summary} ({start})"); }
            updated += 1;
        } else {
            eprintln!("Event: {summary} ({start})");
            if !existing.is_empty() {
                for (k, v) in &existing {
                    eprintln!("existing {k}: {v}");
                }
            }

            let mut new_props = existing.clone();
            let mut changed = false;

            for key in &sorted_keys {
                let values = &properties[*key];
                if existing.contains_key(*key) {
                    continue;
                }
                let prompt = format!("Select {key}:");
                if let Some(value) = prompt_select(&prompt, values)? {
                    new_props.insert((*key).clone(), value);
                    changed = true;
                }
            }

            if changed {
                client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
                eprintln!("updated\n");
                updated += 1;
            } else {
                eprintln!("no changes\n");
                skipped += 1;
            }
        }
    }

    if skipped_no_id > 0 {
        eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
    }
    if !out.quiet { println!("Done. {updated} updated, {skipped} skipped."); }

    Ok(())
}

pub async fn cmd_properties_check(client: &GoogleCalendarClient, args: &CalendarNameArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let properties = config.properties.as_ref()
        .context("no [properties] section in config.toml")?;
    if properties.is_empty() {
        bail!("no properties defined in [properties] section of config.toml");
    }

    let (_, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let mut issues = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let shared = event.extended_properties.as_ref().and_then(|p| p.shared.as_ref());

        let mut event_issues: Vec<String> = Vec::new();

        for key in properties.keys() {
            match shared.and_then(|s| s.get(key)) {
                None => event_issues.push(format!("missing property '{key}'")),
                Some(value) => {
                    let allowed = &properties[key];
                    if !allowed.contains(value) {
                        event_issues.push(format!(
                            "property '{key}' has value '{value}' which is not in allowed values: {}",
                            allowed.join(", ")
                        ));
                    }
                }
            }
        }

        if let Some(shared) = shared {
            for key in shared.keys() {
                if !properties.contains_key(key) {
                    event_issues.push(format!("unknown property '{key}'"));
                }
            }
        }

        if !event_issues.is_empty() {
            println!("{summary} ({start}):");
            for issue in &event_issues {
                println!("- {issue}");
            }
            issues += event_issues.len() as u32;
        }
    }

    if !out.quiet {
        if issues == 0 {
            println!("All {} event(s) have valid properties.", events.len());
        } else {
            println!("\n{issues} issue(s) found across {} event(s).", events.len());
        }
    }

    Ok(())
}

pub async fn cmd_properties_delete(client: &GoogleCalendarClient, args: &PropertiesDeleteArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut skipped_no_id = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => { skipped_no_id += 1; continue; }
        };

        let existing = event.shared_properties();
        if !existing.contains_key(&args.key) {
            skipped += 1;
            continue;
        }

        if !args.all {
            let current_value = &existing[&args.key];
            let prompt = format!("Delete {}={current_value} from '{summary}' ({start})?", args.key);
            match prompt_yes_no_quit(&prompt)? {
                Some(true) => {}
                Some(false) => { skipped += 1; continue; }
                None => {
                    if !out.quiet { println!("\nQuit. {updated} deleted, {skipped} skipped."); }
                    return Ok(());
                }
            }
        }

        client.delete_property(&calendar_id, event_id, &args.key).await?;
        if !out.quiet { println!("deleted from: {summary} ({start})"); }
        updated += 1;
    }

    if skipped_no_id > 0 {
        eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
    }
    if !out.quiet { println!("\nDone. {updated} deleted, {skipped} skipped."); }

    Ok(())
}

pub async fn cmd_properties_rename(client: &GoogleCalendarClient, args: &PropertiesRenameArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut skipped_no_id = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => { skipped_no_id += 1; continue; }
        };

        let mut existing = event.shared_properties();
        let value = match existing.remove(&args.from) {
            Some(v) => v,
            None => { skipped += 1; continue; }
        };

        if !args.all {
            let prompt = format!(
                "Rename '{}'='{}' to '{}'='{}' on '{summary}' ({start})?",
                args.from, value, args.to, value
            );
            match prompt_yes_no_quit(&prompt)? {
                Some(true) => {}
                Some(false) => { skipped += 1; continue; }
                None => {
                    if !out.quiet { println!("\nQuit. {updated} renamed, {skipped} skipped."); }
                    return Ok(());
                }
            }
        }

        existing.insert(args.to.clone(), value);
        client.patch_event_properties(&calendar_id, event_id, &existing).await?;
        if !out.quiet { println!("renamed on: {summary} ({start})"); }
        updated += 1;
    }

    if skipped_no_id > 0 {
        eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
    }
    if !out.quiet { println!("\nDone. {updated} renamed, {skipped} skipped."); }

    Ok(())
}

pub async fn cmd_properties_edit(client: &GoogleCalendarClient, args: &CalendarNameArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let properties = config.properties.as_ref()
        .context("no [properties] section in config.toml")?;
    if properties.is_empty() {
        bail!("no properties defined in [properties] section of config.toml");
    }

    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let sorted_keys: Vec<&String> = {
        let mut keys: Vec<_> = properties.keys().collect();
        keys.sort();
        keys
    };

    let mut updated = 0u32;
    let mut skipped_no_id = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let end = event.end_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => { skipped_no_id += 1; continue; }
        };

        let mut current = event.shared_properties();
        let mut changed = false;
        let mut deleted_keys: Vec<String> = Vec::new();

        loop {
            eprintln!();
            eprintln!("{summary}");
            eprintln!("start: {start}");
            eprintln!("end:   {end}");
            if let Some(location) = &event.location {
                eprintln!("location: {location}");
            }
            if let Some(description) = &event.description {
                eprintln!("description: {description}");
            }
            if current.is_empty() && deleted_keys.is_empty() {
                eprintln!("(no properties)");
            } else {
                for key in &sorted_keys {
                    if let Some(val) = current.get(*key) {
                        eprintln!("{key}: {val}");
                    }
                }
                for (k, v) in &current {
                    if !properties.contains_key(k) {
                        eprintln!("{k}: {v} (unknown)");
                    }
                }
            }

            let mut menu_items: Vec<String> = Vec::new();
            let mut menu_actions: Vec<(&str, String)> = Vec::new(); // (action, key)

            for key in &sorted_keys {
                if current.contains_key(*key) {
                    menu_items.push(format!("change '{key}'"));
                    menu_actions.push(("change", (*key).clone()));
                    menu_items.push(format!("delete '{key}'"));
                    menu_actions.push(("delete", (*key).clone()));
                } else {
                    menu_items.push(format!("add '{key}'"));
                    menu_actions.push(("add", (*key).clone()));
                }
            }
            menu_items.push("next event".to_string());
            menu_items.push("quit".to_string());

            let selection = dialoguer::Select::new()
                .with_prompt("Action")
                .items(&menu_items)
                .default(0)
                .interact()?;

            if selection == menu_items.len() - 2 {
                // next event
                break;
            }
            if selection == menu_items.len() - 1 {
                // quit
                if changed {
                    client.patch_event_properties_with_deletes(&calendar_id, event_id, &current, &deleted_keys).await?;
                    updated += 1;
                    eprintln!("saved.");
                }
                if !out.quiet { println!("\nQuit. {updated} event(s) updated."); }
                return Ok(());
            }

            let (action, key) = &menu_actions[selection];
            match *action {
                "add" | "change" => {
                    let values = &properties[key];
                    let prompt = format!("Select value for '{key}'");
                    if let Some(value) = prompt_select(&prompt, values)? {
                        current.insert(key.clone(), value);
                        changed = true;
                    }
                }
                "delete" => {
                    current.remove(key);
                    deleted_keys.push(key.clone());
                    changed = true;
                    eprintln!("deleted '{key}'");
                }
                _ => unreachable!(),
            }
        }

        if changed {
            client.patch_event_properties_with_deletes(&calendar_id, event_id, &current, &deleted_keys).await?;
            eprintln!("saved.");
            updated += 1;
        }
    }

    if skipped_no_id > 0 {
        eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
    }
    if !out.quiet { println!("\nDone. {updated} event(s) updated."); }

    Ok(())
}

pub async fn cmd_properties_set_value(client: &GoogleCalendarClient, args: &PropertiesSetValueArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let mut updated = 0u32;
    let mut skipped = 0u32;
    let mut skipped_no_id = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let event_id = match &event.id {
            Some(id) => id,
            None => { skipped_no_id += 1; continue; }
        };

        let existing = event.shared_properties();
        match existing.get(&args.key) {
            Some(v) if v == &args.from => {}
            _ => { skipped += 1; continue; }
        }

        if !args.all {
            let prompt = format!(
                "Change {}='{}' to '{}' on '{summary}' ({start})?",
                args.key, args.from, args.to
            );
            match prompt_yes_no_quit(&prompt)? {
                Some(true) => {}
                Some(false) => { skipped += 1; continue; }
                None => {
                    if !out.quiet { println!("\nQuit. {updated} changed, {skipped} skipped."); }
                    return Ok(());
                }
            }
        }

        let mut new_props = existing;
        new_props.insert(args.key.clone(), args.to.clone());
        client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
        if !out.quiet { println!("changed on: {summary} ({start})"); }
        updated += 1;
    }

    if skipped_no_id > 0 {
        eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
    }
    if !out.quiet { println!("\nDone. {updated} changed, {skipped} skipped."); }

    Ok(())
}
