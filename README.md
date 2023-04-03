# tgreddit

A telegram bot that gives you a feed of top posts from your favorite subreddits.

The killer feature: No need to visit Reddit, as all media is embedded thanks to
[yt-dlp][yt-dlp] and Telegram's excellent media support.

Intended to be self-hosted, as Reddit's API has rate-limiting and downloading
videos with `yt-dlp` can be resource intensive. The simplest way to self-host is
to use the prebuilt [docker image](#docker-image) that includes necessary
dependencies.

<img align=left src="https://user-images.githubusercontent.com/11027/178097057-83b27933-9876-405a-b151-a148960819df.jpeg" width=20% height=20%>
<img align=left src="https://user-images.githubusercontent.com/11027/178096986-5f651336-8208-4c40-9c41-58c95173b24d.jpeg" width=20% height=20%>
<img src="https://user-images.githubusercontent.com/11027/178099572-e55c7f3c-986b-4804-8540-1004b36950df.jpeg" width=20% height=20%>

## install

```sh
$ cargo install tgreddit
```

### requirements

Depends on [yt-dlp][yt-dlp] (and for good results, yt-dlp requires ffmpeg).

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

See the
[example configuration](#example-toml-configuration-with-the-options-explained)
below for further explanation on `limit`, `time`, and `filter`.

### `/unsub <subreddit>`

Remove a subscription from the current conversation.

### `/listsubs`

List all subreddit subscriptions for the current conversation.

### `/get <subreddit> [limit=<limit>] [time=<time>] [filter=<filter>]`

Get the current top posts similarly to how subscribing to a subreddit would
return new posts.

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

# Set the post comments links to use an alternative frontend. Useful as the
# official Reddit web app is increasingly user hostile on mobile. Possible
# alternative frontends include teddit.net and libredd.it, but you can use any.
# Optional. Defaults to official Reddit.
links_base_url = "https://teddit.net"

# Set default limit of posts to fetch for each subreddit. Used when not
# specified for a subreddit in the /sub command.
#
# Explanation in more detail: Whenever the bot gets the list of top posts for a
# subreddit, it will only consider the first <limit> posts. For example, if
# your limit is 5, the first time around bot will see 5 new posts and mark those
# as seen and not post anything because it's the first check. Next time around, if
# there's an unseen post among those 5 top posts, it will be posted in Telegram.
#
# So essentially larger the number used as limit, the more posts you can
# expect to see. For example, with time=month and limit=1 you would see a new post
# only when the montly top post changes, which is not that often.
#
# Optional. The default is 1.
default_limit = 1

# Set default time period of top list fetched. Used when not specified for a
# subreddit. String and one of: hour, day, week, month, year, all.
# Optional. The default is `day`.
default_time = "day"

# Set default filter for post type. When fetching for new posts, only posts
# matching the filter are considered.
# String and one of: image, video, link, self_text, gallery
# Optional and unset by default, meaning all post types are considered.
default_filter = "video"
```

Perhaps the simplest way to determine a Telegram channel's ID is to open the
channel in [Telegram Web client][telegram-web] and observing the numeric value
in page URL.

## docker image

There's a prebuilt Docker image with dependencies included at
[rainevi/tgreddit](https://hub.docker.com/repository/docker/rainevi/tgreddit).

Of course, you may also build your own using from the
[Dockerfile](https://raw.githubusercontent.com/raine/tgreddit/master/Dockerfile).

## have an idea, question or a bug report?

Feel free to open an issue or start a new discussion.

[yt-dlp]: https://github.com/yt-dlp/yt-dlp
[telegram-web]: https://web.telegram.org/
