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
use rand::{thread_rng, Rng};

#[poise::command(slash_command)]
pub async fn wish(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

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

// big pity = 5* pity out of 60, small pity = 4* pity out of 10
// 43 = soft pity, below soft pity is 0.6%, above is 0.6% + 6x
pub fn pull(big_pity: i32, small_pity: i32, guarentee: bool) -> String {
    let mut item: String = String::new();
    let mut rng = rand::thread_rng();

    // 5* probability check
    if big_pity < 43 && (rng.gen::<(f64)>() < 0.006) {
        if guarentee || rng.gen::<(f64)>() < 0.5 {
            item = format!("{}-{}", "5", "Master Ball");
        } else {
            item = format!("{}-{}", "5", "Rare Candy");
        }
    }

    if (43..59).contains(&big_pity) {
        let probability = 0.006 + 0.06 * (big_pity - 42) as f64;
        if rng.gen::<(f64)>() < probability || rng.gen::<(f64)>() < 0.5 {
            item = format!("{}-{}", "5", "Master Ball");
        } else {
            item = format!("{}-{}", "5", "Rare Candy");
        }
    }

    if big_pity == 59 {
        if guarentee || rng.gen::<(f64)>() < 0.5 {
            item = format!("{}-{}", "5", "Master Ball");
        } else {
            item = format!("{}-{}", "5", "Rare Candy");
        }
    }

    if item != String::new() {
        return item;
    }

    // 4* probability check
    if small_pity < 8 {
        if rng.gen::<(f64)>() < 0.051 && rng.gen::<(f64)>() < 0.55 {
            item = format!("{}-{}", "4", randomFourStar());
        } else {
            item = format!("{}-{}", "4", randomThreeStar());
        }
    }

    if small_pity == 8 {
        if rng.gen::<(f64)>() < 0.561 && rng.gen::<(f64)>() < 0.55 {
            item = format!("{}-{}", "4", randomFourStar());
        } else {
            item = format!("{}-{}", "4", randomThreeStar());
        }
    }

    if small_pity > 8 {
        if rng.gen::<(f64)>() < 0.55 {
            item = format!("{}-{}", "4", randomFourStar());
        } else {
            item = format!("{}-{}", "4", randomThreeStar());
        }
    }

    if item == String::new() {
        item = format!("{}-{}", "0", "Nothing")
    }

    item
}

pub fn randomThreeStar() -> String {
    let items = Vec::from([
        "Poke Ball",
        "Great Ball",
        "Potion",
        "Super Potion",
        "Small Shield",
    ]);

    let mut rng = rand::thread_rng();
    let random_item = rng.gen_range(0..items.len());
    items[random_item].to_string()
}

pub fn randomFourStar() -> String {
    let items = Vec::from([
        "Great Ball",
        "Ultra Ball",
        "Super Potion",
        "Max Potion",
        "Big Shield",
    ]);

    let mut rng = rand::thread_rng();
    let random_item = rng.gen_range(0..items.len());
    items[random_item].to_string()
}
