# WordleBot

WordleBot is a Telegram bot that implements the wordle game. It's built with the [MOBOT](github.com/0xfe/mobot)
Telegram API framework.

## Try it out

You can try out Bad Worlde Bot by starting a chat with [@badwordlebot](t.me/badwordlebot).

Features:

- Provide arbitrary word lists
- Words can be any length
- Keeps track of played words per chat
- Keeps track of wins/losses per user
- Words must be offensive (okay, that's not a real feature)

## Usage

Set you Telegram API key and run `wordlebot` with a set of word files:

```
Usage: wordlebot [-n <game-name>] [-t <target-words>] [-v <valid-words>] [-s <save-dir>]

Reach new heights.

Options:
  -n, --game-name   how the bot presents itself in the welcome message
  -t, --target-words
                    file containing target words for the bot, one per line
  -v, --valid-words file containing valid words for the bot, one per line
  -s, --save-dir    directory to save user state. If empty, state is not saved.
  --help            display usage information
```

### Example

```
export TELEGRAM_TOKEN="your Telegram API key"
wordlebot -t target_words.txt -v validwords.txt -s /path/to/savedir
```

## License

MIT License Copyright 2023 Mohit Muthanna Cheppudira

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the “Software”), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.