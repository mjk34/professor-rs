//! Shared formatting, embed utilities, and command checks.

use crate::{Context, Error};
use poise::serenity_prelude as serenity;

/// Command check: the author holds the configured moderator role in this guild.
pub async fn check_mod(ctx: Context<'_>) -> Result<bool, Error> {
    let Some(guild_id) = ctx.guild_id() else { return Ok(false); };
    Ok(ctx.author().has_role(ctx, guild_id, ctx.data().mod_id).await.unwrap_or(false))
}

/// Backslash-escapes Discord markdown so user text renders literally (and can't open a
/// masked link or close a surrounding one).
pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '*' | '_' | '~' | '`' | '|' | '[' | ']' | '(' | ')' | '<' | '>') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Write-then-rename so a crash mid-write can't leave a truncated file behind.
/// Stages to `<path>.tmp` beside the target.
pub async fn write_atomic(path: &str, contents: String) {
    let tmp = format!("{path}.tmp");
    if let Err(e) = tokio::fs::write(&tmp, contents).await {
        tracing::error!(file = %tmp, error = %e, "atomic write: staging write failed");
        return;
    }
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        tracing::error!(file = %path, error = %e, "atomic write: rename failed");
    }
}

/// Resolves users concurrently (cache first, then HTTP); `None` where a lookup failed.
pub async fn fetch_users(
    ctx: &serenity::Context,
    ids: impl IntoIterator<Item = serenity::UserId>,
) -> Vec<Option<serenity::User>> {
    serenity::futures::future::join_all(ids.into_iter().map(|id| async move { id.to_user(ctx).await.ok() })).await
}

/// Username of a lookup result from [`fetch_users`], or `"Unknown"`.
pub fn user_name(user: Option<&serenity::User>) -> String {
    user.map_or_else(|| "Unknown".to_string(), |u| u.name.clone())
}

/// Parses a Discord snowflake: ASCII digits only, non-zero (serenity's id types panic on 0).
pub fn parse_id(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|&id| id != 0)
}

/// Parses a user mention (`<@id>`, `<@!id>`) or a bare id. Role (`<@&id>`) and channel
/// mentions are rejected.
pub fn parse_user_mention(user_mention: &str) -> Option<serenity::UserId> {
    let inner = user_mention
        .strip_prefix("<@")
        .and_then(|rest| rest.strip_suffix('>'))
        .map_or(user_mention, |id| id.strip_prefix('!').unwrap_or(id));
    parse_id(inner).map(serenity::UserId::new)
}

/// Creds per notional dollar — creds are displayed as cents of a dollar.
pub const CREDS_PER_DOLLAR: i32 = 100;

/// Converts a cred amount to its notional dollar value.
pub fn creds_to_price(creds: f64) -> f64 {
    creds / f64::from(CREDS_PER_DOLLAR)
}

/// Footer credit shown on every bot embed.
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
    fn escape_markdown_neutralises_links_and_formatting() {
        assert_eq!(escape_markdown("plain text"), "plain text");
        assert_eq!(escape_markdown("**bold** [a](b)"), r"\*\*bold\*\* \[a\]\(b\)");
        assert_eq!(escape_markdown(r"a\b"), r"a\\b");
    }

    #[test]
    fn parse_id_requires_nonzero_digits() {
        assert_eq!(parse_id("42"), Some(42));
        assert_eq!(parse_id("0"), None);
        assert_eq!(parse_id(""), None);
        assert_eq!(parse_id(" 42"), None);
        assert_eq!(parse_id("99999999999999999999999"), None); // overflows u64
    }

    #[tokio::test]
    async fn write_atomic_replaces_file_and_survives_bad_dir() {
        let dir = std::env::temp_dir().join(format!("professor-rs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.json");
        let path = path.to_str().unwrap();
        write_atomic(path, "one".into()).await;
        write_atomic(path, "two".into()).await;
        assert_eq!(std::fs::read_to_string(path).unwrap(), "two");
        assert!(!std::path::Path::new(&format!("{path}.tmp")).exists());
        // Unwritable target: logs and returns without panicking or creating anything.
        let missing = dir.join("no-such-dir").join("f.json");
        write_atomic(missing.to_str().unwrap(), "x".into()).await;
        assert!(!missing.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_user_mention_formats() {
        let id = Some(serenity::UserId::new(123_456_789));
        assert_eq!(parse_user_mention("<@123456789>"), id);
        assert_eq!(parse_user_mention("<@!123456789>"), id);
        assert_eq!(parse_user_mention("123456789"), id);
        assert_eq!(parse_user_mention("notanumber"), None);
        assert_eq!(parse_user_mention("0"), None);
        assert_eq!(parse_user_mention("<@0>"), None);
        // Role mentions, channel mentions and junk must not be read as user ids.
        assert_eq!(parse_user_mention("<@&123456789>"), None);
        assert_eq!(parse_user_mention("<#123456789>"), None);
        assert_eq!(parse_user_mention("1<2>3"), None);
        assert_eq!(parse_user_mention("+123"), None);
    }
}
