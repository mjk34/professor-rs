//!---------------------------------------------------------------------!
//! This file contains a collection of helper functions that get used   !
//! many times in larger pokemon battles.                               !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - get_pokedata                                              !
//!     [x] - spawn_pokemon                                             !
//!     [x] - generate_hp                                               !
//!     [x] - get_advantage                                             !
//!     [x] - get_type_color                                            !
//!---------------------------------------------------------------------!

use crate::data::{PokeData, TrainerData};
use crate::serenity;
use crate::Context;

use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};

pub const COMMON: [usize; 61] = [
    1, 4, 7, 10, 11, 13, 14, 16, 19, 20, 21, 23, 27, 29, 32, 35, 39, 41, 43, 46, 47, 48, 50, 51,
    52, 53, 54, 56, 58, 60, 63, 66, 69, 72, 74, 77, 79, 81, 83, 84, 86, 88, 90, 92, 96, 98, 100,
    102, 104, 108, 109, 114, 116, 118, 120, 122, 124, 128, 129, 132, 147,
];
pub const RARE: [usize; 52] = [
    2, 5, 8, 12, 15, 17, 22, 24, 25, 28, 30, 33, 36, 37, 40, 42, 44, 49, 55, 57, 61, 64, 67, 70,
    73, 75, 78, 80, 82, 85, 87, 89, 91, 93, 95, 97, 99, 101, 103, 105, 110, 111, 117, 119, 121,
    123, 127, 133, 137, 138, 140, 148,
];
pub const MYTHIC: [usize; 33] = [
    3, 6, 9, 18, 26, 31, 34, 38, 45, 59, 62, 65, 68, 71, 76, 94, 106, 107, 112, 113, 115, 125, 126,
    130, 131, 134, 135, 136, 139, 141, 142, 143, 149,
];
pub const LEGENDARY: [usize; 5] = [144, 145, 146, 150, 151];

pub fn get_pokedata(
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

pub fn spawn_pokemon(ctx: Context<'_>, level: i32) -> PokeData {
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

pub fn spawn_trainer(ctx: Context<'_>, level: i32) -> TrainerData {
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

pub fn generate_btns() -> Vec<serenity::CreateActionRow> {
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

pub fn generate_hp(index: usize) -> i32 {
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

pub fn get_advantage(matrix: &[Vec<f32>], names: &[String], type1: &str, type2: &str) -> f32 {
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

pub fn get_type_color(types: &str) -> u32 {
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
