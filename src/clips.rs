use crate::{Context, Error};

// Submit game/other clips
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn submit_clip(_ctx: Context<'_>) -> Result<(), Error> {

    Ok(())
}

// View clip submission summary
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn submit_list(_ctx: Context<'_>) -> Result<(), Error> {

    Ok(())
}