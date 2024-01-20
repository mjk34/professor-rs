
use crate::serenity;
use crate::{Context, Error};
use serenity::Color;

#[poise::command(prefix_command, slash_command)]
pub async fn search_pokemon(ctx: Context<'_>, pokedex_no: usize) -> Result<(), Error> {

    if pokedex_no > 152 {
        let msg_txt = format!("Entry {} does not exist.", pokedex_no);

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Pokedex no.---")
                    .description(msg_txt)
                    .color(Color::new(16760399))
                    .thumbnail("https://archives.bulbagarden.net/media/upload/3/37/RG_Pok%C3%A9dex.png")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    } else {
        let pokemon = ctx
            .data()
            .pokedex
            .get(pokedex_no)
            .expect(format!("Could not find Pokemon no.{}", pokedex_no).as_str());
        let name: String = pokemon.get_name();
        let desc: String = pokemon.get_desc();
        let types: String = pokemon.get_types();
        let sprite: String = pokemon.get_sprite();

        let msg_txt = format!("**{}**: {}\n{}", name, types, desc);
        let type_split: Vec<&str> = types.split("/").collect();
        let first_type = type_split.get(0).expect("Failed to expand type").to_string();
        let poke_color = get_type_color(first_type);

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("Pokedex no.{}", pokedex_no))
                    .description(msg_txt)
                    .color(Color::new(poke_color))
                    .thumbnail(sprite)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}

fn get_type_color (typing: String) -> u32 {
    return match typing.to_lowercase().as_str() {
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
        _ => 2039583
    }

}