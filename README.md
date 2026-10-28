# Pheme

An RSS reader for Linux, written in Rust and QML. Its look and behavior follow the Mac
version of [NetNewsWire](https://github.com/Ranchero-Software/NetNewsWire): a source list
with Today / All Unread / Starred, a timeline, and an article pane.

Like NetNewsWire, it has two kinds of accounts:

- **On My Computer**: feeds Pheme downloads itself (RSS, Atom, and JSON Feed). Read and
  starred state stays on this computer. It starts out subscribed to Daring Fireball and Six Colors.
- **Feedly** (optional): your Feedly subscriptions, with read and starred state synced.

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

Pheme refreshes every 30 minutes, six feeds at a time, and asks servers only for feeds
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

## Signing in to Feedly

Choose it on the welcome screen, or later from the menu at the bottom of the sidebar.
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
- `src/bridge/`: cxx-qt objects exposed to QML
- `qml/`: the interface

Qt writes its log messages to the journal when not run from a terminal:
`journalctl --user -t pheme`.

Tests: `cargo test`. Two more tests use the network and the process-wide database, so
point them at a scratch directory: `XDG_DATA_HOME=/tmp/pheme-test cargo test -- --ignored`.

## License notes

The article template and stylesheet are adapted from NetNewsWire, © Brent Simmons, MIT License.
