use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Debug, Deserialize)]
pub struct CalendarListResponse {
    #[serde(default)]
    pub items: Vec<CalendarListEntry>,
}

#[derive(Debug, Deserialize)]
pub struct CalendarListEntry {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub primary: Option<bool>,
    #[serde(rename = "accessRole")]
    pub access_role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CalendarEvent {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub status: Option<String>,
    pub html_link: Option<String>,
    pub start: Option<EventDateTime>,
    pub end: Option<EventDateTime>,
    #[serde(rename = "extendedProperties")]
    pub extended_properties: Option<ExtendedProperties>,
}

impl CalendarEvent {
    pub fn summary_or_default(&self) -> &str {
        self.summary.as_deref().unwrap_or("<untitled>")
    }

    pub fn start_str(&self) -> String {
        self.start
            .as_ref()
            .map(EventDateTime::describe)
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn end_str(&self) -> String {
        self.end
            .as_ref()
            .map(EventDateTime::describe)
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn shared_properties(&self) -> HashMap<String, String> {
        self.extended_properties
            .as_ref()
            .and_then(|p| p.shared.clone())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> Value {
        let mut obj = Map::new();
        if let Some(id) = &self.id {
            obj.insert("id".to_string(), json!(id));
        }
        if let Some(summary) = &self.summary {
            obj.insert("summary".to_string(), json!(summary));
        }
        if let Some(start) = &self.start {
            obj.insert("start".to_string(), json!(start.describe()));
        }
        if let Some(end) = &self.end {
            obj.insert("end".to_string(), json!(end.describe()));
        }
        if let Some(status) = &self.status {
            obj.insert("status".to_string(), json!(status));
        }
        if let Some(location) = &self.location {
            obj.insert("location".to_string(), json!(location));
        }
        if let Some(description) = &self.description {
            obj.insert("description".to_string(), json!(description));
        }
        if let Some(html_link) = &self.html_link {
            obj.insert("link".to_string(), json!(html_link));
        }
        if let Some(props) = &self.extended_properties {
            if let Some(shared) = &props.shared {
                obj.insert("properties".to_string(), json!(shared));
            }
        }
        Value::Object(obj)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtendedProperties {
    #[serde(default)]
    pub shared: Option<HashMap<String, String>>,
    #[serde(default)]
    pub private: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventDateTime {
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    pub date_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

impl EventDateTime {
    pub fn describe(&self) -> String {
        match (&self.date_time, &self.date) {
            (Some(date_time), None) => date_time.clone(),
            (None, Some(date)) => date.clone(),
            _ => "unknown".to_string(),
        }
    }
}

pub fn print_event(event: &CalendarEvent, show_builtin: bool, json_output: bool) {
    if json_output {
        println!("{}", serde_json::to_string(&event.to_json()).unwrap());
        return;
    }

    let id = event.id.as_deref().unwrap_or("<missing-id>");
    let summary = event.summary_or_default();
    let start = event.start_str();
    let end = event.end_str();
    let bi = if show_builtin { " (built-in)" } else { "" };

    println!("{summary}");
    println!("  id: {id}{bi}");
    println!("  start: {start}{bi}");
    println!("  end: {end}{bi}");

    if let Some(status) = &event.status {
        println!("  status: {status}{bi}");
    }

    if let Some(location) = &event.location {
        println!("  location: {location}{bi}");
    }

    if let Some(description) = &event.description {
        println!("  description: {description}{bi}");
    }

    if let Some(html_link) = &event.html_link {
        println!("  link: {html_link}{bi}");
    }

    if let Some(props) = &event.extended_properties {
        if let Some(shared) = &props.shared {
            if !shared.is_empty() {
                println!("  ---");
                for (key, value) in shared {
                    println!("  {key}: {value}");
                }
            }
        }
    }

    println!();
}

pub fn print_calendar(cal: &CalendarListEntry, json_output: bool) {
    if json_output {
        let mut obj = Map::new();
        if let Some(id) = &cal.id {
            obj.insert("id".to_string(), json!(id));
        }
        if let Some(summary) = &cal.summary {
            obj.insert("summary".to_string(), json!(summary));
        }
        if let Some(primary) = cal.primary {
            obj.insert("primary".to_string(), json!(primary));
        }
        if let Some(role) = &cal.access_role {
            obj.insert("accessRole".to_string(), json!(role));
        }
        if let Some(desc) = &cal.description {
            obj.insert("description".to_string(), json!(desc));
        }
        println!("{}", serde_json::to_string(&Value::Object(obj)).unwrap());
        return;
    }

    let id = cal.id.as_deref().unwrap_or("<missing-id>");
    let summary = cal.summary.as_deref().unwrap_or("<untitled>");
    let primary = if cal.primary.unwrap_or(false) { " (primary)" } else { "" };
    println!("{summary}{primary}");
    println!("  id: {id}");
    if let Some(role) = &cal.access_role {
        println!("  role: {role}");
    }
    if let Some(desc) = &cal.description {
        println!("  description: {desc}");
    }
    println!();
}
