//!---------------------------------------------------------------------!
//! This file contains a collection of MODERATOR related commands to    !
//! to better serve the facilitation of professorBot                    !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - give_creds                                                !
//!     [x] - take_creds                                                !
//!     [ ] - give_wishes                                               !
//!     [ ] - refund_tickets                                            !
//!---------------------------------------------------------------------!

use crate::clips::check_mod;
use crate::data;
use crate::helper::parse_user_mention;
use crate::{serenity, Context, Error};
use poise::serenity_prelude::UserId;

async fn modify_creds(
    ctx: Context<'_>,
    mentioned: String,
    amount: u32,
    is_give: bool,
) -> Result<(), Error> {
    let title = if is_give { "Give Creds" } else { "Take Creds" };

    if amount > 10000 {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(title)
                    .description("The max amount allowed is 10000.")
                    .image("https://cdn.discordapp.com/attachments/1196582162057662484/1205685838877433866/tenor_2.gif?ex=65d94570&is=65c6d070&hm=be06433cb7dd2c592468560dfffbc5ce6c294582db38f177028ba80a46f67a43&")
                    .color(data::EMBED_ERROR)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
        return Ok(());
    }

    let guild_members = ctx
        .guild_id()
        .unwrap()
        .members(ctx.http(), None, None)
        .await;

    let mut guild_ids: Vec<UserId> = Vec::new();
    for member in guild_members.iter() {
        for profile in member {
            guild_ids.push(profile.user.id);
        }
    }

    let data = &ctx.data().users;
    let mentioned_list: Vec<&str> = mentioned.split(' ').collect();
    let mentioned_size = mentioned_list.len();

    let mut processed_list: Vec<u64> = Vec::new();
    for mentioned_user in mentioned_list {
        let parsed_id = match parse_user_mention(mentioned_user) {
            Some(id) => id,
            None => continue,
        };
        let user_id = UserId::from(parsed_id);

        if !guild_ids.contains(&user_id) {
            continue;
        }

        if !data.contains_key(&user_id) {
            data.insert(user_id, Default::default());

            ctx.send(
                poise::CreateReply::default()
                    .content(format!("<@{}>", user_id))
                    .embed(
                        serenity::CreateEmbed::new()
                            .title("Account Created!")
                            .description(format!("Welcome <@{}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in https://discord.com/channels/1194668798830194850/1194700756306108437", user_id))
                            .image("https://gifdb.com/images/high/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif")
                            .color(data::EMBED_DEFAULT),
                    ),
            )
            .await?;
        }

        let u = data.get(&user_id).unwrap();
        let mut user_data = u.write().await;

        if is_give {
            user_data.add_creds(amount as i32);
        } else {
            user_data.sub_creds(amount as i32);
        }
        processed_list.push(parsed_id);
    }

    let process_size = processed_list.len();
    let mut pre_text = String::new();
    let mut desc = String::new();

    if processed_list.is_empty() {
        desc += if is_give { "No one got creds..." } else { "No one lost creds..." };
    } else {
        let action = if is_give {
            format!("Moderator <@{}> gave {} creds to ", ctx.author().id, amount)
        } else {
            format!("Moderator <@{}> took {} creds from ", ctx.author().id, amount)
        };
        desc += &action;
        for id in processed_list {
            pre_text += &format!("<@{}> ", id);
            desc += &format!("<@{}> ", id);
        }
    }

    let image = if is_give {
        "https://cdn.discordapp.com/attachments/1196582162057662484/1205685388157653022/zVdLFbp.gif?ex=65d94505&is=65c6d005&hm=690faecbed4018602cc94a5f7a9db1ff6527d4202a71ba80f27d912d36de3c7e&"
    } else {
        "https://cdn.discordapp.com/attachments/1196582162057662484/1205689656268824596/7Z7b-ezgif.com-video-to-gif-converter.gif?ex=65d948fe&is=65c6d3fe&hm=d3ac81f31552010a87cb5bb894ebef6af6f2e3fc73c223abff1b19ab712c0ae8&"
    };

    ctx.send(
        poise::CreateReply::default()
            .content(pre_text)
            .embed(
                serenity::CreateEmbed::new()
                    .title(title)
                    .description(desc)
                    .image(image)
                    .color(data::EMBED_MOD)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
    )
    .await?;

    if process_size != mentioned_size {
        let note = if is_give {
            format!(
                "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not get creds.\n",
                ctx.author().id, mentioned_size - process_size
            )
        } else {
            format!(
                "**(!) NOTE** <@{}>\n     **{}** @Mentions did not get processed... double check who did \n     not lose creds.\n",
                ctx.author().id, mentioned_size - process_size
            )
        };
        ctx.send(poise::CreateReply::default().content(note)).await?;
    }

    Ok(())
}

/// [!] MODERATOR - reward a user with creds
#[poise::command(slash_command, check = "check_mod")]
pub async fn give_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "amount of creds to give (max: 10000)"] amount: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, amount, true).await
}

/// [!] MODERATOR - take creds from a user
#[poise::command(slash_command, check = "check_mod")]
pub async fn take_creds(
    ctx: Context<'_>,
    #[description = "@username | example: @UwUntu @Rustybamboo"] mentioned: String,
    #[description = "amount of creds to take (max: 10000)"] amount: u32,
) -> Result<(), Error> {
    modify_creds(ctx, mentioned, amount, false).await
}
