use crate::wordle::*;

#[test]
fn it_works() {
    let mut wordle = Wordle::new("hello".into()).unwrap();
    let game = wordle.play_turn("bolle").unwrap();

    assert_eq!(
        game.attempts.first().unwrap().first().unwrap().clone(),
        Letter::Wrong('B')
    );

    assert_eq!(
        game.attempts.first().unwrap().get(1).unwrap().clone(),
        Letter::CorrectButWrongPosition('O')
    );

    assert_eq!(
        game.attempts.first().unwrap().get(2).unwrap().clone(),
        Letter::Correct('L')
    );
}
