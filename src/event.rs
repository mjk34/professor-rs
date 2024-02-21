//!---------------------------------------------------------------------!
//! This file contains a collection of EVENT related commands, the      !
//! current event is POKEMON themed!                                    !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - get_pokedata                                              !
//!     [x] - search_pokemon                                            !
//!     [x] - test_matchup                                              !
//!     [-] - pokedex                                                   !
//!     [ ] - wild_encounter                                            !
//!     [ ] - trainer_battle                                            !
//!---------------------------------------------------------------------!

use crate::data::{self, PokeData, TrainerData};
use crate::serenity;
use crate::{Context, Error};
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::{EditMessage, UserId};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use tokio::sync::RwLock;
use tokio::time::sleep;

const COMMON: [usize; 61] = [
    1, 4, 7, 10, 11, 13, 14, 16, 19, 20, 21, 23, 27, 29, 32, 35, 39, 41, 43, 46, 47, 48, 50, 51,
    52, 53, 54, 56, 58, 60, 63, 66, 69, 72, 74, 77, 79, 81, 83, 84, 86, 88, 90, 92, 96, 98, 100,
    102, 104, 108, 109, 114, 116, 118, 120, 122, 124, 128, 129, 132, 147,
];
const RARE: [usize; 52] = [
    2, 5, 8, 12, 15, 17, 22, 24, 25, 28, 30, 33, 36, 37, 40, 42, 44, 49, 55, 57, 61, 64, 67, 70,
    73, 75, 78, 80, 82, 85, 87, 89, 91, 93, 95, 97, 99, 101, 103, 105, 110, 111, 117, 119, 121,
    123, 127, 133, 137, 138, 140, 148,
];
const MYTHIC: [usize; 33] = [
    3, 6, 9, 18, 26, 31, 34, 38, 45, 59, 62, 65, 68, 71, 76, 94, 106, 107, 112, 113, 115, 125, 126,
    130, 131, 134, 135, 136, 139, 141, 142, 143, 149,
];
const LEGENDARY: [usize; 5] = [144, 145, 146, 150, 151];

fn get_pokedata(
    ctx: Context<'_>,
    pokemon_name: Option<String>,
    pokemon_index: Option<usize>,
) -> PokeData {
    match (pokemon_name, pokemon_index) {
        // handle for user giving a name
        (Some(pokemon_name), None) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if let Some(pkmn) = pokedex
                .iter()
                .find(|&x| x.get_name().to_lowercase() == pokemon_name.to_lowercase())
            {
                pkmn
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving an index
        (None, Some(pokemon_index)) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if pokemon_index < 152 {
                pokedex
                    .get(pokemon_index)
                    .expect("get_pokedata(): Failed to load Pokemon from index")
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving both parameters
        (Some(pokemon_name), Some(pokemon_index)) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if let Some(pkmn) = pokedex
                .iter()
                .find(|&x| x.get_name().to_lowercase() == pokemon_name.to_lowercase())
            {
                pkmn
            } else if pokemon_index < 151 {
                pokedex
                    .get(pokemon_index)
                    .expect("get_pokedata(): Failed to load Pokemon from index")
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving no parameters
        (None, None) => {
            let pokedex = &ctx.data().pokedex;
            pokedex
                .first()
                .expect("get_pokedata(): Failed to load MissingNo.")
                .clone()
        }
    }
}

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
    let oak_img = "https://cdn.discordapp.com/attachments/1196582162057662484/1206355418889064539/c8bfe05ab93e2bcb0bc78301c1a3933a.jpg?ex=65dbb508&is=65c94008&hm=985b07f671001518f422fe541b8c91ee9ac106a273a8bb023a6fe1fdb617dd50&";

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

        let oak_img = "https://cdn.discordapp.com/attachments/1196582162057662484/1206355418889064539/c8bfe05ab93e2bcb0bc78301c1a3933a.jpg?ex=65dbb508&is=65c94008&hm=985b07f671001518f422fe541b8c91ee9ac106a273a8bb023a6fe1fdb617dd50&";
        let poke_img = "https://cdn.discordapp.com/attachments/1196582162057662484/1206355419153563668/nodemaster-pokemon-emerald-starter-selection-screen.jpg?ex=65dbb508&is=65c94008&hm=8a8595e4c8853a015b9c2ccd9c4e6403192c3d655e0f5b7250dd7cab269f214e&";

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
                    .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
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
                    .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
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
                                        .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
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
                    .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
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
                .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn wild_encounter(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let level = user_data.get_level();
    let type_matrix = ctx.data().type_matrix.clone();
    let type_name = ctx.data().type_name.clone();

    // Wild pokemon object
    let mut wild_pokemon = spawn_pokemon(ctx, level);
    wild_pokemon.set_health(generate_hp(wild_pokemon.get_index()));

    // Wild pokemon information
    let wild_pokemon_types: String = wild_pokemon.get_types().clone();
    let wild_pokemon_color = get_type_color(&wild_pokemon_types);

    // Player pokemon team and first pokemon (buddy) object
    let mut current = user_data.event.get_buddy();
    let mut player_team = user_data.event.get_team().clone();
    let mut player_pokemon = player_team.get(current).unwrap().clone();

    // Player pokemon information
    let player_pokemon_types: String = player_pokemon.get_types().clone();
    let player_pokemon_color = get_type_color(&player_pokemon_types);

    let continue_btn = serenity::CreateButton::new("open_modal")
        .label("Continue")
        .custom_id("continue".to_string())
        .style(poise::serenity_prelude::ButtonStyle::Success);

    let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];

    let reply = ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wild Pokemon")
                .description(format!("A wild **{}** appeared!", &wild_pokemon.get_name()))
                .color(Color::new(wild_pokemon_color))
                .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206129126470193162/Untitled-1.png?ex=65dae248&is=65c86d48&hm=f5f74d83901446f6e548943cd227b723d6dd27c380dcad929ac804e63414fbd7&")
                .image(&wild_pokemon.get_wallpaper())
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

    let user_id = user.id;
    let user_avatar = user.avatar_url().unwrap_or_default().to_string();
    let u = Arc::clone(&u);

    tokio::spawn(async move {
        let mut timeout_check = true;
        let mut catch_success = false;
        let mut defeat_pokemon = false;
        let mut wild_go_first = false;
        let mut forced_switch = false;

        async fn timeout_exit(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            user_avatar: &String,
        ) {
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

        async fn wild_pokemeon_turn(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            u: &Arc<RwLock<data::UserData>>,
            wild_pokemon: &mut PokeData,
            player_pokemon: &mut PokeData,
            current: &mut usize,
            wild_multiplier: f32,
        ) -> bool {
            let wild_attack_roll = if COMMON.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(2..8)
            } else if RARE.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(3..10)
            } else if MYTHIC.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(4..12)
            } else {
                thread_rng().gen_range(5..14)
            };

            let wild_pokemon_color = get_type_color(&wild_pokemon.get_types());
            let random_idle = thread_rng().gen_range(0..100);
            let mut forced_switch = false;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}**'s turn... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(1400)).await;

            if random_idle < 12 {
                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += format!(
                    " \u{3000} \u{3000}Wild **{}** dazed into the horizon... \n",
                    &wild_pokemon.get_name()
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(wild_pokemon_color)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                return forced_switch;
            }

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(700)).await;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                &wild_pokemon.get_name(),
                wild_attack_roll
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(700)).await;

            let wild_damage = (wild_attack_roll as f32 * wild_multiplier).round() as i32;
            if player_pokemon.get_current_health() - wild_damage < 0 {
                let _ = &player_pokemon.set_current_health(0);

                forced_switch = true;

                let mut user_data = u.write().await;
                user_data.event.take_damage(*current, 0);
            } else {
                _ = &player_pokemon
                    .set_current_health(player_pokemon.get_current_health() - wild_damage);

                let mut user_data = u.write().await;
                user_data.event.take_damage(*current, wild_damage);
            }

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n\n",
                wild_pokemon.get_name(),
                wild_attack_roll
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** attacks for **{}** damage!\n\n",
                wild_pokemon.get_name(),
                wild_damage
            )
            .as_str();

            if wild_multiplier >= 2.0 {
                desc += " \u{3000} \u{3000} **Super Effective**!!";
            } else if (1.0..2.0).contains(&wild_multiplier) {
                desc += " \u{3000} \u{3000} Effective.";
            } else {
                desc += " \u{3000} \u{3000} *Not very Effective*...";
            }

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            forced_switch
        }

        async fn show_team(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            player_team: &[PokeData],
            current: &usize,
            forced: bool,
        ) {
            let mut desc = "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n".to_string();
            let mut buttons = Vec::new();

            if !forced {
                let back_btn = serenity::CreateButton::new("open_modal")
                    .label("back")
                    .custom_id("back".to_string())
                    .style(poise::serenity_prelude::ButtonStyle::Secondary);
                buttons.push(back_btn);
            }

            for (index, pokemon) in player_team.iter().enumerate() {
                let name = pokemon.get_name();
                let types = pokemon.get_types();
                let current_hp = pokemon.get_current_health();
                let health = pokemon.get_health();

                if index == *current && forced {
                    if name.len() < 7 {
                        desc +=
                            format!("- {}. ~~{:22}~~  \u{2000}\u{2000}", index + 1, name).as_str();
                    } else {
                        desc += format!("- {}. ~~{:20}~~  ", index + 1, name).as_str();
                    }

                    desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();
                } else if index == *current && !forced {
                    if name.len() < 7 {
                        desc +=
                            format!("- {}. **{:22}**  \u{2000}\u{2000}", index + 1, name).as_str();
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

            let title_text = if forced {
                "Switch Pokemon"
            } else {
                "Switch Pokemon?"
            };

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default()
                        .embed(
                            serenity::CreateEmbed::default()
                                .title(title_text)
                                .description(desc)
                                .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206389369880059954/pokeballs.png?ex=65dbd4a7&is=65c95fa7&hm=06799355aeafcb5d59614e9d810975adec64d6538410b869ac036007d10ac46a&")
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        )
                        .components(components),
                )
                .await
                .unwrap();
        }

        while let Some(reaction) = reactions.next().await {
            reaction
                .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();

            let react_id = reaction.member.clone().unwrap_or_default().user.id;
            if react_id == user_id && reaction.data.custom_id.as_str() == "continue" {
                let wild_roll = thread_rng().gen_range(1..21);
                let player_roll = thread_rng().gen_range(1..21);

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(data::EMBED_DEFAULT)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                    &player_pokemon.get_name(),
                    player_roll
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    &wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                    &player_pokemon.get_name(),
                    player_roll
                )
                .as_str();

                let tmp_color: u32;
                if player_roll > wild_roll || player_roll == 1 && wild_roll == 1 {
                    desc += format!(
                        " \u{3000} \u{3000} \u{3000}**{}** goes first!\n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    tmp_color = player_pokemon_color;
                } else {
                    desc += format!(
                        " \u{3000} \u{3000} \u{3000}Wild **{}** goes first!\n",
                        &wild_pokemon.get_name()
                    )
                    .as_str();

                    wild_go_first = true;
                    tmp_color = wild_pokemon_color;
                }

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(tmp_color)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
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
        } else {
            if wild_go_first {
                sleep(Duration::from_millis(1000)).await;

                // potentiall return if forced switch
                let wild_multiplier = get_advantage(
                    &type_matrix,
                    &type_name,
                    &wild_pokemon_types,
                    &player_pokemon_types,
                );
                forced_switch = wild_pokemeon_turn(
                    &msg,
                    &ctx,
                    &u,
                    &mut wild_pokemon,
                    &mut player_pokemon,
                    &mut current,
                    wild_multiplier,
                )
                .await;

                // create force switch here to make playr switch, if user has no team left, battle finishes,
                // player looses creds and buddy goes back to 1 hp
                if forced_switch {
                    show_team(&msg, &ctx, &player_team, &current, forced_switch).await;
                }
            }

            sleep(Duration::from_millis(1700)).await;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += " \u{3000} \u{3000}Your turn... \n";
            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            let components = generate_btns();
            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default()
                        .embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        )
                        .components(components),
                )
                .await
                .unwrap();
        }

        // Create a check for who goes first ---- roll for initiative

        while !catch_success && !defeat_pokemon {
            let mut player_switch = false;
            let mut switch_cancel = false;
            // let mut  player_bag = false;

            forced_switch = false;
            timeout_check = true;

            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();

                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "fight" {
                    let attack_roll = if COMMON.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(2..8)
                    } else if RARE.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(3..10)
                    } else if MYTHIC.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(4..12)
                    } else {
                        thread_rng().gen_range(5..14)
                    };

                    let multiplier = get_advantage(
                        &type_matrix,
                        &type_name,
                        &player_pokemon_types,
                        &wild_pokemon_types,
                    );

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title(format!(
                                            "Wild **{}** |  HP: {}/{}",
                                            &wild_pokemon.get_name(),
                                            &wild_pokemon.get_current_health(),
                                            &wild_pokemon.get_health()
                                        ))
                                        .description(desc)
                                        .thumbnail(&wild_pokemon.get_sprite())
                                        .image(&player_pokemon.get_bsprite())
                                        .colour(player_pokemon_color)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();

                    sleep(Duration::from_millis(700)).await;

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n",
                        &player_pokemon.get_name(),
                        attack_roll
                    )
                    .as_str();

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default().embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(player_pokemon_color)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            ),
                        )
                        .await
                        .unwrap();

                    sleep(Duration::from_millis(700)).await;

                    let damage = (attack_roll as f32 * multiplier).round() as i32;
                    if wild_pokemon.get_current_health() - damage < 0 {
                        wild_pokemon.set_current_health(0);
                    } else {
                        wild_pokemon.set_current_health(wild_pokemon.get_current_health() - damage);
                    }

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                        &player_pokemon.get_name(),
                        attack_roll
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** attacks for **{}** damage!\n\n",
                        &player_pokemon.get_name(),
                        damage
                    )
                    .as_str();

                    if multiplier >= 2.0 {
                        desc += " \u{3000} \u{3000} **Super Effective**!!";
                    } else if (1.0..2.0).contains(&multiplier) {
                        desc += " \u{3000} \u{3000} Effective.";
                    } else {
                        desc += " \u{3000} \u{3000} *Not very Effective*...";
                    }

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default().embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(player_pokemon_color)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            ),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }

                // provide list of pokemon and buttons to switch pokemon
                if react_id == user_id && reaction.data.custom_id.as_str() == "switch" {
                    player_switch = true;
                    break;
                }

                // create a bag menu that shows potions, pokeballs and berries
                if react_id == user_id && reaction.data.custom_id.as_str() == "bag" {
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Bag is not yet implemented! (EXITING)".to_string())
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_ERROR)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();

                    return;
                    // timeout_check = false;
                    // break;
                }
            }

            if player_switch {
                show_team(&msg, &ctx, &player_team, &current, forced_switch).await;

                while let Some(reaction) = reactions.next().await {
                    reaction
                        .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                        .await
                        .unwrap();

                    let react_id = reaction.member.clone().unwrap_or_default().user.id;
                    if react_id == user_id && reaction.data.custom_id.as_str() == "back" {
                        let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                        desc += " \u{3000} \u{3000}Your turn... \n";
                        desc += format!(
                            "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                            &player_pokemon.get_name(),
                            &player_pokemon.get_current_health(),
                            &player_pokemon.get_health()
                        )
                        .as_str();

                        let components = generate_btns();
                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!(
                                                "Wild **{}** |  HP: {}/{}",
                                                &wild_pokemon.get_name(),
                                                &wild_pokemon.get_current_health(),
                                                &wild_pokemon.get_health()
                                            ))
                                            .description(desc)
                                            .thumbnail(&wild_pokemon.get_sprite())
                                            .image(&player_pokemon.get_bsprite())
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
                        switch_cancel = true;
                        break;
                    }
                }

                if timeout_check {
                    timeout_exit(&msg, &ctx, &user_avatar).await;
                    return;
                }
            }

            // if player_bag {}

            if !switch_cancel && !timeout_check && wild_pokemon.get_current_health() == 0 {
                defeat_pokemon = true;
            }

            if !switch_cancel && !timeout_check && !defeat_pokemon && !catch_success {
                sleep(Duration::from_millis(1400)).await;

                // potentiall return if forced switch
                let wild_multiplier = get_advantage(
                    &type_matrix,
                    &type_name,
                    &wild_pokemon_types,
                    &player_pokemon_types,
                );
                wild_pokemeon_turn(
                    &msg,
                    &ctx,
                    &u,
                    &mut wild_pokemon,
                    &mut player_pokemon,
                    &mut current,
                    wild_multiplier,
                )
                .await;

                // create force switch here to make playr switch, if user has no team left, battle finishes,
                // player looses creds and buddy goes back to 1 hp

                sleep(Duration::from_millis(1700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000}Your turn... \n";
                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                let components = generate_btns();
                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(data::EMBED_DEFAULT)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(components),
                    )
                    .await
                    .unwrap();
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }
        }

        msg.write()
            .await
            .edit(
                &ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::default()
                            .title("Battle Won!".to_string())
                            .thumbnail(&user_avatar)
                            .image("https://cdn.discordapp.com/attachments/1196582162057662484/1207548518131048468/gojo-jujutsu-kaisen.gif?ex=65e00c31&is=65cd9731&hm=8cff8d1defe87d2f529bb7e582a3d72e5a3ddb52daa64a07b3dc219b276e5f20&")
                            .colour(data::EMBED_CYAN)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    )
                    .components(Vec::new()),
            )
            .await
            .unwrap();
    });

    Ok(())
}

#[poise::command(slash_command)]
pub async fn trainer_battle(ctx: Context<'_>, level: u32) -> Result<(), Error> {
    let trainer = spawn_trainer(ctx, level as i32);

    let msg_txt: String = match trainer.get_tier().as_str() {
        "Mythic" => {
            format!(
                "Notorious Trainer **{}** wants to test your skills!",
                trainer.get_name()
            )
        }
        "Legendary" => {
            format!(
                "International Trainer **{}** invites you to battle!",
                trainer.get_name()
            )
        }
        _ => {
            format!(
                "**{}** is looking for a pokemon battle!",
                trainer.get_name()
            )
        }
    };
    let trainer_img = trainer.get_wallpaper();

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Trainer Battle")
                .description(msg_txt)
                .color(data::EMBED_TRAINER)
                .image(trainer_img)
                .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206057037247811594/674633.png?ex=65da9f25&is=65c82a25&hm=fca46c5743cea5a20ddbfe7d4d98d2087ac28ade2d6220b88452e2ad3633df88&")
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    for pokemon in trainer.get_team() {
        let name = pokemon.get_name();
        let types: String = pokemon.get_types();
        let type_split: Vec<&str> = types.split('/').collect();
        let first_type = type_split
            .first()
            .expect("search_Pokemon(): Failed to expand first_type")
            .to_string();
        let poke_color = get_type_color(&first_type);

        let desc = format!(
            "HP: {}/{}",
            pokemon.get_current_health(),
            pokemon.get_health()
        );

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("{}'s {}", trainer.get_name(), name))
                    .description(desc)
                    .thumbnail(pokemon.get_sprite())
                    .color(Color::new(poke_color))
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

fn spawn_pokemon(ctx: Context<'_>, level: i32) -> PokeData {
    let random_spawn = thread_rng().gen_range(0..100);

    let spawn_index: &usize;

    if level < 10 {
        spawn_index = COMMON.choose(&mut thread_rng()).unwrap();
    } else if (10..15).contains(&level) {
        spawn_index = if random_spawn < 95 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else {
            RARE.choose(&mut thread_rng()).unwrap()
        };
    } else if (15..20).contains(&level) {
        spawn_index = if random_spawn < 90 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else if (90..99).contains(&random_spawn) {
            RARE.choose(&mut thread_rng()).unwrap()
        } else {
            MYTHIC.choose(&mut thread_rng()).unwrap()
        };
    } else if (20..25).contains(&level) {
        spawn_index = if random_spawn < 82 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else if (82..97).contains(&random_spawn) {
            RARE.choose(&mut thread_rng()).unwrap()
        } else {
            MYTHIC.choose(&mut thread_rng()).unwrap()
        };
    } else if (25..30).contains(&level) {
        spawn_index = if random_spawn < 70 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else if (70..92).contains(&random_spawn) {
            RARE.choose(&mut thread_rng()).unwrap()
        } else if (92..99).contains(&random_spawn) {
            MYTHIC.choose(&mut thread_rng()).unwrap()
        } else {
            LEGENDARY.choose(&mut thread_rng()).unwrap()
        };
    } else if (30..35).contains(&level) {
        spawn_index = if random_spawn < 60 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else if (60..86).contains(&random_spawn) {
            RARE.choose(&mut thread_rng()).unwrap()
        } else if (86..97).contains(&random_spawn) {
            MYTHIC.choose(&mut thread_rng()).unwrap()
        } else {
            LEGENDARY.choose(&mut thread_rng()).unwrap()
        };
    } else {
        spawn_index = if random_spawn < 50 {
            COMMON.choose(&mut thread_rng()).unwrap()
        } else if (50..83).contains(&random_spawn) {
            RARE.choose(&mut thread_rng()).unwrap()
        } else if (83..95).contains(&random_spawn) {
            MYTHIC.choose(&mut thread_rng()).unwrap()
        } else {
            LEGENDARY.choose(&mut thread_rng()).unwrap()
        };
    }

    get_pokedata(ctx, None, Some(*spawn_index))
}

fn spawn_trainer(ctx: Context<'_>, level: i32) -> TrainerData {
    // Get Trainer tier based on user level
    let random_trainer = thread_rng().gen_range(0..100);
    let trainer_type = if level < 10 {
        "Common"
    } else if (10..15).contains(&level) {
        if random_trainer < 95 {
            "Common"
        } else {
            "Mythic"
        }
    } else if (15..20).contains(&level) {
        if random_trainer < 90 {
            "Common"
        } else {
            "Mythic"
        }
    } else if (20..25).contains(&level) {
        if random_trainer < 85 {
            "Common"
        } else {
            "Mythic"
        }
    } else if (25..30).contains(&level) {
        if random_trainer < 80 {
            "Common"
        } else if (80..99).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    } else if (30..35).contains(&level) {
        if random_trainer < 70 {
            "Common"
        } else if (70..95).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    } else {
        if random_trainer < 50 {
            "Common"
        } else if (50..92).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    };

    // Get random trainer in that tier
    let trainer_list = &ctx.data().trainers;
    let mut trainer: TrainerData = match trainer_type {
        "Mythic" => trainer_list[15..25]
            .choose(&mut thread_rng())
            .unwrap()
            .clone(),
        "Legendary" => trainer_list[25..]
            .choose(&mut thread_rng())
            .unwrap()
            .clone(),
        _ => {
            // Common
            trainer_list[..15]
                .choose(&mut thread_rng())
                .unwrap()
                .clone()
        }
    };

    let mut common_index: Vec<usize> = vec![];
    let mut rare_index: Vec<usize> = vec![];
    let mut mythic_index: Vec<usize> = vec![];
    let mut legendary_index: Vec<usize> = vec![];

    // Sample pokemon from each their based on specific trainer's types
    let trainer_types = trainer.get_types();
    let type_split: Vec<&str> = trainer_types.split('/').collect();
    for typing in type_split {
        for index in COMMON {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                common_index.push(index);
            }
        }
        for index in RARE {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                rare_index.push(index);
            }
        }
        for index in MYTHIC {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                mythic_index.push(index);
            }
        }
        for index in LEGENDARY {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                legendary_index.push(index);
            }
        }
    }

    // Get pokemon based on trainer tier from the sample indexes
    // Common -> 2 pokemon
    // Mythic -> 3 pokemon
    // Legendary -> 4 pokemon
    let team_count = match trainer.get_tier().as_str() {
        "Mythic" => 3,
        "Legendary" => 4,
        _ => 2,
    };

    // Get pokemon, set iconic trainers primary
    let mut team_index: Vec<usize> = vec![];
    for i in 0..team_count {
        // Special trainers
        match (i, trainer.get_name().as_str()) {
            (0, "Ashe") => {
                // pikachu
                team_index.push(25_usize);
                continue;
            }

            (0, "Brock") => {
                // onix
                team_index.push(95_usize);
                continue;
            }

            (0, "Misty") => {
                // starmie
                team_index.push(121_usize);
                continue;
            }

            (0, "Gary") => {
                //eevee
                team_index.push(133_usize);
                continue;
            }

            (0, "Blaine") => {
                // rapidash
                team_index.push(78_usize);
                continue;
            }

            (0, "Giovanni") => {
                // rhydon
                team_index.push(112_usize);
                continue;
            }

            (0, "Erika") => {
                // vileplume
                team_index.push(45_usize);
                continue;
            }

            (0, "Koga") => {
                // venomoth
                team_index.push(49_usize);
                continue;
            }

            (0, "Morty") => {
                // gengar
                team_index.push(95_usize);
                continue;
            }

            (1, "Morty") => {
                // moltres
                team_index.push(146_usize);
                continue;
            }

            (0, "Lance") => {
                //gyrados
                team_index.push(130_usize);
                continue;
            }

            (1, "Lance") => {
                //dragonite
                team_index.push(149_usize);
                continue;
            }

            (0, "Iono") => {
                // zapdos
                team_index.push(25_usize);
                continue;
            }

            (0, "N") => {
                // mew
                team_index.push(151_usize);
                continue;
            }

            (0, "Olivia") => {
                // kabutops
                team_index.push(141_usize);
                continue;
            }

            (0, "Marnie") => {
                // electabuzz
                team_index.push(125_usize);
                continue;
            }

            (0, "Cynthia") => {
                // mewtwo
                team_index.push(150_usize);
                continue;
            }

            (0, "Jesse & James") => {
                // meowth
                team_index.push(52_usize);
                continue;
            }

            _ => {}
        }

        let mut check = true;
        while check {
            let random_tier = thread_rng().gen_range(0..100);
            let poke_index: &usize = match trainer.get_tier().as_str() {
                "Mythic" => {
                    if random_tier < 30 {
                        common_index.choose(&mut thread_rng()).unwrap()
                    } else if (30..80).contains(&random_tier) {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        mythic_index.choose(&mut thread_rng()).unwrap()
                    }
                }

                "Legendary" => {
                    if random_tier < 20 {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    } else if (20..70).contains(&random_tier) {
                        mythic_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        legendary_index.choose(&mut thread_rng()).unwrap()
                    }
                }

                _ => {
                    if random_tier < 70 {
                        common_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    }
                }
            };

            // check if pokemon is already in team (no duplicates)
            if !team_index.contains(poke_index) && poke_index > &0 {
                team_index.push(*poke_index);
                check = false;
            }
        }
    }

    team_index.shuffle(&mut thread_rng());
    for index in team_index {
        let mut pokemon = get_pokedata(ctx, None, Some(index));
        match trainer.get_tier().as_str() {
            "Mythic" => pokemon.set_health(thread_rng().gen_range(23..38)),
            "Legendary" => pokemon.set_health(thread_rng().gen_range(31..46)),
            _ => pokemon.set_health(thread_rng().gen_range(15..30)),
        }

        trainer.give_pokemon(pokemon);
    }

    trainer
}

fn generate_btns() -> Vec<serenity::CreateActionRow> {
    // create buttons
    let mut buttons = Vec::new();

    let fight_btn = serenity::CreateButton::new("open_modal")
        .label("Fight")
        .custom_id("fight".to_string())
        .style(poise::serenity_prelude::ButtonStyle::Primary);
    buttons.push(fight_btn);

    let catch_btn = serenity::CreateButton::new("open_modal")
        .label("Switch")
        .custom_id("switch".to_string())
        .style(poise::serenity_prelude::ButtonStyle::Primary);
    buttons.push(catch_btn);

    let bag_btn = serenity::CreateButton::new("open_modal")
        .label("Bag")
        .custom_id("bag".to_string())
        .style(poise::serenity_prelude::ButtonStyle::Primary);
    buttons.push(bag_btn);

    vec![serenity::CreateActionRow::Buttons(buttons)]
}

fn generate_hp(index: usize) -> i32 {
    // Generate health based on tier
    if LEGENDARY.contains(&index) {
        thread_rng().gen_range(30..50)
    } else if MYTHIC.contains(&index) {
        thread_rng().gen_range(20..40)
    } else if RARE.contains(&index) {
        thread_rng().gen_range(15..30)
    } else {
        thread_rng().gen_range(10..20)
    }
}

fn get_advantage(matrix: &[Vec<f32>], names: &[String], type1: &str, type2: &str) -> f32 {
    let dual_type1: Vec<&str> = type1.split('/').collect();
    let dual_type2: Vec<&str> = type2.split('/').collect();

    let mut type_advantage: f32 = 1.0;
    for type1 in &dual_type1 {
        for type2 in &dual_type2 {
            let type1_index = names.iter().position(|x| x == type1).unwrap();
            let type2_index = names.iter().position(|x| x == type2).unwrap();

            let value = matrix.get(type1_index).unwrap().get(type2_index).unwrap();
            type_advantage *= value;
        }
    }

    type_advantage
}

fn get_type_color(types: &str) -> u32 {
    let type_split: Vec<&str> = types.split('/').collect();
    let first_type = type_split
        .first()
        .expect("search_Pokemon(): Failed to expand first_type")
        .to_string();

    match first_type.to_lowercase().as_str() {
        "normal" => 11053176,
        "fire" => 15761456,
        "water" => 6852848,
        "electric" => 16306224,
        "grass" => 7915600,
        "ice" => 10016984,
        "fighting" => 12595240,
        "poison" => 10502304,
        "ground" => 14729320,
        "flying" => 11047152,
        "psychic" => 16275592,
        "bug" => 11057184,
        "rock" => 12099640,
        "ghost" => 7362712,
        "dark" => 7362632,
        "steel" => 12105936,
        "fairy" => 15775420,
        "dragon" => 7354616,
        _ => 2039583,
    }
}

// fn get_type_emoji(typing: &str) -> String {
//     let dual_type: Vec<&str> = typing.split('/').collect();
//     let mut emojis: String = "".to_string();
//     for element in dual_type {
//         emojis += match element.to_lowercase().as_str() {
//             "normal" => ":white_circle:",
//             "fire" => ":fire:",
//             "water" => ":droplet:",
//             "electric" => ":zap:",
//             "grass" => ":leaves:",
//             "ice" => ":snowflake:",
//             "fighting" => ":punch:",
//             "poison" => ":skull:",
//             "ground" => ":mountain:",
//             "flying" => ":wing:",
//             "psychic" => ":fish_cake:",
//             "bug" => ":lady_beetle:",
//             "rock" => ":ring:",
//             "ghost" => ":ghost:",
//             "dark" => ":waxing_crescent_moon:",
//             "steel" => "nut_and_bolt:",
//             "fairy" => ":fairy:",
//             "dragon" => ":dragon:",
//             _ => "",
//         }
//     }

//     emojis
// }
