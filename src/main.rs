use std::{collections::HashSet, sync::Arc};

use crate::game::Wordle;
use anyhow::anyhow;
use argh::FromArgs;
use log::*;
use mobot::*;
use rand::seq::SliceRandom;

mod game;

/// emoji_letter takes a capital letter and returns the corresponding emoji letter
/// inside the Regional Indicator Symbol range.
fn emoji_letter(l: char) -> char {
    let base = 0x1F1E6;
    let a = 'A' as u32;
    let target = l.to_ascii_uppercase() as u32;

    std::char::from_u32(base + target - a).unwrap_or('?')
}

/// render_game takes a game::Game and returns a string representation of it.
/// Emoji codepoints: https://emojipedia.org/emoji/
fn render_game(game: &game::Game) -> String {
    let mut s = String::new();
    for attempt in &game.attempts {
        for letter in attempt {
            match letter {
                game::Letter::Correct(c) => {
                    s.push_str(&format!("{} ", emoji_letter(*c).to_string()))
                }
                game::Letter::CorrectButWrongPosition(c) => s.push_str(&format!(" `{}`  ", c)),
                game::Letter::Wrong(_) => s.push_str("\u{2796} "),
            }
        }
        s.push_str("\n\n");
    }
    s
}

/// App represents the bot state for the wordle bot.
#[derive(Clone, Default, BotState)]
struct App {
    target_words: Arc<Vec<String>>,
    valid_words: Arc<HashSet<String>>,
    won_words: HashSet<String>,
    wordle: Option<Wordle>,
}

/// handle_chat_event is the main Telegram handler for the bot.
async fn handle_chat_event(e: Event, state: State<App>) -> Result<Action, anyhow::Error> {
    // Get the message
    let message = e.update.get_message()?.clone().text.unwrap().clone();

    // Get the sender's first name
    let from = e
        .update
        .get_message()?
        .clone()
        .from
        .unwrap_or_default()
        .first_name;

    // Get the application state
    let mut app = state.get().write().await;
    let target_word;

    // If there's no active game, start one.
    if app.wordle.is_none() {
        // Scan the list for an unplayed word, or pick a random one.
        target_word = app
            .target_words
            .iter()
            .find(|&w| !app.won_words.contains(w))
            .or_else(|| app.target_words.choose(&mut rand::thread_rng()))
            .ok_or(anyhow!("no target words found"))?
            .clone();

        info!(
            "Starting new game with {}, target word: {}.",
            from, target_word
        );
        app.wordle = Some(Wordle::new(target_word)?);
        return Ok(Action::ReplyText(format!(
            "Hi {}, Welcome to Wordle! Guess the 5-letter word.",
            from
        )));
    }

    // There's an active game, so play a turn. First check if the word is valid.
    if !app.valid_words.is_empty() && !app.valid_words.contains(&message) {
        return Ok(Action::ReplyText(format!(
            "Sorry {}, that's not a valid word. Try again.",
            from
        )));
    }

    let wordle = app.wordle.as_mut().unwrap();
    let game = wordle.play_turn(message)?;
    let mut reply = render_game(&game);
    target_word = wordle.target_word.clone();

    match game.state {
        game::State::Playing => reply.push_str("\nNext move?"),
        game::State::Won => {
            reply.push_str("\nYou won\\!");
            app.won_words.insert(target_word);
            app.wordle = None;
        }
        game::State::Lost => {
            reply.push_str("\nYou lost\\!");
            app.wordle = None;
        }
    }

    Ok(Action::ReplyMarkdown(reply))
}

#[derive(FromArgs)]
/// Reach new heights.
struct Args {
    /// file containing target words for the bot, one per line
    #[argh(option, short = 't', default = "String::from(\"target_words.txt\")")]
    target_words: String,

    /// file containing valid words for the bot, one per line
    #[argh(option, short = 'v', default = "String::from(\"valid_words.txt\")")]
    valid_words: String,
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

#[tokio::main]
async fn main() {
    mobot::init_logger();
    let args: Args = argh::from_env();

    let mut target_words = read_words(args.target_words);
    target_words.shuffle(&mut rand::thread_rng());

    let valid_words = HashSet::from_iter(read_words(args.valid_words).iter().cloned());

    let app = App {
        target_words: Arc::new(target_words),
        valid_words: Arc::new(valid_words),
        ..Default::default()
    };

    let client = Client::new(std::env::var("TELEGRAM_TOKEN").unwrap().into());
    info!("Starting bot...");
    Router::new(client)
        .with_state(app)
        .add_route(Route::Default, handle_chat_event)
        .start()
        .await;
}
