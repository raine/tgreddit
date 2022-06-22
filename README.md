# tgreddit

Get a feed of the best stuff in Reddit to Telegram.

The killer feature: No need to visit Reddit, as all media is embedded thanks to
[yt-dlp](yt-dlp) and Telegram's excellent media support.

https://user-images.githubusercontent.com/11027/174842488-f886f8f4-d527-4afa-9c7e-4528e7130afa.mp4

## install

```sh
$ cargo install tgreddit
```

### requirements

Depends on `yt-dlp` and `ffmpeg`.

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

# Token of your Telegram bot - you get this from @botfather.
telegram_bot_token = "..."

# Keep the program running after checking configured subreddits for new posts,
# and check again periodically. Setting to `false` would be useful when running
# the program with crontab.
keep_running = true

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
default_limit = 1

# Set default time period of top list fetched. Used when not specified for a
# subreddit. String and one of: hour, day, week, month, year, all
default_time = "day"

# A map value with the key being the Telegram chat id (channel, or your user id
# for DM) and the value a list of subreddit configurations.
[channels]
100000 = [
  # Fetch the top 5 posts of the last week for /r/rust
  {subreddit="rust", limit=5, time="week"},

  # Fetch the top post of the last month for /r/golang
  {subreddit="golang", limit=1, time="month"},

  # Query /r/AnimalsBeingJerks and use the default values for `limit` and
  # `time` from above. Consider only videos.
  {subreddit="AnimalsBeingJerks", filter="video"},
]
100001 = [
  # etc.
]
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
