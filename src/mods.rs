//! Moderator commands: `give_creds`, `take_creds`, `prune`.

use crate::data;
use crate::helper::{check_mod, default_footer, parse_user_mention, CREDS_PER_DOLLAR};
use crate::{serenity, Context, Error};
use poise::serenity_prelude::{futures, ChannelId, MessageId, UserId};
use std::time::Duration;

/// Maximum dollar amount (not creds) a moderator may transfer in one give/take.
const MAX_DOLLAR_TRANSFER: u32 = 100_000;

#[derive(Copy, Clone)]
enum CreditOp { Give, Take }

impl CreditOp {
    const fn title(self) -> &'static str {
        match self {
            Self::Give => "Give Creds",
            Self::Take => "Take Creds",
        }
    }

    const fn nobody_text(self) -> &'static str {
        match self {
            Self::Give => "No one got creds...",
            Self::Take => "No one lost creds...",
        }
    }

    /// "gave $5 to" / "took $5 from".
    fn action_text(self, dollars: u32) -> String {
        match self {
            Self::Give => format!("gave ${dollars} to"),
            Self::Take => format!("took ${dollars} from"),
        }
    }

    /// Verb for the "did not get/lose creds" note.
    const fn verb(self) -> &'static str {
        match self {
            Self::Give => "get",
            Self::Take => "lose",
        }
    }

    const fn image(self) -> &'static str {
        match self {
            Self::Give => "https://cdn.discordapp.com/attachments/1196582162057662484/1205685388157653022/zVdLFbp.gif?ex=65d94505&is=65c6d005&hm=690faecbed4018602cc94a5f7a9db1ff6527d4202a71ba80f27d912d36de3c7e&",
            Self::Take => "https://cdn.discordapp.com/attachments/1196582162057662484/1205689656268824596/7Z7b-ezgif.com-video-to-gif-converter.gif?ex=65d948fe&is=65c6d3fe&hm=d3ac81f31552010a87cb5bb894ebef6af6f2e3fc73c223abff1b19ab712c0ae8&",
        }
    }

    const fn apply(self, user: &mut data::UserData, creds: i32) {
        match self {
            Self::Give => { user.add_creds(creds); }
            Self::Take => { user.sub_creds(creds); }
        }
    }
}

/// Cred amount for a dollar transfer, or `None` outside `1..=MAX_DOLLAR_TRANSFER`.
fn dollars_to_creds(dollars: u32) -> Option<i32> {
    if !(1..=MAX_DOLLAR_TRANSFER).contains(&dollars) {
        return None;
    }
    i32::try_from(dollars).ok()?.checked_mul(CREDS_PER_DOLLAR)
}

/// Unique user ids mentioned in `text`, plus the number of distinct tokens that weren't
/// user mentions. `<@1>` and `<@!1>` are the same user, so they count once.
fn parse_mentions(text: &str) -> (Vec<UserId>, usize) {
    let mut ids: Vec<UserId> = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for token in text.split_whitespace() {
        match parse_user_mention(token) {
            Some(id) => {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            None => {
                if !unparsed.contains(&token) {
                    unparsed.push(token);
                }
            }
        }
    }
    (ids, unparsed.len())
}

async fn modify_creds(
    ctx: Context<'_>,
    mentioned: String,
    dollars: u32,
    op: CreditOp,
) -> Result<(), Error> {
    let Some(amount) = dollars_to_creds(dollars) else {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(op.title())
                    .description(format!("Amount must be between $1 and ${MAX_DOLLAR_TRANSFER}."))
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205685838877433866/tenor_2.gif?ex=65d94570&is=65c6d070&hm=be06433cb7dd2c592468560dfffbc5ce6c294582db38f177028ba80a46f67a43&")
                    .color(data::EMBED_ERROR)
                    .footer(default_footer()),
            ),
        )
        .await?;
        return Ok(());
    };

    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("This command can only be used in a server.").await?;
        return Ok(());
    };

    // Deduped so mentioning someone twice (in either mention form) applies the amount once.
    let (candidates, unparsed) = parse_mentions(&mentioned);
    let mentioned_size = candidates.len() + unparsed;

    // Membership lookups below can take a few round trips — don't miss the 3s reply window.
    ctx.defer().await?;

    // Per-user lookups (cache first, then GET member), run concurrently, instead of listing the
    // guild: listing caps at 1000 members and needs the privileged GUILD_MEMBERS intent.
    let lookups = futures::future::join_all(candidates.iter().map(|&id| guild_id.member(ctx, id))).await;

    let mut processed_list: Vec<UserId> = Vec::new();
    for (user_id, lookup) in candidates.into_iter().zip(lookups) {
        if let Err(e) = lookup {
            tracing::info!(user = %user_id, error = %e, "modify_creds: mention is not a guild member — skipped");
            continue;
        }

        let (u, created) = ctx.data().get_or_create_user(user_id);
        // Credit before announcing so a failed welcome send can't abort midway through the list.
        op.apply(&mut *u.write().await, amount);
        processed_list.push(user_id);

        if created {
            if let Err(e) = data::send_welcome(ctx, user_id).await {
                tracing::warn!(user = %user_id, error = %e, "modify_creds: failed to send welcome message");
            }
        }
    }

    let process_size = processed_list.len();
    let mut pre_text = String::new();
    let desc = if processed_list.is_empty() {
        op.nobody_text().to_string()
    } else {
        let mut desc = format!("Moderator <@{}> {} ", ctx.author().id, op.action_text(dollars));
        for id in processed_list {
            pre_text += &format!("<@{id}> ");
            desc += &format!("<@{id}> ");
        }
        desc
    };

    ctx.send(
        poise::CreateReply::default()
            .content(pre_text)
            .embed(
                serenity::CreateEmbed::new()
                    .title(op.title())
                    .description(desc)
                    .image(op.image())
                    .color(data::EMBED_MOD)
                    .footer(default_footer()),
            ),
    )
    .await?;

    if process_size != mentioned_size {
        let note = format!(
            "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not {} creds.\n",
            ctx.author().id, mentioned_size - process_size, op.verb()
        );
        ctx.send(poise::CreateReply::default().content(note)).await?;
    }

    Ok(())
}

/// [!] MODERATOR - reward a user with creds
#[poise::command(slash_command, check = "check_mod")]
pub async fn give_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "dollar amount to give (max: $100,000)"] // keep in sync with MAX_DOLLAR_TRANSFER
    dollars: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, dollars, CreditOp::Give).await
}

/// [!] MODERATOR - take creds from a user
#[poise::command(slash_command, check = "check_mod")]
pub async fn take_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "dollar amount to take (max: $100,000)"] // keep in sync with MAX_DOLLAR_TRANSFER
    dollars: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, dollars, CreditOp::Take).await
}

/// Discord rejects bulk deletes of messages older than 14 days. An hour of margin keeps a
/// message from ageing out between being fetched and being deleted.
const BULK_DELETE_MAX_AGE_SECS: i64 = 14 * 24 * 60 * 60 - 60 * 60;
/// Messages fetched (and bulk-deleted) per round trip — Discord's maximum for both.
const PRUNE_BATCH: u8 = 100;
/// How long the moderator has to confirm a prune before it's cancelled.
const PRUNE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

/// Splits message ids into those young enough for one bulk-delete call and older ones that
/// must be deleted individually. Age comes from the snowflake, so no message fetch is needed.
fn split_by_bulk_age(ids: &[MessageId], now_unix: i64) -> (Vec<MessageId>, Vec<MessageId>) {
    ids.iter().partition(|id| now_unix - id.created_at().unix_timestamp() < BULK_DELETE_MAX_AGE_SECS)
}

#[derive(Debug, Default)]
struct PruneOutcome {
    deleted: usize,
    failed: usize,
    /// Why pruning stopped early, if it did.
    stopped: Option<String>,
}

/// Deletes every message in `channel_id` older than the command, newest first: recent ones
/// in bulk, older ones one at a time. Messages that can't be deleted (e.g. some system
/// messages) are counted and skipped; if a whole batch fails with nothing deleted yet, the
/// bot most likely lacks Manage Messages, so it stops instead of failing every message.
async fn prune_channel(ctx: Context<'_>, channel_id: ChannelId) -> PruneOutcome {
    let mut outcome = PruneOutcome::default();
    let mut before: Option<MessageId> = None;

    loop {
        let mut query = serenity::GetMessages::new().limit(PRUNE_BATCH);
        if let Some(id) = before {
            query = query.before(id);
        }
        let batch = match channel_id.messages(ctx, query).await {
            Ok(batch) => batch,
            Err(e) => {
                outcome.stopped = Some(format!("couldn't read channel history: {e}"));
                break;
            }
        };
        // Page by the oldest id seen, so undeletable messages can't be fetched forever.
        let Some(oldest) = batch.last() else { break };
        before = Some(oldest.id);

        let ids: Vec<MessageId> = batch.iter().map(|m| m.id).collect();
        let (recent, old) = split_by_bulk_age(&ids, chrono::Utc::now().timestamp());
        let mut last_error = None;

        if !recent.is_empty() {
            match channel_id.delete_messages(ctx.http(), &recent).await {
                Ok(()) => outcome.deleted += recent.len(),
                Err(e) => {
                    outcome.failed += recent.len();
                    last_error = Some(e.to_string());
                }
            }
        }
        for id in old {
            match channel_id.delete_message(ctx.http(), id).await {
                Ok(()) => outcome.deleted += 1,
                Err(e) => {
                    outcome.failed += 1;
                    last_error = Some(e.to_string());
                }
            }
        }

        if outcome.deleted == 0 {
            if let Some(e) = last_error {
                outcome.stopped = Some(format!("nothing could be deleted — does the bot have Manage Messages? ({e})"));
                break;
            }
        }
    }
    outcome
}

fn prune_embed(desc: impl Into<String>, color: serenity::Color) -> serenity::CreateEmbed {
    serenity::CreateEmbed::new()
        .title("Prune")
        .description(desc)
        .color(color)
        .footer(default_footer())
}

/// [!] MODERATOR - delete every message in this channel
#[poise::command(slash_command, guild_only, check = "check_mod")]
pub async fn prune(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();
    // Ids are unique per invocation so a stale prompt's buttons can't confirm a newer one.
    let confirm_id = format!("{}-prune-confirm", ctx.id());
    let cancel_id = format!("{}-prune-cancel", ctx.id());

    // Ephemeral: only the moderator sees the prompt, and it isn't part of channel history.
    let reply = ctx.send(poise::CreateReply::default()
        .ephemeral(true)
        .embed(prune_embed(
            format!("Delete **every message** in <#{channel_id}>? This can't be undone."),
            data::EMBED_MOD,
        ))
        .components(vec![serenity::CreateActionRow::Buttons(vec![
            serenity::CreateButton::new(&confirm_id).label("Prune").style(serenity::ButtonStyle::Danger),
            serenity::CreateButton::new(&cancel_id).label("Cancel").style(serenity::ButtonStyle::Secondary),
        ])])
    ).await?;

    let press = {
        let (confirm_id, cancel_id) = (confirm_id.clone(), cancel_id.clone());
        serenity::ComponentInteractionCollector::new(ctx)
            .author_id(ctx.author().id)
            .channel_id(channel_id)
            .timeout(PRUNE_CONFIRM_TIMEOUT)
            .filter(move |p| p.data.custom_id == confirm_id || p.data.custom_id == cancel_id)
            .await
    };
    if let Some(p) = &press {
        p.create_response(ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();
    }
    let confirmed = press.is_some_and(|p| p.data.custom_id == confirm_id);
    if !confirmed {
        reply.edit(ctx, poise::CreateReply::default()
            .embed(prune_embed("Prune cancelled — nothing was deleted.", data::EMBED_ERROR))
            .components(vec![])
        ).await?;
        return Ok(());
    }

    reply.edit(ctx, poise::CreateReply::default()
        .embed(prune_embed(
            format!("Pruning <#{channel_id}>… messages older than 14 days are deleted one by one, so this can take a while."),
            data::EMBED_DEFAULT,
        ))
        .components(vec![])
    ).await?;

    let outcome = prune_channel(ctx, channel_id).await;
    tracing::info!(
        channel = %channel_id,
        moderator = %ctx.author().id,
        deleted = outcome.deleted,
        failed = outcome.failed,
        stopped = ?outcome.stopped,
        "prune finished",
    );

    let mut desc = format!("Deleted **{}** message(s) from <#{channel_id}>.", outcome.deleted);
    if outcome.failed > 0 {
        desc += &format!("\n**{}** couldn't be deleted.", outcome.failed);
    }
    if let Some(reason) = &outcome.stopped {
        desc += &format!("\nStopped early: {reason}");
    }
    let color = if outcome.stopped.is_some() { data::EMBED_ERROR } else { data::EMBED_CYAN };
    // The interaction token expires after 15 minutes; a very long prune can outlive it.
    if let Err(e) = reply.edit(ctx, poise::CreateReply::default().embed(prune_embed(desc, color))).await {
        tracing::warn!(error = %e, "prune: couldn't post the result (interaction expired?)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollars_to_creds_boundaries() {
        assert_eq!(dollars_to_creds(0), None);
        assert_eq!(dollars_to_creds(1), Some(CREDS_PER_DOLLAR));
        assert_eq!(dollars_to_creds(MAX_DOLLAR_TRANSFER), Some(100_000 * CREDS_PER_DOLLAR));
        assert_eq!(dollars_to_creds(MAX_DOLLAR_TRANSFER + 1), None);
    }

    #[test]
    fn parse_mentions_dedups_across_forms_and_counts_junk_once() {
        let (ids, unparsed) = parse_mentions("<@1>  <@!1> <@2> junk junk <@&3>");
        assert_eq!(ids, vec![UserId::new(1), UserId::new(2)]);
        assert_eq!(unparsed, 2); // "junk" once, plus the role mention
        assert_eq!(parse_mentions("   "), (vec![], 0));
    }

    /// Snowflake for a message sent at `unix_secs` (Discord epoch is 2015-01-01).
    fn message_at(unix_secs: i64) -> MessageId {
        const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;
        MessageId::new(u64::try_from((unix_secs * 1000 - DISCORD_EPOCH_MS) << 22).unwrap())
    }

    #[test]
    fn split_by_bulk_age_respects_fourteen_day_limit() {
        let now = 1_800_000_000;
        let day = 24 * 60 * 60;
        let fresh = message_at(now - 60);
        let edge = message_at(now - BULK_DELETE_MAX_AGE_SECS + 1);
        let too_old = message_at(now - 14 * day);
        let ancient = message_at(now - 400 * day);
        let (recent, old) = split_by_bulk_age(&[fresh, edge, too_old, ancient], now);
        assert_eq!(recent, vec![fresh, edge]);
        assert_eq!(old, vec![too_old, ancient]);
    }

    #[test]
    fn credit_op_apply() {
        let mut u = data::UserData::default();
        CreditOp::Give.apply(&mut u, 500);
        CreditOp::Take.apply(&mut u, 200);
        assert_eq!(u.get_creds(), 300);
    }
}
