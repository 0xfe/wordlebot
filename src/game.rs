use serde::{Deserialize, Serialize};

#[derive(Debug, Eq, PartialEq)]
pub enum State {
    Playing,
    Won,
    Lost,
}

#[derive(Debug)]
pub enum Letter {
    Correct(char),
    CorrectButWrongPosition(char),
    Wrong(char),
}

#[derive(Debug)]
pub struct Game {
    pub state: State,
    pub attempts: Vec<Vec<Letter>>,
}

impl Game {
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
    pub target_word: String,
    pub attempts: Vec<String>,
}

impl Wordle {
    pub fn new(target_word: String) -> anyhow::Result<Wordle> {
        if target_word.len() < 3 {
            anyhow::bail!("target_word must be at least 3 letters long")
        }

        Ok(Wordle {
            target_word: target_word.to_uppercase(),
            attempts: Vec::new(),
        })
    }

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

    pub fn assess(&self, word: impl Into<String>) -> anyhow::Result<Vec<Letter>> {
        let word = word.into().to_uppercase();
        if word.len() != self.target_word.len() {
            anyhow::bail!("word must be {} characters long", self.target_word.len())
        }

        let mut letters = Vec::new();
        for (i, c) in word.chars().enumerate() {
            if self.target_word.contains(c) {
                if self.target_word.chars().nth(i) == Some(c) {
                    letters.push(Letter::Correct(c));
                } else {
                    letters.push(Letter::CorrectButWrongPosition(c));
                }
            } else {
                letters.push(Letter::Wrong(c));
            }
        }

        Ok(letters)
    }

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
