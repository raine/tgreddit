<h1 align="center">tgreddit</h1>

<p align="center">
  <strong>Reddit's top posts, delivered to Telegram</strong>
</p>

<p align="center">
  <a href="#installation">Install</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#configuration">Configuration</a> ·
  <a href="#deployment">Deployment</a>
</p>

---

A self-hosted Telegram bot that monitors your favorite subreddits and sends you
the top posts. All media is embedded directly in Telegram thanks to
[yt-dlp][yt-dlp] — no need to open Reddit.

<p align="center">
  <img src="https://user-images.githubusercontent.com/11027/178097057-83b27933-9876-405a-b151-a148960819df.jpeg" width="20%">
  <img src="https://user-images.githubusercontent.com/11027/178096986-5f651336-8208-4c40-9c41-58c95173b24d.jpeg" width="20%">
  <img src="https://user-images.githubusercontent.com/11027/178099572-e55c7f3c-986b-4804-8540-1004b36950df.jpeg" width="20%">
</p>

## Installation

Requires [yt-dlp][yt-dlp] and [ffmpeg][ffmpeg] at runtime for media downloads.

### Script

```bash
curl -fsSL https://raw.githubusercontent.com/raine/tgreddit/master/scripts/install.sh | bash
```

### Pre-built binaries

Download from [GitHub releases](https://github.com/raine/tgreddit/releases/latest):

| Platform              | Download                                                                                                                |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Linux (x64)           | [tgreddit-linux-x64.tar.gz](https://github.com/raine/tgreddit/releases/latest/download/tgreddit-linux-x64.tar.gz)       |
| Linux (ARM64)         | [tgreddit-linux-arm64.tar.gz](https://github.com/raine/tgreddit/releases/latest/download/tgreddit-linux-arm64.tar.gz)   |
| macOS (Apple Silicon) | [tgreddit-darwin-arm64.tar.gz](https://github.com/raine/tgreddit/releases/latest/download/tgreddit-darwin-arm64.tar.gz) |
| macOS (Intel)         | [tgreddit-darwin-x64.tar.gz](https://github.com/raine/tgreddit/releases/latest/download/tgreddit-darwin-x64.tar.gz)     |

### Cargo

```bash
cargo install tgreddit
```

### Docker

Pre-built image with all dependencies included:

```bash
docker pull rainevi/tgreddit
```

See [Docker deployment](#docker) below.

## Commands

### `/sub <subreddit> [time=<time>] [limit=<limit>] [filter=<filter>]`

Subscribe to a subreddit's top posts. The bot periodically checks the
subreddit's top posts and sends you any new ones that appear. Subscriptions are
per-conversation — add them in channels or private chats.

- **`time`** — the time window to watch: `hour`, `day`, `week`, `month`,
  `year`, `all`. Think of it as "top posts of the ___". Default: `day`.
- **`limit`** — how many top posts to watch. With `limit=3`, you'll see posts
  as they enter the top 3. Higher means more posts. Default: `1`.
- **`filter`** — only send certain post types: `image`, `video`, `link`,
  `self_text`, `gallery`.

When called with just a subreddit name, the bot replies with an inline keyboard
to configure settings interactively. If you already know the parameters you
want, pass them directly:

```
/sub AnimalsBeingJerks
/sub AnimalsBeingJerks time=week limit=5
/sub AnimalsBeingJerks time=week filter=video
```

### `/unsub <subreddit>`

Remove a subscription from the current conversation.

### `/listsubs`

List all subscriptions for the current conversation as an interactive inline
keyboard. Tapping a subscription opens an edit menu where you can:

- **Cycle limit** — tap to cycle through 1, 3, 5, 10, 25
- **Cycle time period** — tap to cycle through hour, day, week, month, year, all
- **Cycle filter** — tap to cycle through all, image, video, link, self_text, gallery
- **Pause / Resume** — pause a subscription so it stops checking for new posts
  without losing its settings
- **Unsubscribe** — remove the subscription

### `/get <subreddit> [time=<time>] [limit=<limit>] [filter=<filter>]`

One-shot fetch of current top posts without subscribing. Accepts the same
`time`, `limit`, and `filter` options as `/sub`.

## Configuration

### Environment variables

| Variable      | Description                              |
| ------------- | ---------------------------------------- |
| `CONFIG_PATH` | Path to TOML configuration file. **Required** |
| `RUST_LOG`    | Log level. `info` recommended.           |

### Config file

Full example: [config.example.toml](config.example.toml)

```toml
# Path to SQLite database for tracking seen posts.
# Optional. Defaults to $HOME/.local/state/tgreddit/data.db3.
db_path = "/path/to/data.db3"

# Telegram user IDs allowed to use bot commands.
authorized_user_ids = [123123123]

# Bot token from @BotFather.
telegram_bot_token = "..."

# How often to check for new posts (seconds).
check_interval_secs = 600

# Skip sending posts found on first check of a new subreddit.
# Prevents a flood of messages when adding a subscription.
# Optional. Default: true.
skip_initial_send = true

# Use an alternative Reddit frontend for comment links.
# Optional. Default: official Reddit.
links_base_url = "https://teddit.net"

# Default time window for top posts: hour, day, week, month, year, all.
# Think of it as "top posts of the ___".
# Optional. Default: day.
default_time = "day"

# Default number of top posts to watch per subreddit.
# With limit=3, you'll see posts as they enter the top 3.
# Optional. Default: 1.
default_limit = 1

# Default post type filter: image, video, link, self_text, gallery.
# Optional. Default: unset (all types).
default_filter = "video"
```

> **Tip**: The easiest way to find a Telegram channel's ID is to open it in
> [Telegram Web][telegram-web] and look at the numeric value in the URL.

## Deployment

### Docker

Pre-built image with yt-dlp and ffmpeg included:

```bash
docker run -d \
  -v /path/to/config.toml:/app/config.toml \
  -v /path/to/data:/data \
  -e CONFIG_PATH=/app/config.toml \
  -e RUST_LOG=info \
  rainevi/tgreddit
```

Image available at [rainevi/tgreddit](https://hub.docker.com/r/rainevi/tgreddit)
for both `linux/amd64` and `linux/arm64`.

### systemd

Example setup for running on a Linux server or Raspberry Pi.

1. **Install tgreddit and runtime dependencies:**

   ```bash
   curl -fsSL https://raw.githubusercontent.com/raine/tgreddit/master/scripts/install.sh | bash
   sudo apt install -y ffmpeg python3
   sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
   sudo chmod a+rx /usr/local/bin/yt-dlp
   ```

2. **Create a dedicated user and directories:**

   ```bash
   sudo useradd -r -s /usr/sbin/nologin tgreddit
   sudo mkdir -p /opt/tgreddit /var/lib/tgreddit
   sudo chown tgreddit:tgreddit /var/lib/tgreddit
   sudo cp "$(which tgreddit)" /opt/tgreddit/tgreddit
   sudo cp config.example.toml /opt/tgreddit/config.toml
   # Edit /opt/tgreddit/config.toml with your settings
   ```

3. **Create `/etc/systemd/system/tgreddit.service`:**

   ```ini
   [Unit]
   Description=tgreddit
   Documentation=https://github.com/raine/tgreddit
   After=network-online.target
   Wants=network-online.target

   [Service]
   Type=simple
   User=tgreddit
   Group=tgreddit
   WorkingDirectory=/opt/tgreddit
   Environment=CONFIG_PATH=/opt/tgreddit/config.toml
   Environment=RUST_LOG=info
   ExecStart=/opt/tgreddit/tgreddit
   Restart=on-failure
   RestartSec=5

   NoNewPrivileges=yes
   ProtectSystem=strict
   ProtectHome=yes
   PrivateTmp=yes
   ReadWritePaths=/var/lib/tgreddit
   RestrictSUIDSGID=yes
   ProtectKernelTunables=yes
   ProtectControlGroups=yes
   DevicePolicy=closed
   RestrictRealtime=yes
   LockPersonality=yes

   StandardOutput=journal
   StandardError=journal
   SyslogIdentifier=tgreddit

   [Install]
   WantedBy=multi-user.target
   ```

4. **Enable and start:**

   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now tgreddit
   sudo journalctl -u tgreddit -f   # follow logs
   ```

## Development

The project uses [`just`][just], [`direnv`][direnv] and [`entr`][entr].

```bash
just dev
```

[yt-dlp]: https://github.com/yt-dlp/yt-dlp
[ffmpeg]: https://ffmpeg.org/
[telegram-web]: https://web.telegram.org/
[just]: https://github.com/casey/just
[direnv]: https://direnv.net/
[entr]: https://github.com/eradman/entr
