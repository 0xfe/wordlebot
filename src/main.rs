use std::{collections::HashSet, path::Path};

use crate::app::*;

use anyhow::Context;
use argh::FromArgs;
use log::*;
use mobot::*;
use rand::seq::SliceRandom;

mod app;
mod game;

#[derive(FromArgs)]
/// Reach new heights.
struct Args {
    /// how the bot presents itself in the welcome message
    #[argh(
        option,
        short = 'n',
        default = "String::from(\"Bad Wordle \u{1F608}\")"
    )]
    game_name: String,

    /// file containing target words for the bot, one per line
    #[argh(option, short = 't', default = "String::from(\"target_words.txt\")")]
    target_words: String,

    /// file containing valid words for the bot, one per line
    #[argh(option, short = 'v', default = "String::from(\"valid_words.txt\")")]
    valid_words: String,

    /// directory to save user state. If empty, state is not saved.
    #[argh(option, short = 's')]
    save_dir: Option<String>,
}

// read_words reads a file containing one word per line, and returns a vector of
// strings. It filters out empty lines and lines that start with a '#'.
fn read_words(path: impl AsRef<str>) -> Vec<String> {
    std::fs::read_to_string(path.as_ref())
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .filter(|s| !s.starts_with("#"))
        .filter(|s| !s.trim().is_empty())
        .collect()
}

async fn start(args: Args) -> anyhow::Result<()> {
    // Read the list of target words.
    let mut target_words = read_words(args.target_words);
    if target_words.is_empty() {
        anyhow::bail!("No target words found.");
    }

    // Shuffle the target words.
    target_words.shuffle(&mut rand::thread_rng());

    // Read the list of valid words, and make sure the target words are in it.
    let mut valid_words = HashSet::from_iter(read_words(args.valid_words).iter().cloned());
    target_words.iter().for_each(|w| {
        valid_words.insert(w.to_ascii_lowercase());
    });

    if valid_words.is_empty() {
        error!("No valid words found. Not validating words.");
    }

    if !Path::new(&args.save_dir.clone().unwrap_or_default()).exists() {
        error!("Save directory does not exist. Not saving state.");
    }

    // Initialize the bot app state.
    let mut app = App::new(args.game_name, target_words);
    app.set_valid_words(valid_words);
    app.set_save_dir(args.save_dir.unwrap_or_default());

    // Initialize the Telegram client.
    let client = Client::new(
        std::env::var("TELEGRAM_TOKEN")
            .context("Could not fetch API key from TELEGRAM_TOKEN env variable.")?
            .into(),
    );

    info!("Starting bot...");
    Router::new(client)
        .with_state(app)
        .add_route(Route::Default, handle_chat_event)
        .start()
        .await;

    Ok(())
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    mobot::init_logger();
    let args: Args = argh::from_env();
    if let Err(e) = start(args).await {
        error!("{}", e);
        return std::process::ExitCode::FAILURE;
    }

    std::process::ExitCode::SUCCESS
}
