use std::sync::Arc;

use anyhow::anyhow;
use log::*;
use mobot::api::escape_md;
use mobot::*;

use crate::app::*;
use crate::wordle;

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
fn render_game(game: &wordle::Game) -> String {
    let mut s = String::from("Your attempts:\n\n");
    for attempt in &game.attempts {
        for letter in attempt {
            match letter {
                wordle::Letter::Correct(c) => {
                    s.push_str(&format!(" {}", emoji_letter(*c).to_string()))
                }
                wordle::Letter::CorrectButWrongPosition(c) => {
                    s.push_str(&format!(" * `{}` *  ", c))
                }
                wordle::Letter::Wrong(c) => {
                    s.push_str(&format!(" || ~{}~ ||  ", c))
                    // s.push_str("\u{2796} ")
                }
            }
        }
        s.push_str("\n\n");
    }
    s
}

pub async fn handle_new_game(e: Event, state: State<App>) -> Result<Action, anyhow::Error> {
    // Get the sender's first name
    let from = e.update.get_message()?.clone().from.unwrap_or_default();

    // Get the application state
    let mut app = state.get().write().await;
    let target_word;
    if let Err(e) = app.load(&from).await {
        warn!("No saved game state: {}", e);
    }

    target_word = app.start_game().await?;
    app.inc_games(&from).await; // saves state

    info!(
        "Starting new game with {} ({}), target word: {}.",
        from.first_name,
        from.username.clone().unwrap_or("unknown".into()),
        target_word
    );

    app.admin_log(
        Arc::clone(&e.api),
        format!(
            "{} ({}) starting a new game with word {}.",
            from.first_name,
            from.username.clone().unwrap_or_default(),
            target_word,
        ),
    )
    .await;

    let first_game = if app.score(&from.id.to_string()).await.games == 0 {
        "This is your first game.".to_string()
    } else {
        format!("Your score: {}.", app.score(&from.id.to_string()).await)
    };

    return Ok(Action::ReplyText(format!(
        "Hi {}, Welcome to {}!\n\n{}\nGuess the {}-letter word.",
        from.first_name,
        app.game_name,
        first_game,
        target_word.len()
    )));
}

pub async fn handle_bot_command(e: Event, state: State<App>) -> Result<Action, anyhow::Error> {
    // Get the command
    let command = e
        .update
        .get_message()?
        .text
        .as_ref()
        .ok_or(anyhow!("No command"))?;

    let reply = match command.as_str() {
        "/help" => {
            let game_name = state.get().read().await.game_name.clone();
            format!(
                "Welcome to {}! The goal of the game is to guess the target word within 6 tries.

Type /new to restart the game or /score to see your score",
                game_name
            )
        }

        "/new" => {
            return handle_new_game(e, state).await;
        }

        "/start" => {
            return handle_new_game(e, state).await;
        }

        "/admin" => {
            let mut app = state.get().write().await;
            if app
                .auth_admin(
                    e.update
                        .from_user()?
                        .username
                        .clone()
                        .unwrap_or_default()
                        .as_str(),
                    e.update.chat_id()?,
                )
                .await?
            {
                "Admin messages routed to this chat.".into()
            } else {
                "You are not an admin.".into()
            }
        }

        "/score" => {
            let from = e.update.get_message()?.clone().from.unwrap_or_default();
            let mut app = state.get().write().await;

            // Get the application state
            if let Err(e) = app.load(&from).await {
                warn!("No saved game state: {}", e);
                format!("You have not played any games yet.")
            } else {
                format!("Your score: {}", app.score(&from.id.to_string()).await)
            }
        }

        _ => "I don't know that command.".into(),
    };

    Ok(Action::ReplyText(reply))
}

/// handle_chat_event is the main Telegram handler for the bot.
pub async fn handle_chat_event(e: Event, state: State<App>) -> Result<Action, anyhow::Error> {
    // Get the message
    let message = e.update.get_message()?.clone().text.unwrap().clone();

    // Get the sender's first name
    let from = e.update.get_message()?.clone().from.unwrap_or_default();

    // Get the application state
    {
        let mut state = state.get().write().await;
        if let Err(err) = state.load(&from).await {
            warn!("No saved game state: {}", err);
            state
                .admin_log(
                    Arc::clone(&e.api),
                    format!(
                        "New user: {} ({})",
                        from.first_name,
                        from.username.clone().unwrap_or_default()
                    ),
                )
                .await;
        }
    }

    // If there's no active game, start one.
    if !state.get().read().await.is_playing() {
        // Scan the list for an unplayed word, or pick a random one.
        return handle_new_game(e, state).await;
    }

    // There's an active game, so play a turn.
    info!(
        "{} ({}) guessed {}",
        from.first_name,
        from.username.clone().unwrap_or("unknown".into()),
        message
    );

    // Play a turn
    let turn = state
        .get()
        .write()
        .await
        .play_turn(&from, message.clone())
        .await?;

    let (mut reply, target_word, attempted_letters, score) = {
        let app = state.get().read().await;
        let wordle = app.wordle.as_ref().unwrap();
        let reply = render_game(&wordle.game()?);
        let target_word = wordle.target_word.clone().to_uppercase();
        let attempted_letters = wordle
            .game()?
            .attempted_letters()
            .iter()
            .map(|c| format!("`{}`", c))
            .collect::<Vec<_>>()
            .join(" ");
        let score = app.score(&from.id.to_string()).await;

        (reply, target_word, attempted_letters, score)
    };

    match turn {
        Move::InvalidWord => {
            reply = format!(
                "Sorry {}, that's not a valid word\\. Try again\\.",
                escape_md(from.first_name.as_str())
            )
        }
        Move::InvalidLength => {
            reply = format!(
                "Sorry {}, the word must be {} letters long\\. Try again\\.",
                escape_md(from.first_name.as_str()),
                target_word.len()
            )
        }
        Move::Valid => reply.push_str(
            format!(
                "\nNice try\\. Guess another word\\?\nAttempts: {}",
                attempted_letters
            )
            .as_str(),
        ),
        Move::Won => {
            reply.push_str(
                escape_md(format!("\nYou won! \u{1F46F}\nYour score: {}", score).as_str()).as_str(),
            );
            info!(
                "{} ({}) won with {}",
                from.first_name,
                from.clone().username.unwrap_or("unknown".into()),
                message
            );
        }
        Move::Lost => {
            reply.push_str(
                escape_md(
                    format!(
                        "\nYou lost! Target word: {} \u{1F979}\nYour score: {}",
                        target_word, score
                    )
                    .as_str(),
                )
                .as_str(),
            );
            info!(
                "{} ({}) lost with {} (target: {})",
                from.first_name,
                from.clone().username.unwrap_or("unknown".into()),
                message,
                target_word
            );
        }
    }

    state
        .get()
        .read()
        .await
        .admin_log(
            Arc::clone(&e.api),
            format!(
                "{} ({}) played word '{}' against '{}' {}.",
                from.first_name,
                from.username.clone().unwrap_or_default(),
                message,
                target_word,
                match turn {
                    Move::InvalidWord => "which was invalid",
                    Move::InvalidLength => "which was the wrong length",
                    Move::Valid => "which was valid",
                    Move::Won => "and won",
                    Move::Lost => "and lost",
                }
            ),
        )
        .await;

    Ok(Action::ReplyMarkdown(reply))
}
