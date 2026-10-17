//! Shared formatting and embed utilities.

use poise::serenity_prelude as serenity;

/// Parses `<@id>`, `<@!id>` or a bare id. Rejects 0, which serenity's `UserId` panics on.
pub fn parse_user_mention(user_mention: &str) -> Option<u64> {
    user_mention
        .replace(&['<', '>', '!', '@', '&'][..], "")
        .parse::<u64>()
        .ok()
        .filter(|&id| id != 0)
}

/// Creds per notional dollar — creds are displayed as cents of a dollar.
pub const CREDS_PER_DOLLAR: i32 = 100;

/// Converts a cred amount to its notional dollar value.
pub fn creds_to_price(creds: f64) -> f64 {
    creds / f64::from(CREDS_PER_DOLLAR)
}

pub fn default_footer() -> serenity::CreateEmbedFooter {
    serenity::CreateEmbedFooter::new("@~ powered by UwUntu & RustyBamboo")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creds_to_price_converts_cents() {
        assert_eq!(creds_to_price(1000.0), 10.0);
        assert_eq!(creds_to_price(4250.0), 42.50);
    }

    #[test]
    fn parse_user_mention_formats() {
        assert_eq!(parse_user_mention("<@123456789>"), Some(123_456_789));
        assert_eq!(parse_user_mention("<@!123456789>"), Some(123_456_789));
        assert_eq!(parse_user_mention("123456789"), Some(123_456_789));
        assert_eq!(parse_user_mention("notanumber"), None);
        assert_eq!(parse_user_mention("0"), None);
        assert_eq!(parse_user_mention("<@0>"), None);
    }
}
