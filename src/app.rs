/// App is the main bot application and handler. It implements the outer game logic, keeping
/// track of the game state per user, scores, and persistence.
use anyhow::*;
use log::*;
use mobot::{api::User, *};
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

use crate::game;
use crate::game::Wordle;

pub enum Move {
    Valid,
    InvalidWord,
    InvalidLength,
    Won,
    Lost,
}

/// Score represents a user's score.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Score {
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

/// SaveData represents the data that is saved for each user on disk. Data
/// is saved in JSON format.
#[derive(Serialize, Deserialize)]
struct SaveData {
    user_id: String,
    #[serde(default)]
    user_handle: String,
    #[serde(default)]
    user_first_name: String,
    #[serde(default)]
    user_last_name: String,
    #[serde(default)]
    won_words: Vec<String>,
    #[serde(default)]
    played_words: Vec<String>,
    score: Score,
    last_wordle: Option<Wordle>,
}

/// App represents the bot state for the wordle bot.
#[derive(Clone, Default, BotState)]
pub struct App {
    // App global
    pub game_name: String,
    pub admin_user: Option<String>,
    admin_chat_id: Arc<RwLock<Option<i64>>>,
    save_dir: String,
    scores: Arc<RwLock<HashMap<String, Score>>>,
    target_words: Arc<Vec<String>>,
    valid_words: Arc<HashSet<String>>,

    // Per chat ID
    pub wordle: Option<Wordle>,
    played_words: HashSet<String>,
    won_words: HashSet<String>,
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

    pub fn is_playing(&self) -> bool {
        if self.wordle.is_none() {
            return false;
        }

        match self.wordle.as_ref().unwrap().game().unwrap().state {
            game::State::Playing => true,
            _ => false,
        }
    }

    pub async fn start_game(&mut self) -> Result<String> {
        // Get the sender's first name
        let target_word = self
            .target_words
            .iter()
            .find(|&w| !self.played_words.contains(&w.to_ascii_uppercase()))
            .or_else(|| self.target_words.choose(&mut rand::thread_rng()))
            .ok_or(anyhow!("no target words found"))?
            .clone()
            .to_uppercase();

        self.wordle = Some(Wordle::new(target_word.clone())?);
        self.played_words.insert(target_word.clone());
        Ok(target_word)
    }

    /// Authorizes the user as an admin.
    pub async fn auth_admin(&mut self, username: &str, chat_id: i64) -> bool {
        if self.admin_user.is_some() && self.admin_user.as_ref().unwrap().eq(username) {
            *self.admin_chat_id.write().await = Some(chat_id);
            return true;
        }
        false
    }

    /// Sends a log message to the admin chat
    pub async fn admin_log(&self, api: Arc<API>, text: String) {
        let chat_id = *self.admin_chat_id.read().await;
        if let Some(chat_id) = chat_id {
            _ = api
                .send_message(&api::SendMessageRequest {
                    chat_id,
                    text: format!("`{}`", api::escape_code(text.as_str())),
                    parse_mode: Some(api::ParseMode::MarkdownV2),
                    ..Default::default()
                })
                .await;
        }
    }

    /// Returns true if the word is a valid word.
    pub fn is_valid_word(&self, word: String) -> bool {
        self.valid_words.is_empty() || self.valid_words.contains(&word.to_ascii_lowercase())
    }

    /// Set the valid words for this game.
    pub fn set_valid_words(&mut self, valid_words: HashSet<String>) {
        self.valid_words = Arc::new(valid_words);
    }

    /// Set the directory where game state is saved.
    pub fn set_save_dir(&mut self, save_dir: String) {
        self.save_dir = save_dir;
    }

    /// Returns the user's current score
    pub async fn score(&self, from: &String) -> Score {
        self.scores
            .read()
            .await
            .get(from)
            .cloned()
            .unwrap_or_default()
    }

    /// Increments the number of games this user played and saves state.
    pub async fn inc_games(&self, from: &User) {
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
    pub async fn inc_wins(&mut self, from: &User) {
        self.scores
            .write()
            .await
            .entry(from.id.to_string())
            .or_default()
            .wins += 1;
        self.won_words
            .insert(self.wordle.as_ref().unwrap().target_word.clone());
        if let Err(e) = self.save(from).await {
            error!("Error saving game state: {}", e);
        }
    }

    pub async fn play_turn(&mut self, from: &User, word: String) -> anyhow::Result<Move> {
        if !self.is_valid_word(word.clone()) {
            return Ok(Move::InvalidWord);
        }

        if word.len() != self.wordle.as_ref().unwrap().target_word.len() {
            return Ok(Move::InvalidLength);
        }

        let game = self.wordle.as_mut().unwrap().play_turn(word)?;
        match game.state {
            game::State::Won => {
                self.inc_wins(&from).await;
                Ok(Move::Won)
            }
            game::State::Lost => Ok(Move::Lost),
            _ => Ok(Move::Valid),
        }
    }

    /// Save game state for user
    pub async fn save(&self, user: &User) -> anyhow::Result<()> {
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
            user_first_name: user.first_name.clone(),
            user_last_name: user.last_name.clone().unwrap_or_default(),
            played_words: self.played_words.iter().cloned().collect(),
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

    /// Load game state for user.
    pub async fn load(&mut self, user: &User) -> anyhow::Result<()> {
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

        self.won_words = HashSet::from_iter(save_data.won_words.clone());
        if self.played_words.len() < self.won_words.len() {
            self.played_words = HashSet::from_iter(save_data.won_words);
        } else {
            self.played_words = HashSet::from_iter(save_data.played_words);
        }
        self.scores
            .write()
            .await
            .insert(user.id.to_string(), save_data.score);
        self.wordle = save_data.last_wordle;

        Ok(())
    }
}
