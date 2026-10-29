# Pheme

An RSS reader for Linux, written in Rust and QML. Its look and behavior follow the Mac
version of [NetNewsWire](https://github.com/Ranchero-Software/NetNewsWire): a source list
with Today / All Unread / Starred, a timeline, and an article pane.

Like NetNewsWire, it has two kinds of accounts:

- **On My Computer**: feeds Pheme downloads itself (RSS, Atom, and JSON Feed). Read and
  starred state stays on this computer. It starts out subscribed to Daring Fireball, Six Colors, and MacRumors.
- **Feedly** (optional, not finished): your Feedly subscriptions, with read and starred
  state synced. Off behind a feature flag for now; see below.

## Narrow windows

The layout adapts to the window's width, so Pheme works tiled beside other apps:

- **Wide (980 px and up):** sidebar, timeline, and article side by side. Hide the sidebar
  with the sidebar button or Ctrl+Alt+S.
- **Medium (560–980 px):** two panes at a time: sidebar and timeline while choosing a
  feed, timeline and article while reading. The sidebar button switches between them.
- **Narrow (under 560 px):** one pane at a time, with back buttons. ← and → move between them.

## Feeds on your computer

- **New Feed** (Ctrl+N, or the **+** button): enter a feed's address, or just a site's,
  like `inessential.com`; Pheme finds the feed from the page.
- **New Folder** (Ctrl+Shift+N). Right-click a feed or folder to rename, move, or delete it.
- **Import/Export Subscriptions (OPML)** from the menu at the bottom of the sidebar, to
  move your subscriptions from or to another reader.

Pheme refreshes every 30 minutes (change it in Settings), six feeds at a time, and asks servers only for feeds
that changed. Articles older than 90 days are not imported, and read, unstarred ones that
old are cleaned up.

## Building

Needs Rust (2024 edition) and Qt 6 with Qt Quick Controls and Qt WebEngine
(on Arch: `qt6-base qt6-declarative qt6-webengine`).

    cargo run --release

To install for your user:

    cargo install --path .
    install -Dm644 data/pheme.desktop ~/.local/share/applications/pheme.desktop
    install -Dm644 resources/icons/pheme.svg ~/.local/share/icons/hicolor/scalable/apps/pheme.svg

## Settings

**Settings…** (Ctrl+,, or the menu at the bottom of the sidebar) has NetNewsWire's
General preferences:

- **Appearance:** match the desktop's light or dark setting (the default), or always
  use light or dark. Article pages follow it too.
- **Unread Counts:** show the sidebar's unread counts, a dot, or nothing.
- **Article Text Size**
- **Article Theme:** Pheme's default, NetNewsWire's bundled themes, or your own.
  **Import Theme…** installs a NetNewsWire `.nnwtheme` folder, or a zip file holding
  one, and switches to it; importing a theme again replaces it. Themes live in
  `~/.local/share/pheme/Themes` (**Open Themes Folder**); a `.nnwtheme` folder copied
  there by hand shows up the next time Settings comes forward.
- **Article Content:** enable JavaScript in articles, for embedded videos and posts.
  Off by default. The NewsFax, Appanoose, and Promenade themes use a little JavaScript
  for decoration and look slightly plainer without it.
- **Refresh Feeds:** manually only, or every 30 minutes to every 8 hours.

## Feature flags and the Developer tab

Feedly support is off behind a feature flag. Release builds always leave it off. Dev
builds (`cargo run`, without `--release`) add a **Developer** tab to Settings, where
**Feedly support** turns it on. Turning it off signs out of
Feedly. With the flag off there's no welcome screen and no Feedly menu items.

## Signing in to Feedly

With the flag on, choose it on the welcome screen, or later from the menu at the bottom of the sidebar.
Feedly offers two ways in:

1. **Developer access token** (works today). Get one at <https://feedly.com/v3/auth/dev>
   and paste it into Pheme's sign-in screen. Feedly's developer tokens expire and can't be
   refreshed, so you'll need to paste a new one when Pheme says Feedly no longer accepts it.

2. **"Sign In with Feedly" (OAuth)**. This needs a Feedly API client ID and secret, which
   Feedly issues on request (NetNewsWire's are private). Once you have one, either set
   `PHEME_FEEDLY_CLIENT_ID`, `PHEME_FEEDLY_CLIENT_SECRET` (and optionally
   `PHEME_FEEDLY_REDIRECT_URI`, default `http://localhost:8080/`) at build time or run
   time, or create `~/.config/pheme/feedly.json`:

       { "client_id": "…", "client_secret": "…", "redirect_uri": "http://localhost:8080/" }

   Pheme opens your browser and listens on that localhost address for Feedly's redirect.
   OAuth tokens refresh automatically.

## Where things live

- `~/.local/share/pheme/pheme.db`: both accounts' feeds and articles, and read/starred changes waiting to go to Feedly
- `~/.local/share/pheme/credentials.json`: your Feedly tokens (mode 0600)
- `~/.config/pheme/Pheme.conf`: window layout and view preferences

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| ↑ ↓ | Previous / next item |
| ← → | Move between sidebar, timeline, and article |
| N, Space, + | Next unread |
| R | Toggle read |
| S | Toggle starred |
| K | Mark all as read |
| B, Return | Open article in browser |
| Ctrl+R, F5 | Refresh |
| Ctrl+N | New feed |
| Ctrl+Shift+N | New folder |
| Delete | Delete the selected local feed or folder |
| Ctrl+F | Search articles |
| Ctrl+Alt+S | Show/hide the sidebar |
| Ctrl+Shift+H | Hide/show read articles |
| Ctrl+= / Ctrl+- | Article text size |

## Layout

- `src/feedly/`: Feedly API client, models, and OAuth (after NetNewsWire's `Account/Feedly`)
- `src/sync.rs`: sync phases, after `FeedlyAccountDelegate.refreshAll`
- `src/local.rs`: the On My Computer account: feed discovery, fetching, parsing, icons, OPML
- `src/store.rs`: SQLite storage
- `src/render.rs`, `resources/article/`: article HTML, using NetNewsWire's template and stylesheet (MIT)
- `src/themes.rs`, `resources/themes/`: article themes, including NetNewsWire's bundled ones (MIT)
- `src/bridge/`: cxx-qt objects exposed to QML
- `qml/`: the interface

Qt writes its log messages to the journal when not run from a terminal:
`journalctl --user -t pheme`.

Tests: `cargo test`. Two more tests use the network and the process-wide database, so
point them at a scratch directory: `XDG_DATA_HOME=/tmp/pheme-test cargo test -- --ignored`.

## License notes

The article template and stylesheet are adapted from NetNewsWire, © Brent Simmons, MIT License.
The themes in `resources/themes/` come from NetNewsWire's repository (MIT License); each
theme's Info.plist names its creator. NewsFax's font is ModeSeven (freeware); see its License.md.
