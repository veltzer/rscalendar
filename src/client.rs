use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde_json::{Map, Value, json};
use urlencoding::encode;

use crate::auth::NoInteractionDelegate;
use crate::config::{Config, credentials_path, token_cache_path};
use crate::models::{CalendarEvent, CalendarListEntry, CalendarListResponse, EventDateTime};

pub const API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

#[derive(Clone)]
pub struct GoogleCalendarClient {
    pub http: Client,
    pub access_token: String,
}

impl GoogleCalendarClient {
    pub async fn from_cache() -> Result<Self> {
        let cache_path = token_cache_path()?;
        if !cache_path.exists() {
            eprintln!("Error: not authenticated. Run 'rscalendar auth' first.");
            std::process::exit(1);
        }

        let secret = yup_oauth2::read_application_secret(credentials_path()?)
            .await
            .context("failed to read credentials.json")?;

        let auth = yup_oauth2::InstalledFlowAuthenticator::builder(
            secret,
            yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(cache_path)
        .flow_delegate(Box::new(NoInteractionDelegate))
        .build()
        .await
        .context("failed to build authenticator")?;

        let token = auth
            .token(&[CALENDAR_SCOPE])
            .await
            .context("failed to obtain access token; try running 'rscalendar auth' again")?;

        let access_token = token.token().context("token has no access_token field")?.to_string();

        Ok(Self {
            http: Client::new(),
            access_token,
        })
    }

    pub async fn create_calendar(&self, summary: &str) -> Result<Value> {
        let url = format!("{API_BASE}/calendars");
        let response = self
            .authorized(self.http.post(&url))
            .json(&json!({ "summary": summary }))
            .send()
            .await
            .context("failed to create calendar")?;
        let response = response.error_for_status().map_err(api_error)?;
        let calendar: Value = response.json().await.context("failed to decode created calendar")?;

        let cal_id = calendar["id"].as_str().context("created calendar has no id")?;
        let acl_url = format!("{API_BASE}/calendars/{}/acl", encode(cal_id));
        self.authorized(self.http.post(&acl_url))
            .json(&json!({
                "role": "reader",
                "scope": { "type": "default" }
            }))
            .send()
            .await
            .context("failed to set calendar ACL")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(calendar)
    }

    pub async fn list_all_events(&self, calendar_id: &str) -> Result<Vec<CalendarEvent>> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let url = format!("{API_BASE}/calendars/{}/events", encode(calendar_id));

            let mut request = self
                .authorized(self.http.get(url))
                .query(&[
                    ("maxResults", "2500"),
                    ("singleEvents", "true"),
                    ("orderBy", "startTime"),
                ]);

            if let Some(token) = &page_token {
                request = request.query(&[("pageToken", token.as_str())]);
            }

            let response = request
                .send()
                .await
                .context("failed to call Google Calendar list events API")?;

            let response = response.error_for_status().map_err(api_error)?;
            let body: Value = response
                .json()
                .await
                .context("failed to decode event list response")?;

            if let Some(items) = body["items"].as_array() {
                let events: Vec<CalendarEvent> = serde_json::from_value(json!(items))
                    .context("failed to deserialize events")?;
                all_events.extend(events);
            }

            match body["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }

        Ok(all_events)
    }

    pub async fn insert_event_raw(&self, calendar_id: &str, payload: &Value) -> Result<CalendarEvent> {
        let url = format!("{API_BASE}/calendars/{}/events", encode(calendar_id));

        let response = self
            .authorized(self.http.post(url))
            .json(payload)
            .send()
            .await
            .context("failed to insert event")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode inserted event")
    }

    pub async fn list_calendars(&self) -> Result<Vec<CalendarListEntry>> {
        let url = format!("{API_BASE}/users/me/calendarList");

        let response = self
            .authorized(self.http.get(url))
            .send()
            .await
            .context("failed to call Google Calendar list calendars API")?;

        let response = response.error_for_status().map_err(api_error)?;
        let body: CalendarListResponse = response
            .json()
            .await
            .context("failed to decode calendar list response")?;
        Ok(body.items)
    }

    pub async fn create_event(&self, calendar_id: &str, summary: &str, start: &EventDateTime, end: &EventDateTime, description: Option<&str>, location: Option<&str>) -> Result<CalendarEvent> {
        let mut payload = Map::new();
        payload.insert("summary".to_string(), json!(summary));
        payload.insert("start".to_string(), serde_json::to_value(start)?);
        payload.insert("end".to_string(), serde_json::to_value(end)?);
        if let Some(d) = description {
            payload.insert("description".to_string(), json!(d));
        }
        if let Some(l) = location {
            payload.insert("location".to_string(), json!(l));
        }

        let url = format!("{API_BASE}/calendars/{}/events", encode(calendar_id));
        let response = self
            .authorized(self.http.post(url))
            .json(&Value::Object(payload))
            .send()
            .await
            .context("failed to call Google Calendar create event API")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode created event response")
    }

    pub async fn update_event(&self, calendar_id: &str, event_id: &str, payload: &Map<String, Value>) -> Result<CalendarEvent> {
        if payload.is_empty() {
            bail!("no fields were provided to update");
        }

        let url = format!("{API_BASE}/calendars/{}/events/{}", encode(calendar_id), encode(event_id));
        let response = self
            .authorized(self.http.patch(url))
            .json(payload)
            .send()
            .await
            .context("failed to call Google Calendar update event API")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode updated event response")
    }

    pub async fn patch_event_properties(
        &self,
        calendar_id: &str,
        event_id: &str,
        shared: &HashMap<String, String>,
    ) -> Result<CalendarEvent> {
        let url = format!("{API_BASE}/calendars/{}/events/{}", encode(calendar_id), encode(event_id));
        let payload = json!({ "extendedProperties": { "shared": shared } });

        let response = self
            .authorized(self.http.patch(url))
            .json(&payload)
            .send()
            .await
            .context("failed to patch event properties")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode patched event")
    }

    pub async fn delete_property(&self, calendar_id: &str, event_id: &str, key: &str) -> Result<()> {
        let url = format!("{API_BASE}/calendars/{}/events/{}", encode(calendar_id), encode(event_id));
        let mut shared = Map::new();
        shared.insert(key.to_string(), Value::Null);
        let payload = json!({ "extendedProperties": { "shared": shared } });

        self.authorized(self.http.patch(url))
            .json(&payload)
            .send()
            .await
            .context("failed to delete property")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(())
    }

    pub async fn patch_event_properties_with_deletes(
        &self,
        calendar_id: &str,
        event_id: &str,
        current: &HashMap<String, String>,
        deleted_keys: &[String],
    ) -> Result<()> {
        let mut patch_shared = Map::new();
        for (k, v) in current {
            patch_shared.insert(k.clone(), json!(v));
        }
        for k in deleted_keys {
            patch_shared.insert(k.clone(), Value::Null);
        }
        let payload = json!({ "extendedProperties": { "shared": patch_shared } });
        let url = format!("{API_BASE}/calendars/{}/events/{}", encode(calendar_id), encode(event_id));

        self.authorized(self.http.patch(url))
            .json(&payload)
            .send()
            .await
            .context("failed to update properties")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(())
    }

    pub async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()> {
        let url = format!("{API_BASE}/calendars/{}/events/{}", encode(calendar_id), encode(event_id));

        self.authorized(self.http.delete(url))
            .send()
            .await
            .context("failed to call Google Calendar delete event API")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(())
    }

    pub fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.bearer_auth(&self.access_token)
    }
}

pub fn resolve_calendar_id<'a>(
    calendars: &'a [CalendarListEntry],
    name: Option<&str>,
    config: &Config,
) -> Result<&'a str> {
    let name = name
        .or(config.calendar_name.as_deref())
        .context("no calendar name specified; use --calendar-name or set calendar_name in config.toml")?;
    let matches: Vec<&CalendarListEntry> = calendars
        .iter()
        .filter(|c| c.summary.as_deref() == Some(name))
        .collect();
    if matches.is_empty() {
        bail!("no calendar named '{name}' found");
    }
    if matches.len() > 1 {
        let ids: Vec<String> = matches
            .iter()
            .map(|c| c.id.as_deref().unwrap_or("<no id>").to_string())
            .collect();
        bail!(
            "multiple calendars named '{name}' found (IDs: {}); use a unique calendar name",
            ids.join(", ")
        );
    }
    matches[0]
        .id
        .as_deref()
        .context("calendar has no id")
}

fn api_error(error: reqwest::Error) -> anyhow::Error {
    if let Some(status) = error.status() {
        anyhow::anyhow!("Google Calendar API request failed with status {status}")
    } else {
        anyhow::anyhow!(error)
    }
}
