# TODO

## High Value

- `list` should accept multiple `--calendar-name` flags and merge results
- `event show --event-id <ID>` — show a single event's full details
- `event search` — search across all calendars by summary/description
- `export --format csv` — export events to CSV for spreadsheets/invoicing
- `report` — generate a report: hours per client per month, with course breakdown
- Recurring event support — create/edit recurring events (currently flattened with singleEvents=true)

## Medium Value

- `event duplicate --event-id <ID> --start <NEW_START>` — copy an event to a new time
- `calendar export --format ics` — backup a calendar to ICS file
- `calendar import --format ics` — restore from ICS
- `config check` — validate the config file
- `properties list` — show all defined property keys and their allowed values from config
- Colored output using `console` crate (already a transitive dependency from dialoguer)

## Teaching Business Workflows

- `invoice --client AgileSparks --month 2026-03` — summarize teaching hours for a client in a month
- `schedule --client AgileSparks --course "Linux Fundamentals" --start 2026-04-01 --sessions 5 --interval weekly` — create a series of teaching events
- `availability --month 2026-04` — show free days/slots based on existing events

## Infrastructure

- Integration tests with a mock Google Calendar server (using wiremock or mockito)
