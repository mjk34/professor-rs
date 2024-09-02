//!---------------------------------------------------------------------!
//! This file contains a collection of functions related to wishing for !
//! items: pokeballs, potions, items, usables...                        !
//!                                                                     !
//! Commands:                                                           !
//!     [-] - wish                                                      !
//!     [ ] - ....                                                      !
//!---------------------------------------------------------------------!

use crate::data::EMBED_ERROR;
use crate::serenity;
use crate::{Context, Error};

#[poise::command(slash_command)]
async fn wish(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    // let team = user_data.event.get_team();
    // let buddy = user_data.event.get_buddy();

    let desc = "Please wait for wishing system";
    let embed_color = EMBED_ERROR;
    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Test-Bag")
                .description(desc)
                .color(embed_color)
                .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}
