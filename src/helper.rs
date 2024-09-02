//!---------------------------------------------------------------------!
//! This file contains a collection of internal functions to help       !
//! reduce repetitive code                                              !
//!                                                                     !
//! Commands:                                                           !
//!     [ ] - parse_user_mention                                        !
//!---------------------------------------------------------------------!

use chrono::{Datelike, NaiveDate, TimeDelta, Utc};

pub fn parse_user_mention(user_mention: String) -> u64 {
    user_mention
        .replace(&['<', '>', '!', '@', '&'][..], "")
        .parse::<u64>()
        .unwrap_or(1)
}

pub fn get_current_date() -> String {
    let today = Utc::now() - TimeDelta::hours(4);
    format!("{}-{}-{}", today.year(), today.month(), today.day())
}

pub fn get_reminder_date(date_str: &str) -> String {
    let date_parsed = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap();
    let reminder = date_parsed - TimeDelta::days(14);
    format!(
        "{}-{}-{}",
        reminder.year(),
        reminder.month(),
        reminder.day()
    )
}
