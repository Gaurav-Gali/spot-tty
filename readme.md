# spot-tty 󰝚

A Spotify TUI for your terminal — built with Rust and ratatui.
Works standalone in any terminal, and as a floating Neovim plugin.

![spot-tty screenshot](https://raw.githubusercontent.com/YOUR_USERNAME/spot-tty/main/screenshot.png)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Gaurav-Gali/spot-tty/main/install.sh | bash
```

The script will:

1. Install Rust if not already present
2. Clone and build spot-tty
3. Place the binary at `~/.local/bin/spot-tty`
4. Ask for your Spotify API credentials and save them
5. Optionally install the Neovim plugin (auto-detected)

> **Requires:** git, curl. Rust is installed automatically if missing.

---

## Spotify setup

You need a free Spotify Developer app to get API credentials:

1. Go to [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard)
2. Click **Create app** (any name/description)
3. In app **Settings → Redirect URIs** add: `http://127.0.0.1:8888/callback`
4. Copy your **Client ID** and **Client Secret** — the installer will ask for these

> **Important:** Spotify apps start in Development Mode. To use write features (queue), go to **Settings → User Management** and add your Spotify account email.

---

## Usage

### Terminal

```bash
spot-tty
```

On first launch a browser window opens for Spotify OAuth. After that, credentials are cached at `~/.config/spot-tty/token.json`.

### Neovim

Press `<leader>ts` to open spot-tty in a floating window.
Press `q` to close and return to your code.

Or run `:SpotTty` from the command line.

---

## Keybinds

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `l` / `→` / `Enter` | Focus explorer |
| `h` / `←` | Focus sidebar |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `M` | Jump to middle |
| `gp` | Jump to Playlists |
| `gl` | Jump to Liked Songs |
| `1`–`9` | Numeric prefix (e.g. `5j`) |

### Playback

| Key | Action |
|-----|--------|
| `Enter` | Play selected track |
| `Space` | Pause / Resume |
| `n` | Next track |
| `N` | Previous track |

### Overlays

| Key | Action |
|-----|--------|
| `/` | Fuzzy search |
| `i` | Track actions (play, queue) |
| `p` | Profile & stats |
| `q` | Quit |

### Inside overlays

| Key | Action |
|-----|--------|
| `↑` `↓` | Navigate |
| `Enter` | Confirm |
| `Esc` | Close |

---

## Configuration

Credentials are stored at `~/.config/spot-tty/.env`:

```env
RSPOTIFY_CLIENT_ID=your_client_id
RSPOTIFY_CLIENT_SECRET=your_client_secret
RSPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback
```

You can also set these as shell environment variables instead of using the file.

---

## Neovim plugin

The installer sets this up automatically. To configure manually, add to your lazy.nvim config:

```lua
return {
  dir = vim.fn.expand("~/.config/nvim/plugins/spot-tty.nvim"),
  config = function()
    require("spot-tty").setup({
      binary    = vim.fn.expand("~/.local/bin/spot-tty"),
      keymap    = "<leader>ts",   -- set to false to disable
      width     = 0.85,           -- fraction of editor width
      height    = 0.85,           -- fraction of editor height
      border    = "rounded",      -- "rounded"|"single"|"double"|"solid"
      title     = " 󰝚  spot-tty ",
    })
  end,
}
```

Then run `:Lazy sync` inside Neovim.

---

## Uninstall

```bash
rm ~/.local/bin/spot-tty
rm -rf ~/.config/spot-tty
rm -rf ~/.config/nvim/plugins/spot-tty.nvim
rm ~/.config/nvim/lua/plugins/spot-tty.lua
```

---

## Building from source

```bash
git clone https://github.com/YOUR_USERNAME/spot-tty
cd spot-tty
cargo build --release
ln -sf $(pwd)/target/release/spot-tty ~/.local/bin/spot-tty
```

---

## Requirements

- macOS or Linux
- Rust 1.75+ (installed automatically by the installer)
- A Spotify account (free or premium)
- A terminal with true colour support

> Cover art uses the Kitty graphics protocol in Kitty/WezTerm/Ghostty/iTerm2,
> and half-block Unicode characters as fallback in all other terminals (including Neovim).
