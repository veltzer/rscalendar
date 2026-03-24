# calendar create

Create a new public Google Calendar.

## Usage

```bash
rscalendar calendar create <NAME>
```

## Details

Creates a new calendar with the given name and sets it to public (readable by anyone). The calendar ID is printed on success — use it with `--calendar-id` or in `config.toml`.

## Examples

```bash
$ rscalendar calendar create Teaching
Created public calendar 'Teaching'
  id: abc123@group.calendar.google.com
```
