use crate::{app::App, handlers::handle_chat_event};
use log::*;
use mobot::*;

#[tokio::test]
async fn it_works() {
    mobot::init_logger();

    // Create a FakeAPI and attach it to the client. Any Telegram requests are now forwarded
    // to `fakeserver` instead.
    let fakeserver = fake::FakeAPI::new();
    let client = Client::new("token".to_string().into()).with_post_handler(fakeserver.clone());

    // Keep the Telegram poll timeout short for testing. The default Telegram poll timeout is 60s.
    let mut router = Router::new(client)
        .with_state(App::new(
            "BadWordle".into(),
            vec!["hello".to_string(), "world".to_string()],
        ))
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

    // Send the message "ping1", expect the response "pong(1): ping1"
    println!("Sending ping");
    chat.send_text("ping1").await.unwrap();
    println!(
        "chat.recv_update().await.unwrap().to_string(): {}",
        chat.recv_update().await.unwrap().to_string()
    );

    /*
    assert_eq!(
        chat.recv_update().await.unwrap().to_string(),
        "pong: qubyte"
    );

    let chat2 = fakeserver.create_chat("hacker").await;

    // Send the message "ping1", expect the response "pong(1): ping1"
    chat2.send_text("ping1").await.unwrap();
    assert_eq!(
        chat2.recv_update().await.unwrap().to_string(),
        "Sorry! Unauthorized user: hacker."
    );
    */

    // All done shutdown the router, and wait for it to complete.
    info!("Shutting down...");
    shutdown_tx.send(()).await.unwrap();
    shutdown_notifier.notified().await;
}
