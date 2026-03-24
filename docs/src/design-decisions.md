# Design Decisions

## Event Tags via Shared Extended Properties

Google Calendar API offers `extendedProperties` on events for storing custom key-value metadata. There are two scopes:

- **`private`** — scoped to the OAuth client ID that created them. Only the same client ID can read them back.
- **`shared`** — visible to anyone with access to the event (all attendees, all apps, including public calendar readers via the API).

### Why `shared`?

We chose `shared` extended properties for the following reasons:

1. **Data durability** — `private` properties are tied to the OAuth client ID in `credentials.json`. If you ever recreate your OAuth credentials in Google Cloud Console (new project, deleted and re-created client ID, etc.), all `private` extended properties become permanently inaccessible. The data isn't deleted, but there is no way to recover it with a different client ID. With `shared`, the data belongs to the event itself and survives credential changes.

2. **Acceptable visibility tradeoff** — `shared` properties are visible via the API to anyone who can read the event. On public calendars, this means anyone querying the API could see the tags. However, extended properties are **not** displayed in the Google Calendar web or mobile UI — they are only accessible programmatically. For a personal CLI tool, this is an acceptable tradeoff.

3. **Interoperability** — `shared` properties can be read and written by any tool or script with access to the calendar, not just rscalendar. This makes it possible to build other tools that interact with the same tags.

### Implementation Plan

Tags will be stored as a comma-separated string under a single key in `shared` extended properties:

```json
{
  "extendedProperties": {
    "shared": {
      "tags": "work,urgent,followup"
    }
  }
}
```

The Google Calendar API supports filtering events by extended properties using the `sharedExtendedProperty` query parameter, enabling efficient tag-based queries without client-side filtering.
