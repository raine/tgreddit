# tgreddit

Get a feed of your favorite subreddits to Telegram.

The killer feature: No need to visit Reddit, as all media is embedded thanks to
[yt-dlp](yt-dlp) and Telegram's excellent media support.

<img align=left src="https://user-images.githubusercontent.com/11027/177398248-31e122d4-7e12-4986-9742-5f5a56c2529d.PNG" width=20% height=20%>
<img align=left src="https://user-images.githubusercontent.com/11027/177400544-685a89d0-3c2f-4e1a-8bc5-7802d0c6180d.jpeg" width=20% height=20%>
<img src="https://user-images.githubusercontent.com/11027/177397025-f1cdf171-ec0d-4f4a-aa3b-05ecacbb63bd.PNG" width=20% height=20%>

## install

```sh
$ cargo install tgreddit
```

### requirements

Depends on [yt-dlp](yt-dlp) (and for good results, yt-dlp requires ffmpeg).

## bot commands

### `/sub <subreddit> [limit=<limit>] [time=<time>] [filter=<filter>]`

Add a subscription to subreddit's top posts with optional options. Subscriptions
are conversation specific, and may be added in channels where the bot is
participating or in private chats with the bot.

If the options are not given, when checking for new posts, the program will
default to configuration in config.toml, if any.

Example: `/sub AnimalsBeingJerks limit=5 time=week filter=video`

Explanation: Subscribe to top posts in r/AnimalsBeingJerks so that the top 5
posts of the weekly top list are considered. Whenever a new post appears among
those top 5 posts, they will be posted in the conversation.

### `/unsub <subreddit>`

Remove a subscription from the current conversation.

### `/listsubs`

List all subreddit subscriptions for the current conversation.

## configuration

### env vars

- `CONFIG_PATH`: Path to TOML configuration file. **required**

### example toml configuration with the options explained

Example config without comments:
[config.example.toml](https://raw.githubusercontent.com/raine/tgreddit/master/config.example.toml)

```toml
# Path to a SQLite database used to track seen posts.
# Optional. Defaults to $HOME/.local/state/tgreddit/data.db3.
db_path = "/path/to/data.db3"

# List of Telegram user ids that can use the commands provided by the bot.
authorized_users = [
  123123123
]

# Token of your Telegram bot - you get this from @botfather.
telegram_bot_token = "..."

# How often to query each configured subreddit for new posts. Applies only if
# keep_running is enabled.
check_interval_secs = 600

# Whether posts seen on the first check of a new subreddit are considered new
# or not. Generally having this enabled is better unless you want multiple new
# messages when a new subreddit is added.
# Optional. Defaults to true.
skip_initial_send = true

# Set default limit of posts to fetch for each subreddit. Used when not
# specified for a subreddit.
# Optional. The default is 1.
default_limit = 1

# Set default time period of top list fetched. Used when not specified for a
# subreddit. String and one of: hour, day, week, month, year, all.
# Optional. The default is `day`.
default_time = "day"
```

Perhaps the simplest way to determine a Telegram channel's ID is to open the
channel in [Telegram Web client][telegram-web] and observing the numeric value
in page URL.

## docker image

There's a prebuilt Docker image with dependencies included at
[rainevi/tgreddit](https://hub.docker.com/repository/docker/rainevi/tgreddit).
Of course, you may also build your own using from the
[Dockerfile](https://raw.githubusercontent.com/raine/tgreddit/master/Dockerfile).

[yt-dlp]: https://github.com/yt-dlp/yt-dlp
[telegram-web]: https://web.telegram.org/
