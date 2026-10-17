//! Moderator commands: `give_creds`, `take_creds`.

use crate::data;
use crate::helper::{check_mod, default_footer, parse_user_mention, CREDS_PER_DOLLAR};
use crate::{serenity, Context, Error};
use poise::serenity_prelude::{futures, UserId};

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

    #[test]
    fn credit_op_apply() {
        let mut u = data::UserData::default();
        CreditOp::Give.apply(&mut u, 500);
        CreditOp::Take.apply(&mut u, 200);
        assert_eq!(u.get_creds(), 300);
    }
}
