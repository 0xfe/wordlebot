/// Wordle is a game where you have to guess a word. The word is chosen by the game, and you
/// have 6 attempts to guess it. After each attempt, the game tells you which letters you
/// guessed correctly, and which letters are in the word but in the wrong position.
///
/// This module implements the game logic.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// State represents the current player state of a game.
#[derive(Debug, Eq, PartialEq)]
pub enum State {
    Playing,
    Won,
    Lost,
}

/// Letter represents the position of a single letter in an attempted
/// word.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Letter {
    Correct(char),
    CorrectButWrongPosition(char),
    Wrong(char),
}

/// Game represents a single Wordle board that can be rendered and presented
/// to the player.
#[derive(Debug)]
pub struct Game {
    pub state: State,
    pub attempts: Vec<Vec<Letter>>,
}

impl Game {
    /// `attempted_letters` returns a sorted deduplicated vector of all the letters that
    /// have been attempted so far.
    pub fn attempted_letters(&self) -> Vec<char> {
        let mut letters = self
            .attempts
            .iter()
            .flat_map(|a| a.iter())
            .map(|l| match l {
                Letter::Correct(c) => *c,
                Letter::CorrectButWrongPosition(c) => *c,
                Letter::Wrong(c) => *c,
            })
            .collect::<Vec<_>>();
        letters.sort();
        letters.dedup();
        letters
    }
}

/// Wordle represents a single Worldle game.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Wordle {
    /// The target word that the player is trying to guess.
    pub target_word: String,

    /// The words that the player has attempted so far.
    pub attempts: Vec<String>,
}

impl Wordle {
    /// `new` creates a new Wordle game with the given target word.
    pub fn new(target_word: String) -> anyhow::Result<Wordle> {
        if target_word.len() < 3 {
            anyhow::bail!("target_word must be at least 3 letters long")
        }

        Ok(Wordle {
            target_word: target_word.to_uppercase(),
            attempts: Vec::new(),
        })
    }

    /// `game` returns a Game instance that can be rendered and presented to the player.
    pub fn game(&self) -> anyhow::Result<Game> {
        let state = if self.attempts.contains(&self.target_word) {
            State::Won
        } else if self.attempts.len() >= 6 {
            State::Lost
        } else {
            State::Playing
        };

        let attempts: Result<Vec<_>, _> = self.attempts.iter().map(|a| self.assess(a)).collect();

        Ok(Game {
            state,
            attempts: attempts?,
        })
    }

    // `assess` compares the given word to the target word, and returns a vector of positional
    // Letter instances. The vector is the same length as the target word, and each Letter
    // corresponds to the letter in the same position in the target word.
    //
    // Duplicates are handled as per the rules of Wordle.
    pub fn assess(&self, word: impl Into<String>) -> anyhow::Result<Vec<Letter>> {
        let word = word.into().to_uppercase();
        if word.len() != self.target_word.len() {
            anyhow::bail!("word must be {} characters long", self.target_word.len())
        }

        let mut letters = Vec::new();

        // Keep track of the number of times each letter appears in the target word.
        let target_letter_count = self.target_word.chars().fold(HashMap::new(), |mut acc, c| {
            *acc.entry(c).or_insert(0) += 1;
            acc
        });

        // Keep track of the number of times each letter appears in the played word.
        let mut dup_letter_count = HashMap::new();
        for (i, c) in word.chars().enumerate() {
            if self.target_word.contains(c) {
                if self.target_word.chars().nth(i) == Some(c) {
                    letters.push(Letter::Correct(c));
                    *dup_letter_count.entry(c).or_insert(0) += 1;
                } else {
                    letters.push(Letter::CorrectButWrongPosition(c));
                    *dup_letter_count.entry(c).or_insert(0) += 1;
                }
            } else {
                letters.push(Letter::Wrong(c));
            }
        }

        // Remove dups by replacing duplicated CorrectButWrongPosition letters with Wrong letters.
        // https://wordfinder.yourdictionary.com/blog/can-letters-repeat-in-wordle-a-closer-look-at-the-rules/
        letters = letters
            .iter()
            .map(|l| match l {
                Letter::Correct(c) => Letter::Correct(*c),
                Letter::CorrectButWrongPosition(c) => {
                    let letter_count = dup_letter_count.entry(*c).or_insert(0);
                    if *letter_count > *target_letter_count.get(c).unwrap_or(&0) {
                        *letter_count -= 1;
                        Letter::Wrong(*c)
                    } else {
                        Letter::CorrectButWrongPosition(*c)
                    }
                }
                Letter::Wrong(c) => Letter::Wrong(*c),
            })
            .collect();

        Ok(letters)
    }

    /// `play_turn` plays a turn of the game, and returns a Game instance that can be rendered
    /// and presented to the player.
    pub fn play_turn(&mut self, word: impl Into<String>) -> anyhow::Result<Game> {
        let word = word.into().to_uppercase();
        if word.len() != self.target_word.len() {
            anyhow::bail!("word must be {} characters long", self.target_word.len())
        }

        let game = self.game()?;
        if game.state != State::Playing {
            anyhow::bail!("game is over")
        }

        self.attempts.push(word.clone());
        self.game()
    }
}
