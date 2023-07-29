use crate::{app::App, handlers::handle_chat_event};
use log::*;
use mobot::*;

/// This is an end-to-end test that starts the bot with just one target word ("hello"). It then
/// starts two chats with the bot, and has them play the game. The first chat should win right
/// away, and the second chat attempts 6 turns and loses.
#[tokio::test]
async fn it_works() {
    mobot::init_logger();

    // Create a FakeAPI and attach it to the client. Any Telegram requests are now forwarded
    // to `fakeserver` instead.
    let fakeserver = fake::FakeAPI::new();
    let client = Client::new("token".to_string().into()).with_post_handler(fakeserver.clone());

    // Keep the Telegram poll timeout short for testing. The default Telegram poll timeout is 60s.
    let mut router = Router::new(client)
        .with_state(App::new("BadWordle".into(), vec!["hello".to_string()]))
        .with_poll_timeout_s(1);

    router.add_route(Route::Message(Matcher::Any), handle_chat_event);

    // Since we're passing ownership of the Router to a background task, grab the
    // shutdown channels so we can shut it down from this task.
    let (shutdown_notifier, shutdown_tx) = router.shutdown();

    // Start the router in a background task.
    tokio::spawn(async move {
        info!("Starting router...");
        router.start().await;
    });

    // We're in the foreground. Create a new chat session with the bot, providing your
    // username. This shows up in the `from` field of messages.
    let chat = fakeserver.create_chat("qubyte").await;

    // Send a message to the bot. This starts a new game.
    chat.send_text("hi").await.unwrap();
    assert!(chat
        .recv_update()
        .await
        .unwrap()
        .to_string()
        .starts_with("Hi qubyte, Welcome to BadWordle!"));

    // Start a new chat as a different user and send a message. This should also
    // start a new game with the new user.
    let chat2 = fakeserver.create_chat("hacker").await;
    chat2.send_text("hi").await.unwrap();
    assert!(chat2
        .recv_update()
        .await
        .unwrap()
        .to_string()
        .starts_with("Hi hacker, Welcome to BadWordle!"));

    // First user attempts a word and wins right away.
    chat.send_text("hello").await.unwrap();
    assert!(chat
        .recv_update()
        .await
        .unwrap()
        .to_string()
        .contains("You won"));

    // Second user attempts 5 incorrect words and misses.
    for _ in 0..5 {
        chat2.send_text("bello").await.unwrap();
        assert!(chat2
            .recv_update()
            .await
            .unwrap()
            .to_string()
            .contains("Your attempts"));
    }

    // Second user attempts the wrong word and loses.
    chat2.send_text("bello").await.unwrap();
    assert!(chat2
        .recv_update()
        .await
        .unwrap()
        .to_string()
        .contains("You lost"));

    // All done shutdown the router, and wait for it to complete.
    info!("Shutting down...");
    shutdown_tx.send(()).await.unwrap();
    shutdown_notifier.notified().await;
}
