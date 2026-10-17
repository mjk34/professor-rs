//! Birthday and event reminders: reads/writes `.eventdb` and fires Discord messages.
use crate::data;
use crate::helper::{default_footer, parse_id, write_atomic};
use crate::serenity;
use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, Utc};
use poise::serenity_prelude::{ChannelId, CreateMessage};
use std::env;

/// Path to the flat-file birthday/event database (CSV, one entry per line).
const EVENT_FILE: &str = ".eventdb";
/// Number of days in advance to send a birthday reminder to the mod channel.
const BIRTHDAY_REMINDER_DAYS_AHEAD: i64 = 14;
/// Default timezone offset from UTC when `TZ_OFFSET_HOURS` is unset — EDT (UTC-4).
const DEFAULT_TZ_OFFSET_HOURS: i64 = -4;

/// Local calendar date for a UTC offset in hours (e.g. -4 for EDT).
fn local_today(now_utc: DateTime<Utc>, offset_hours: i64) -> NaiveDate {
    (now_utc + TimeDelta::hours(offset_hours)).date_naive()
}

/// Whether a row's reminded/pinged flags belong to an already-passed birthday and should reset.
/// Only reset once the next occurrence is outside the reminder window — otherwise a birthday in
/// early January (seen from late December) resets and re-sends its reminder every run.
const fn flags_expired(birthday_passed_this_year: bool, days_until_next: i64) -> bool {
    birthday_passed_this_year && days_until_next > BIRTHDAY_REMINDER_DAYS_AHEAD
}

/// One `.eventdb` entry: `MM-DD,name,user_id,reminded,pinged`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EventRow {
    date: String,
    name: String,
    user_id: String,
    reminded: bool,
    pinged: bool,
}

/// A line of `.eventdb`. Lines that don't parse are kept verbatim so a rewrite never drops them.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Event(EventRow),
    Raw(String),
}

fn parse_line(line: &str) -> Line {
    let cells: Vec<&str> = line.split(',').collect();
    let n = cells.len();
    if n < 5 {
        return Line::Raw(line.to_string());
    }
    // The name is the only free-text column, so a name containing commas spans the
    // cells between the date and the last three fields.
    Line::Event(EventRow {
        date: cells[0].to_string(),
        name: cells[1..n - 3].join(","),
        user_id: cells[n - 3].to_string(),
        reminded: cells[n - 2] == "1",
        pinged: cells[n - 1] == "1",
    })
}

/// Parses the file body. The first line is a `-` placeholder header when present.
fn parse_file(contents: &str) -> Vec<Line> {
    let mut lines = contents.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim() == "-") {
        lines.next();
    }
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let parsed = parse_line(l);
            if matches!(parsed, Line::Raw(_)) {
                tracing::warn!(line = %l, "reminder: malformed .eventdb line — kept as-is, not processed");
            }
            parsed
        })
        .collect()
}

fn serialize(lines: &[Line]) -> String {
    let mut contents = String::from("-");
    for line in lines {
        contents.push('\n');
        match line {
            Line::Event(r) => contents += &format!(
                "{},{},{},{},{}",
                r.date, r.name, r.user_id, u8::from(r.reminded), u8::from(r.pinged)
            ),
            Line::Raw(raw) => contents += raw,
        }
    }
    contents
}

/// Date a birthday is observed in `year`; Feb 29 falls back to Feb 28 in non-leap years.
fn observed_birthday(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day)
        .or_else(|| if month == 2 && day == 29 { NaiveDate::from_ymd_opt(year, 2, 28) } else { None })
}

fn days_phrase(days: i64) -> String {
    if days == 1 { "tomorrow".to_string() } else { format!("in {days} days") }
}

async fn send_reminder(http: &serenity::Http, channel: ChannelId, desc: &str) -> Result<(), crate::Error> {
    channel
        .send_message(
            http,
            CreateMessage::default().embed(
                serenity::CreateEmbed::default()
                    .title("Reminder")
                    .description(desc)
                    .color(data::EMBED_CYAN)
                    .footer(default_footer()),
            ),
        )
        .await?;
    Ok(())
}

/// What to send for one birthday row on a given day.
#[derive(Debug, PartialEq, Eq)]
enum Notice {
    /// Advance heads-up to the mod channel.
    Remind { days: i64, date: NaiveDate },
    /// Day-of ping in general chat.
    Ping,
}

/// Decides what (if anything) to send for `row` on `today`, resetting flags left over from a
/// past birthday. The flag for a returned notice is set by the caller only once the send
/// succeeds, so a failed send is retried on the next run.
fn plan_row(row: &mut EventRow, today: NaiveDate) -> Option<Notice> {
    let mut date_parts = row.date.split('-');
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let date_this_year = observed_birthday(today.year(), month, day)?;
    let date_next_year = observed_birthday(today.year() + 1, month, day)?;

    let next_occurrence = if today <= date_this_year { date_this_year } else { date_next_year };
    let days_until = (next_occurrence - today).num_days();

    if flags_expired(today > date_this_year, days_until) {
        row.reminded = false;
        row.pinged = false;
    }

    if days_until == 0 {
        // The day-of ping replaces the advance reminder.
        row.reminded = true;
        return (!row.pinged).then_some(Notice::Ping);
    }
    (days_until <= BIRTHDAY_REMINDER_DAYS_AHEAD && !row.reminded)
        .then_some(Notice::Remind { days: days_until, date: next_occurrence })
}

/// `mod_id` and `gen_chat` come from `Data` (parsed once at startup); `MOD_CHAT` is only
/// used here, so it is read from the environment.
pub(crate) async fn check_birthday(http: &serenity::Http, mod_id: serenity::RoleId, gen_chat: ChannelId) {
    let contents = match tokio::fs::read_to_string(EVENT_FILE).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(file = %EVENT_FILE, "event file not found — no birthdays to check");
            return;
        }
        Err(e) => {
            // Don't rewrite a file we couldn't read.
            tracing::warn!(file = %EVENT_FILE, error = %e, "could not read event file — skipping birthday check");
            return;
        }
    };
    let mut lines = parse_file(&contents);

    let tz_offset: i64 = env::var("TZ_OFFSET_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TZ_OFFSET_HOURS);
    let today = local_today(Utc::now(), tz_offset);
    let mod_chat = env::var("MOD_CHAT").ok().as_deref().and_then(parse_id).map(ChannelId::new);

    for line in &mut lines {
        let Line::Event(row) = line else { continue };
        let Some(notice) = plan_row(row, today) else { continue };

        let (channel, desc) = match notice {
            Notice::Remind { days, date } => {
                let Some(mod_chat) = mod_chat else {
                    tracing::warn!("reminder: MOD_CHAT unset or invalid — skipping birthday reminder");
                    continue;
                };
                (mod_chat, format!("Hey <@&{mod_id}>, {}'s Birthday is coming up {}! ({date})", row.name, days_phrase(days)))
            }
            Notice::Ping => (
                gen_chat,
                format!("Heyyyyyyy, its someone's special day!! It's {}'s (<@{}>) Birthday!!!", row.name, row.user_id),
            ),
        };

        match send_reminder(http, channel, &desc).await {
            Ok(()) => match notice {
                Notice::Remind { .. } => row.reminded = true,
                Notice::Ping => row.pinged = true,
            },
            Err(e) => tracing::warn!(error = %e, ?notice, "failed to send birthday notice — will retry next run"),
        }
    }

    let updated = serialize(&lines);
    if updated != contents {
        write_atomic(EVENT_FILE, updated).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn local_today_applies_offset_sign_correctly() {
        // 02:00 UTC on Jan 2 is still Jan 1 in EDT (UTC-4).
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 2, 0, 0).unwrap();
        assert_eq!(local_today(now, -4), date(2026, 1, 1));
        // 22:00 UTC on Jan 1 is already Jan 2 at UTC+4.
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 22, 0, 0).unwrap();
        assert_eq!(local_today(now, 4), date(2026, 1, 2));
    }

    #[test]
    fn flags_kept_for_new_year_birthday_inside_window() {
        // Seen from Dec 22, a Jan 5 birthday "passed" this year but is 14 days out — keep flags.
        let days_until = (date(2027, 1, 5) - date(2026, 12, 22)).num_days();
        assert!(!flags_expired(true, days_until));
    }

    #[test]
    fn parse_line_keeps_commas_in_names() {
        let Line::Event(row) = parse_line("03-14,Smith, John,123,1,0") else { panic!("should parse") };
        assert_eq!(row.name, "Smith, John");
        assert_eq!(row.user_id, "123");
        assert!(row.reminded && !row.pinged);
    }

    #[test]
    fn malformed_lines_survive_a_rewrite() {
        let contents = "-\n03-14,Ann,1,0,0\ngarbage line\n12-01,Bob, Jr,2,1,1";
        let lines = parse_file(contents);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], Line::Raw("garbage line".to_string()));
        assert_eq!(serialize(&lines), contents);
    }

    #[test]
    fn parse_file_without_header_keeps_first_row() {
        assert_eq!(parse_file("03-14,Ann,1,0,0").len(), 1);
    }

    #[test]
    fn leap_day_birthday_observed_on_feb_28() {
        assert_eq!(observed_birthday(2027, 2, 29), Some(date(2027, 2, 28)));
        assert_eq!(observed_birthday(2028, 2, 29), Some(date(2028, 2, 29)));
        assert_eq!(observed_birthday(2027, 2, 30), None);
    }

    #[test]
    fn days_phrase_matches_distance() {
        assert_eq!(days_phrase(1), "tomorrow");
        assert_eq!(days_phrase(14), "in 14 days");
    }

    fn row(date: &str, reminded: bool, pinged: bool) -> EventRow {
        EventRow { date: date.into(), name: "Ann".into(), user_id: "1".into(), reminded, pinged }
    }

    #[test]
    fn plan_row_across_the_birthday() {
        // 14 days out: advance reminder (flag not set until the send succeeds).
        let mut r = row("03-15", false, false);
        assert_eq!(plan_row(&mut r, date(2026, 3, 1)), Some(Notice::Remind { days: 14, date: date(2026, 3, 15) }));
        assert!(!r.reminded);
        // 15 days out: nothing yet.
        assert_eq!(plan_row(&mut row("03-16", false, false), date(2026, 3, 1)), None);
        // Already reminded: nothing until the day itself.
        let mut r = row("03-15", true, false);
        assert_eq!(plan_row(&mut r, date(2026, 3, 10)), None);
        // On the day: ping, and the advance reminder is considered done.
        assert_eq!(plan_row(&mut r, date(2026, 3, 15)), Some(Notice::Ping));
        assert!(r.reminded);
        // Day after (both sent): flags reset for next year, nothing to send.
        let mut r = row("03-15", true, true);
        assert_eq!(plan_row(&mut r, date(2026, 3, 16)), None);
        assert!(!r.reminded && !r.pinged);
    }

    #[test]
    fn plan_row_wraps_year_and_skips_invalid_dates() {
        let mut r = row("01-05", false, false);
        assert_eq!(plan_row(&mut r, date(2026, 12, 25)), Some(Notice::Remind { days: 11, date: date(2027, 1, 5) }));
        assert_eq!(plan_row(&mut row("13-40", false, false), date(2026, 3, 1)), None);
        assert_eq!(plan_row(&mut row("garbage", false, false), date(2026, 3, 1)), None);
    }

    #[test]
    fn flags_reset_after_birthday_passes() {
        // The day after a Jan 5 birthday, the next one is ~a year out — reset.
        let days_until = (date(2027, 1, 5) - date(2026, 1, 6)).num_days();
        assert!(flags_expired(true, days_until));
        // Upcoming birthday this year never resets.
        assert!(!flags_expired(false, 30));
    }
}
