use anyhow::*;
use log::*;
use mobot::{
    api::{escape_md, User},
    *,
};
use rand::seq::SliceRandom;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
    sync::RwLock,
};

use serde::{Deserialize, Serialize};

use crate::game::{self, Wordle};

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
                game::Letter::CorrectButWrongPosition(c) => s.push_str(&format!(" *_{}_*  ", c)),
                game::Letter::Wrong(_) => s.push_str("\u{2796} "),
            }
        }
        s.push_str("\n\n");
    }
    s
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Score {
    pub games: u32,
    pub wins: u32,
}

impl Display for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.0}% ({}/{})",
            self.wins as f32 / self.games as f32 * 100.0,
            self.wins,
            self.games
        )
    }
}

#[derive(Serialize, Deserialize)]
struct GameHistory {
    start_time: String,
    end_time: String,
    target_word: String,
    attempts: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct SaveData {
    last_wordle: Option<Wordle>,
    user_id: String,
    user_handle: String,
    won_words: Vec<String>,
    score: Score,
}

/// App represents the bot state for the wordle bot.
#[derive(Clone, Default, BotState)]
pub struct App {
    // App global
    game_name: String,
    save_dir: String,
    scores: Arc<RwLock<HashMap<String, Score>>>,
    target_words: Arc<Vec<String>>,
    valid_words: Arc<HashSet<String>>,

    // Per chat ID
    won_words: HashSet<String>,
    wordle: Option<Wordle>,
}

impl App {
    /// Creates a new App instance.
    pub fn new(game_name: String, target_words: Vec<String>) -> App {
        App {
            game_name,
            target_words: Arc::new(target_words),
            ..Default::default()
        }
    }

    pub fn set_valid_words(&mut self, valid_words: HashSet<String>) {
        self.valid_words = Arc::new(valid_words);
    }

    pub fn set_save_dir(&mut self, save_dir: String) {
        self.save_dir = save_dir;
    }

    /// Returns the user's current score
    async fn score(&self, from: &String) -> Score {
        self.scores
            .read()
            .await
            .get(from)
            .cloned()
            .unwrap_or_default()
    }

    /// Increments the number of games this user played and saves state.
    async fn inc_games(&self, from: &User) {
        self.scores
            .write()
            .await
            .entry(from.id.to_string())
            .or_default()
            .games += 1;
        if let Err(e) = self.save(from).await {
            error!("Error saving game state: {}", e);
        }
    }

    /// Increments the number of wins for this user and saves state.
    async fn inc_wins(&self, from: &User) {
        self.scores
            .write()
            .await
            .entry(from.id.to_string())
            .or_default()
            .wins += 1;
        if let Err(e) = self.save(from).await {
            error!("Error saving game state: {}", e);
        }
    }

    /// Save game state for user
    async fn save(&self, user: &User) -> anyhow::Result<()> {
        if self.save_dir.is_empty() {
            return Ok(());
        }

        let filename = format!("{}/{}.json", self.save_dir, user.id);

        let mut file = File::create(filename.clone())
            .await
            .context(format!("Error creating file {}", filename))?;

        let last_wordle = self.wordle.clone();

        let save_data = SaveData {
            user_id: user.id.clone().to_string(),
            user_handle: user.username.clone().unwrap_or_default(),
            won_words: self.won_words.iter().cloned().collect(),
            score: self.score(&user.id.to_string()).await,
            last_wordle,
        };

        file.write_all(
            serde_json::to_vec(&save_data)
                .context("Error serializing game state")?
                .as_ref(),
        )
        .await
        .context(format!("Error writing file {}", filename))
    }

    async fn load(&mut self, user: &User) -> anyhow::Result<()> {
        if self.save_dir.is_empty() {
            bail!("No save directory configured");
        }

        let filename = format!("{}/{}.json", self.save_dir, user.id);

        let mut file = File::open(filename.clone())
            .await
            .context(format!("Error opening file {}", filename))?;

        let mut contents = vec![];
        file.read_to_end(&mut contents)
            .await
            .context(format!("Error reading file {}", filename))?;

        let save_data: SaveData = serde_json::from_slice(&contents)
            .context(format!("Error deserializing game state from {}", filename))?;

        self.won_words = HashSet::from_iter(save_data.won_words);
        self.scores
            .write()
            .await
            .insert(user.id.to_string(), save_data.score);
        self.wordle = save_data.last_wordle;

        Ok(())
    }
}

/// handle_chat_event is the main Telegram handler for the bot.
pub async fn handle_chat_event(e: Event, state: State<App>) -> Result<Action, anyhow::Error> {
    // Get the message
    let message = e.update.get_message()?.clone().text.unwrap().clone();

    // Get the sender's first name
    let from = e.update.get_message()?.clone().from.unwrap_or_default();

    // Get the application state
    let mut app = state.get().write().await;
    let target_word;
    if let Err(e) = app.load(&from).await {
        warn!("No saved game state: {}", e);
    }

    // If there's no active game, start one.
    if app.wordle.is_none() {
        // Load game state

        // Scan the list for an unplayed word, or pick a random one.
        target_word = app
            .target_words
            .iter()
            .find(|&w| !app.won_words.contains(w))
            .or_else(|| app.target_words.choose(&mut rand::thread_rng()))
            .ok_or(anyhow!("no target words found"))?
            .clone()
            .to_uppercase();

        info!(
            "Starting new game with {} ({}), target word: {}.",
            from.first_name,
            from.username.clone().unwrap_or("unknown".into()),
            target_word
        );
        app.wordle = Some(Wordle::new(target_word.clone())?);
        let first_game = if app.score(&from.id.to_string()).await.games == 0 {
            "This is your first game.".to_string()
        } else {
            format!("Your score: {}.", app.score(&from.id.to_string()).await)
        };

        app.inc_games(&from).await;
        return Ok(Action::ReplyText(format!(
            "Hi {}, Welcome to {}!\n\n{}\nGuess the {}-letter word.",
            from.first_name,
            app.game_name,
            first_game,
            target_word.len()
        )));
    }

    // There's an active game, so get the target word.
    target_word = app.wordle.as_ref().unwrap().target_word.clone();
    info!(
        "{} ({}) guessed {}",
        from.first_name,
        from.username.clone().unwrap_or("unknown".into()),
        message
    );

    // Check if the message is the right length.
    if message.len() != target_word.len() {
        return Ok(Action::ReplyText(format!(
            "Sorry {}, the word must be {} letters long. Try again.",
            from.first_name,
            target_word.len()
        )));
    }

    // Check if the message is a valid word.
    if !app.valid_words.is_empty() && !app.valid_words.contains(&message.to_ascii_lowercase()) {
        return Ok(Action::ReplyText(format!(
            "Sorry {}, that's not a valid word. Try again.",
            from.first_name
        )));
    }

    // Play the turn.
    let wordle = app.wordle.as_mut().unwrap();
    let game = wordle.play_turn(message.clone())?;
    let mut reply = render_game(&game);

    match game.state {
        game::State::Playing => reply.push_str("\nNice try\\. Guess another word\\?"),
        game::State::Won => {
            app.won_words.insert(target_word);
            app.wordle = None;
            app.inc_wins(&from).await;
            reply.push_str(
                escape_md(
                    format!(
                        "\nYou won! \u{1F46F}\nYour score: {}",
                        app.score(&from.id.to_string()).await
                    )
                    .as_str(),
                )
                .as_str(),
            );
            info!(
                "{} ({}) won with {}",
                from.first_name,
                from.clone().username.unwrap_or("unknown".into()),
                message
            );
        }
        game::State::Lost => {
            reply.push_str(
                escape_md(
                    format!(
                        "\nYou lost! \u{1F979}\nYour score: {}",
                        app.score(&from.id.to_string()).await
                    )
                    .as_str(),
                )
                .as_str(),
            );
            app.wordle = None;
            info!(
                "{} ({}) lost with {} (target: {})",
                from.first_name,
                from.clone().username.unwrap_or("unknown".into()),
                message,
                target_word
            );
        }
    }

    if let Err(e) = app.save(&from).await {
        error!("Error saving game state: {}", e);
    }

    Ok(Action::ReplyMarkdown(reply))
}
