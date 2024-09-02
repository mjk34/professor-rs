//!---------------------------------------------------------------------!
//! This file contains a collection of basic pokemon related commands   !
//! to start, check pokemon and bags                                    !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - search_pokemon                                            !
//!     [x] - choose_starter                                            !
//!     [x] - buddy                                                     !
//!     [-] - test-bag         //remove after testing                   !
//!     [x] - switch_buddy                                              !
//!     [x] - team                                                      !
//!     [-] - pre_populate     //pre-populate items                     !
//!---------------------------------------------------------------------!

use crate::data::{self, EMBED_ERROR};
use crate::poke_helper::{generate_hp, get_pokedata, get_type_color, spawn_pokemon};
use crate::serenity;
use crate::{Context, Error};
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::EditMessage;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use tokio::sync::RwLock;

/// look up a pokemon by name or by pokedex index entry
#[poise::command(slash_command)]
pub async fn search_pokemon(
    ctx: Context<'_>,
    #[description = "name of the pokemon e.g. bulbasaur, charmander, squirtle..."]
    pokemon_name: Option<String>,
    #[description = "index entry of the pokemon e.g. 1, 2, 3..."] pokemon_index: Option<usize>,
) -> Result<(), Error> {
    let pokemon = get_pokedata(ctx, pokemon_name, pokemon_index);

    let name: String = pokemon.get_name();
    let index: usize = pokemon.get_index();
    let desc: String = pokemon.get_desc();
    let types: String = pokemon.get_types();
    let sprite: String = pokemon.get_sprite();

    let msg_txt = format!("**{}**: {}\n{}", name, types, desc);
    let poke_color = get_type_color(&types);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("Pokedex no.{}", index))
                .description(msg_txt)
                .color(Color::new(poke_color))
                .thumbnail(sprite)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn choose_starter(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let team = user_data.event.get_team();
    let oak_img = "https://cdn.discordapp.com/attachments/1260223476766343188/1262159431626657912/c8bfe05ab93e2bcb0bc78301c1a3933a.jpg?ex=66959512&is=66944392&hm=921650be90fd6411624a4f5c24cc16adcf8cbec021a053f9d96b64d55c43852c&";

    if team.is_empty() {
        // Oak dialogue
        let oak_text1 = format!(
            "Hello <@{}>!\n\nWelcome to the world of Pokémon!\n\n(hit *continue* at the bottom to go next)",
            user.id
        );
        let oak_text2 = "My name is **Oak**! People call me the Pokémon Prof!\nThis world is inhabited by creatures called Pokémon!". to_string();
        let oak_text3 = "For some people, Pokémon are pets. Other use them for fights. Myself… I study Pokémon as a profession.\n\n".to_string();
        let oak_text4 =
            "You need your own Pokémon for your protection.\n\nI know! There are 3 Pokémon here! Haha! They are inside the Poké Balls.\n\n You can have one! Choose!\n\n"
                .to_string();
        let oak_text5 = format!("Now, <@{}>, which Pokémon do you want?", user.id);
        let oak_text6 = "Your very own Pokémon legend is about to unfold! A world of dreams and adventures with Pokémon awaits! Let's go!".to_string();

        let oak_img = "https://cdn.discordapp.com/attachments/1260223476766343188/1262159431626657912/c8bfe05ab93e2bcb0bc78301c1a3933a.jpg?ex=66959512&is=66944392&hm=921650be90fd6411624a4f5c24cc16adcf8cbec021a053f9d96b64d55c43852c&";
        let poke_img = "https://cdn.discordapp.com/attachments/1260223476766343188/1262159432117387304/nodemaster-pokemon-emerald-starter-selection-screen.jpg?ex=66959512&is=66944392&hm=c1180aaebf110c78a25470e42bbb23afb077140af7a159149c9eead778268569&";

        // continue
        let continue_btn = serenity::CreateButton::new("open_modal")
            .label("continue")
            .custom_id("continue".to_string())
            .style(poise::serenity_prelude::ButtonStyle::Success);

        let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];

        let reply = ctx
            .send(
                poise::CreateReply::default()
                    .content(format!("<@{}>", user.id))
                    .embed(
                        serenity::CreateEmbed::new()
                            .title("????????")
                            .description(oak_text1)
                            .color(data::EMBED_DEFAULT)
                            .thumbnail(user.avatar_url().unwrap_or_default().to_string())
                            .image(oak_img)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    )
                    .components(components),
            )
            .await?;

        let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
        let msg = Arc::clone(&msg_og);
        let mut reactions = msg
            .read()
            .await
            .await_component_interactions(ctx)
            .timeout(Duration::new(120, 0))
            .stream();

        let mut bulbasaur = get_pokedata(ctx, None, Some(1));
        bulbasaur.set_health(thread_rng().gen_range(15..30));

        let mut squirtle = get_pokedata(ctx, None, Some(7));
        squirtle.set_health(thread_rng().gen_range(15..30));

        let mut charmander = get_pokedata(ctx, None, Some(4));
        charmander.set_health(thread_rng().gen_range(15..30));

        let ctx = ctx.serenity_context().clone();

        let user_id = user.id;
        let user_avatar = user.avatar_url().unwrap_or_default().to_string();
        let u = Arc::clone(&u);

        tokio::spawn(async move {
            async fn timeout_exit(
                msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
                ctx: &poise::serenity_prelude::Context,
                user_avatar: &String,
            ) {
                let desc = "[Response timed out... ]".to_string();
                msg.write()
                    .await
                    .edit(
                        ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title("Time Out - Choose Starter".to_string())
                                    .description(&desc)
                                    .thumbnail(user_avatar)
                                    .colour(data::EMBED_ERROR)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();
            }

            let mut timeout_check = true;
            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();
                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "continue" {
                    let continue_btn = serenity::CreateButton::new("open_modal")
                        .label("continue")
                        .custom_id("continue1".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Success);

                    let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Prof Oak".to_string())
                                        .description(&oak_text2)
                                        .image(oak_img)
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_DEFAULT)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(components),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }

            let mut timeout_check = true;

            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();
                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "continue1" {
                    let continue_btn = serenity::CreateButton::new("open_modal")
                        .label("continue")
                        .custom_id("continue2".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Success);

                    let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Prof Oak".to_string())
                                        .description(&oak_text3)
                                        .image(oak_img)
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_DEFAULT)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(components),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }

            let mut timeout_check = true;

            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();
                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "continue2" {
                    let continue_btn = serenity::CreateButton::new("open_modal")
                        .label("continue")
                        .custom_id("continue3".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Success);

                    let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Prof Oak".to_string())
                                        .description(&oak_text4)
                                        .image(oak_img)
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_DEFAULT)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(components),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }

            let mut timeout_check = true;
            let mut not_choose = true;

            // choose pokemon
            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();
                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "continue3" {
                    let mut buttons = Vec::new();

                    let left_btn = serenity::CreateButton::new("open_modal")
                        .label("Left Poké Ball")
                        .custom_id("left".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Primary);
                    buttons.push(left_btn);

                    let center_btn = serenity::CreateButton::new("open_modal")
                        .label("Center Poké Ball")
                        .custom_id("center".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Primary);
                    buttons.push(center_btn);

                    let right_btn = serenity::CreateButton::new("open_modal")
                        .label("Right Poké Ball")
                        .custom_id("right".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Primary);
                    buttons.push(right_btn);

                    let components = vec![serenity::CreateActionRow::Buttons(buttons)];
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Prof Oak".to_string())
                                        .description(&oak_text5)
                                        .image(poke_img)
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_DEFAULT)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(components),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }

            while not_choose {
                while let Some(reaction) = reactions.next().await {
                    reaction
                        .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                        .await
                        .unwrap();
                    let react_id = reaction.member.clone().unwrap_or_default().user.id;

                    let mut buttons = Vec::new();
                    let back_btn = serenity::CreateButton::new("open_modal")
                        .label("back")
                        .custom_id("back".to_string())
                        .style(poise::serenity_prelude::ButtonStyle::Secondary);
                    buttons.push(back_btn);

                    // bulbasaur
                    if react_id == user_id && reaction.data.custom_id.as_str() == "left" {
                        let name = bulbasaur.get_name();
                        let desc = bulbasaur.get_desc();
                        let sprite = bulbasaur.get_sprite();
                        let img = bulbasaur.get_wallpaper();

                        let types = bulbasaur.get_types();
                        let type_split: Vec<&str> = types.split('/').collect();
                        let first_type = type_split
                            .first()
                            .expect("search_Pokemon(): Failed to expand first_type")
                            .to_string();
                        let color = get_type_color(&first_type);

                        let choose_btn = serenity::CreateButton::new("open_modal")
                            .label(format!("Choose {}!", name).as_str())
                            .custom_id(format!("choose-{}", name))
                            .style(poise::serenity_prelude::ButtonStyle::Success);
                        buttons.push(choose_btn);

                        let components = vec![serenity::CreateActionRow::Buttons(buttons)];

                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!("Choose {}?", name))
                                            .description(desc)
                                            .image(img)
                                            .thumbnail(sprite)
                                            .colour(color)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(components),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        not_choose = false;
                        break;
                    }

                    // squirtle
                    if react_id == user_id && reaction.data.custom_id.as_str() == "center" {
                        let name = squirtle.get_name();
                        let desc = squirtle.get_desc();
                        let sprite = squirtle.get_sprite();
                        let img = squirtle.get_wallpaper();

                        let types = squirtle.get_types();
                        let type_split: Vec<&str> = types.split('/').collect();
                        let first_type = type_split
                            .first()
                            .expect("search_Pokemon(): Failed to expand first_type")
                            .to_string();
                        let color = get_type_color(&first_type);

                        let choose_btn = serenity::CreateButton::new("open_modal")
                            .label(format!("Choose {}!", name).as_str())
                            .custom_id(format!("choose-{}", name))
                            .style(poise::serenity_prelude::ButtonStyle::Success);
                        buttons.push(choose_btn);

                        let components = vec![serenity::CreateActionRow::Buttons(buttons)];

                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!("Choose {}?", name))
                                            .description(desc)
                                            .image(img)
                                            .thumbnail(sprite)
                                            .colour(color)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(components),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        not_choose = false;
                        break;
                    }

                    // charmander
                    if react_id == user_id && reaction.data.custom_id.as_str() == "right" {
                        let name = charmander.get_name();
                        let desc = charmander.get_desc();
                        let sprite = charmander.get_sprite();
                        let img = charmander.get_wallpaper();

                        let types = charmander.get_types();
                        let type_split: Vec<&str> = types.split('/').collect();
                        let first_type = type_split
                            .first()
                            .expect("search_Pokemon(): Failed to expand first_type")
                            .to_string();
                        let color = get_type_color(&first_type);

                        let choose_btn = serenity::CreateButton::new("open_modal")
                            .label(format!("Choose {}!", name).as_str())
                            .custom_id(format!("choose-{}", name))
                            .style(poise::serenity_prelude::ButtonStyle::Success);
                        buttons.push(choose_btn);

                        let components = vec![serenity::CreateActionRow::Buttons(buttons)];

                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!("Choose {}?", name))
                                            .description(desc)
                                            .image(img)
                                            .thumbnail(sprite)
                                            .colour(color)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(components),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        not_choose = false;
                        break;
                    }
                }

                if timeout_check {
                    timeout_exit(&msg, &ctx, &user_avatar).await;
                    return;
                }

                let mut timeout_check = true;

                while let Some(reaction) = reactions.next().await {
                    reaction
                        .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                        .await
                        .unwrap();
                    let react_id = reaction.member.clone().unwrap_or_default().user.id;
                    let react_str = reaction.data.custom_id.to_string();
                    let string_split: Vec<&str> = react_str.split('-').collect();

                    if react_id == user_id && string_split[0] == "choose" {
                        match string_split[1] {
                            "Bulbasaur" => {
                                let mut user_data = u.write().await;
                                user_data.event.add_pokemon(bulbasaur.clone());
                                user_data.event.set_buddy(0);
                                not_choose = false;
                            }

                            "Squirtle" => {
                                let mut user_data = u.write().await;
                                user_data.event.add_pokemon(squirtle.clone());
                                user_data.event.set_buddy(0);
                                not_choose = false;
                            }

                            "Charmander" => {
                                let mut user_data = u.write().await;
                                user_data.event.add_pokemon(charmander.clone());
                                user_data.event.set_buddy(0);
                                not_choose = false;
                            }

                            _ => {
                                not_choose = true;
                            }
                        }

                        if !not_choose {
                            msg.write()
                                .await
                                .edit(
                                    &ctx,
                                    EditMessage::default()
                                        .embed(
                                            serenity::CreateEmbed::default()
                                                .title("Prof Oak".to_string())
                                                .description(&oak_text6)
                                                .image(oak_img)
                                                .thumbnail(&user_avatar)
                                                .colour(data::EMBED_CYAN)
                                                .footer(serenity::CreateEmbedFooter::new(
                                                    "@~ powered by UwUntu & RustyBamboo",
                                                )),
                                        )
                                        .components(Vec::new()),
                                )
                                .await
                                .unwrap();
                        }

                        timeout_check = false;
                        break;
                    }

                    if react_id == user_id && reaction.data.custom_id.as_str() == "back" {
                        let mut buttons = Vec::new();

                        let left_btn = serenity::CreateButton::new("open_modal")
                            .label("Left Poké Ball")
                            .custom_id("left".to_string())
                            .style(poise::serenity_prelude::ButtonStyle::Primary);
                        buttons.push(left_btn);

                        let center_btn = serenity::CreateButton::new("open_modal")
                            .label("Center Poké Ball")
                            .custom_id("center".to_string())
                            .style(poise::serenity_prelude::ButtonStyle::Primary);
                        buttons.push(center_btn);

                        let right_btn = serenity::CreateButton::new("open_modal")
                            .label("Right Poké Ball")
                            .custom_id("right".to_string())
                            .style(poise::serenity_prelude::ButtonStyle::Primary);
                        buttons.push(right_btn);

                        let components = vec![serenity::CreateActionRow::Buttons(buttons)];
                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title("Prof Oak".to_string())
                                            .description(&oak_text5)
                                            .image(poke_img)
                                            .thumbnail(&user_avatar)
                                            .colour(data::EMBED_DEFAULT)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(components),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        not_choose = true;
                        break;
                    }
                }

                if timeout_check {
                    timeout_exit(&msg, &ctx, &user_avatar).await;
                    return;
                }
            }
        });
    } else {
        let oaktextf = format!("Oh, <@{}>! How is my old Pokémon?\n\nWell, it seems to like you a lot. You must be talented as a Pokémon trainer!", user.id);
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Prof Oak".to_string())
                    .description(oaktextf)
                    .color(data::EMBED_ERROR)
                    .image(oak_img)
                    .thumbnail(user.avatar_url().unwrap_or_default().to_string())
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}

#[poise::command(slash_command)]
pub async fn buddy(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let team = user_data.event.get_team();
    let buddy = user_data.event.get_buddy();

    if !team.is_empty() {
        let pokemon = &team[buddy];

        let name: String = pokemon.get_name();
        let types: String = pokemon.get_types();
        let sprite: String = pokemon.get_sprite();

        let type_split: Vec<&str> = types.split('/').collect();
        let first_type = type_split
            .first()
            .expect("search_Pokemon(): Failed to expand first_type")
            .to_string();
        let poke_color = get_type_color(&first_type);

        let health = pokemon.get_health();
        let current = pokemon.get_current_health();
        let hp_percent = current as f32 / health as f32;

        let desc = if hp_percent > 0.80 {
            format!(
                "**{}** is brimming with energy! (HP: {}/{})",
                name, current, health
            )
        } else if (0.50..0.80).contains(&hp_percent) {
            format!("**{}** is happy. (HP: {}/{})", name, current, health)
        } else if (0.30..0.50).contains(&hp_percent) {
            format!("**{}** is tired... (HP: {}/{})", name, current, health)
        } else {
            format!(
                "**{}** is knocked out... (HP: {}/{})",
                name, current, health
            )
        };

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("{}'s Buddy", ctx.author().name))
                    .description(desc)
                    .color(Color::new(poke_color))
                    .thumbnail(sprite)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Buddy")
                    .description("You don't have a buddy right now...")
                    .color(data::EMBED_ERROR)
                    .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}

#[poise::command(slash_command)]
pub async fn test_bag(ctx: Context<'_>) -> Result<(), Error> {
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

#[poise::command(slash_command)]
pub async fn switch_buddy(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author().clone();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let user_id = user.id;
    let user_name = user.name;
    let user_avatar = ctx.author().avatar_url().unwrap_or_default().to_string();

    let buddy = user_data.event.get_buddy();
    let team = user_data.event.get_team();

    if !team.is_empty() {
        let mut desc = "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n".to_string();
        let mut buttons = Vec::new();

        let cancel_btn = serenity::CreateButton::new("open_modal")
            .label("cancel")
            .custom_id("cancel".to_string())
            .style(poise::serenity_prelude::ButtonStyle::Secondary);
        buttons.push(cancel_btn);

        for (index, pokemon) in team.iter().enumerate() {
            let name = pokemon.get_name();
            let types = pokemon.get_types();
            let current_hp = pokemon.get_current_health();
            let health = pokemon.get_health();

            if index == buddy {
                if name.len() < 7 {
                    desc += format!("- {}. **{:22}**  \u{2000}\u{2000}", index + 1, name).as_str();
                } else {
                    desc += format!("- {}. **{:20}**  ", index + 1, name).as_str();
                }

                desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();
            } else {
                if name.len() < 7 {
                    desc += format!("- {}. {:22}  \u{2000}", index + 1, name).as_str();
                } else {
                    desc += format!("- {}. {:20}  ", index + 1, name).as_str();
                }

                desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();

                let pk_btn = serenity::CreateButton::new("open_modal")
                    .label(&name)
                    .custom_id(format!("pokemon-{}", index))
                    .style(poise::serenity_prelude::ButtonStyle::Primary);
                buttons.push(pk_btn);
            }
        }

        let components = vec![serenity::CreateActionRow::Buttons(buttons)];

        desc += "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";

        let reply = ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Switch Buddy")
                    .description(&desc)
                    .color(data::EMBED_DEFAULT)
                    .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ).components(components)
        )
        .await?;

        let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
        let msg = Arc::clone(&msg_og);
        let mut reactions = msg
            .read()
            .await
            .await_component_interactions(ctx)
            .timeout(Duration::new(120, 0))
            .stream();

        let ctx = ctx.serenity_context().clone();
        let u = Arc::clone(&u);

        tokio::spawn(async move {
            let mut timeout_check = true;

            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();

                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "cancel" {
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Switch Buddy")
                                        .description(&desc)
                                        .color(data::EMBED_DEFAULT)
                                        .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();
                }

                let mut new_buddy = buddy;
                if react_id == user_id {
                    new_buddy = match reaction.data.custom_id.as_str() {
                        "pokemon-0" => 0,
                        "pokemon-1" => 1,
                        "pokemon-2" => 2,
                        "pokemon-3" => 3,
                        "pokemon-4" => 4,
                        _ => buddy,
                    };

                    let mut user_data = u.write().await;
                    user_data.event.set_buddy(new_buddy);
                    timeout_check = false;
                }

                if new_buddy != buddy {
                    let pokemon = &team[new_buddy];

                    let name: String = pokemon.get_name();
                    let types: String = pokemon.get_types();
                    let sprite: String = pokemon.get_sprite();

                    let type_split: Vec<&str> = types.split('/').collect();
                    let first_type = type_split
                        .first()
                        .expect("search_Pokemon(): Failed to expand first_type")
                        .to_string();
                    let poke_color = get_type_color(&first_type);

                    let health = pokemon.get_health();
                    let current = pokemon.get_current_health();
                    let hp_percent = current as f32 / health as f32;

                    let desc = if hp_percent > 0.80 {
                        format!(
                            "**{}** is brimming with energy! (HP: {}/{})",
                            name, current, health
                        )
                    } else if (0.50..0.80).contains(&hp_percent) {
                        format!("**{}** is happy. (HP: {}/{})", name, current, health)
                    } else if (0.30..0.50).contains(&hp_percent) {
                        format!("**{}** is tired... (HP: {}/{})", name, current, health)
                    } else {
                        format!(
                            "**{}** is knocked out... (HP: {}/{})",
                            name, current, health
                        )
                    };

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title(format!("{}'s Buddy", user_name))
                                        .description(desc)
                                        .color(Color::new(poke_color))
                                        .thumbnail(sprite)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();
                }
            }

            if timeout_check {
                let desc = "[Response timed out... try again tomorrow (next: `/uwu`)]".to_string();
                msg.write()
                    .await
                    .edit(
                        ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title("????????".to_string())
                                    .description(&desc)
                                    .thumbnail(user_avatar)
                                    .colour(data::EMBED_ERROR)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();
            }
        });
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Switch Buddy")
                    .description("You don't have anyone in your team...")
                    .color(data::EMBED_ERROR)
                    .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}

#[poise::command(slash_command)]
pub async fn team(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let team = user_data.event.get_team();
    let buddy = user_data.event.get_buddy();
    let mut desc = String::new();

    let embed_color: Color = if team.is_empty() {
        desc += "You don't have anyone in your team...";
        data::EMBED_ERROR
    } else {
        desc += "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";

        for (index, pokemon) in team.into_iter().enumerate() {
            let name = pokemon.get_name();
            let types = pokemon.get_types();
            let current = pokemon.get_current_health();
            let health = pokemon.get_health();

            if index == buddy {
                if name.len() < 7 {
                    desc += format!("- {}. **{:22}**  \u{2000}\u{2000}", index + 1, name).as_str();
                } else {
                    desc += format!("- {}. **{:20}**  ", index + 1, name).as_str();
                }

                desc += format!("{:20}  HP: {}/{}\n", types, current, health).as_str();
            } else {
                if name.len() < 7 {
                    desc += format!("- {}. {:22}  \u{2000}", index + 1, name).as_str();
                } else {
                    desc += format!("- {}. {:20}  ", index + 1, name).as_str();
                }

                desc += format!("{:20}  HP: {}/{}\n", types, current, health).as_str();
            }
        }

        desc += "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";

        data::EMBED_CYAN
    };

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("{}'s Team", ctx.author().name))
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

#[poise::command(slash_command)]
pub async fn pre_populate(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let mut user_data = u.write().await;

    let team_size = user_data.event.get_team().len();
    let generate = 5 - team_size;

    for _i in 0..generate {
        let mut pokemon = spawn_pokemon(ctx, 1);
        let health = generate_hp(pokemon.get_index());
        pokemon.set_health(health);

        user_data.event.add_pokemon(pokemon);
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("TESTING - Pre-Populate Team")
                .description("Team full!")
                .color(data::EMBED_ERROR)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}
