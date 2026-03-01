#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# spot-tty installer
# Usage: curl -fsSL https://raw.githubusercontent.com/YOUR_USERNAME/spot-tty/main/install.sh | bash
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info() { echo -e "${CYAN}  →${RESET} $*"; }
success() { echo -e "${GREEN}  ✓${RESET} $*"; }
warn() { echo -e "${YELLOW}  ⚠${RESET} $*"; }
error() {
  echo -e "${RED}  ✗${RESET} $*"
  exit 1
}
header() { echo -e "\n${BOLD}$*${RESET}"; }

REPO="https://github.com/Gaurav-Gali/spot-tty"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/spot-tty"
NVIM_PLUGIN_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/nvim/plugins/spot-tty.nvim"
NVIM_LAZY_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/nvim/lua/plugins"

echo -e "${BOLD}"
echo "  ███████╗██████╗  ██████╗ ████████╗    ████████╗████████╗██╗   ██╗"
echo "  ██╔════╝██╔══██╗██╔═══██╗╚══██╔══╝       ██╔══╝╚══██╔══╝╚██╗ ██╔╝"
echo "  ███████╗██████╔╝██║   ██║   ██║    █████╗██║      ██║    ╚████╔╝ "
echo "  ╚════██║██╔═══╝ ██║   ██║   ██║    ╚════╝██║      ██║     ╚██╔╝  "
echo "  ███████║██║     ╚██████╔╝   ██║          ██║      ██║      ██║   "
echo "  ╚══════╝╚═╝      ╚═════╝    ╚═╝          ╚═╝      ╚═╝      ╚═╝   "
echo -e "${RESET}"
echo -e "  ${CYAN}Spotify TUI for your terminal — and Neovim${RESET}"
echo ""

# ── 1. Check OS ───────────────────────────────────────────────────────────────
header "Checking system..."
OS="$(uname -s)"
case "$OS" in
Linux | Darwin) success "OS: $OS" ;;
*) error "Unsupported OS: $OS (Linux and macOS only)" ;;
esac

# ── 2. Check / install Rust ───────────────────────────────────────────────────
header "Checking Rust toolchain..."
if command -v cargo &>/dev/null; then
  RUST_VER="$(rustc --version | awk '{print $2}')"
  success "Rust $RUST_VER already installed"
else
  warn "Rust not found — installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
  success "Rust installed"
fi

# ── 3. Clone repo ─────────────────────────────────────────────────────────────
header "Fetching spot-tty..."
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v git &>/dev/null; then
  git clone --depth=1 "$REPO" "$TMP_DIR/spot-tty" || error "Failed to clone $REPO — check your internet connection and that the repo is public"
  success "Cloned repository"
else
  error "git is required but not installed. Install git and re-run."
fi

# ── 4. Build ──────────────────────────────────────────────────────────────────
header "Building spot-tty (this takes ~1 min on first run)..."
cd "$TMP_DIR/spot-tty"
cargo build --release --quiet
success "Build complete"

# ── 5. Install binary ─────────────────────────────────────────────────────────
header "Installing binary..."
mkdir -p "$INSTALL_DIR"
cp target/release/spot-tty "$INSTALL_DIR/spot-tty"
chmod +x "$INSTALL_DIR/spot-tty"
success "Binary installed to $INSTALL_DIR/spot-tty"

# Ensure ~/.local/bin is on PATH
SHELL_RC=""
case "$SHELL" in
*/zsh) SHELL_RC="$HOME/.zshrc" ;;
*/bash) SHELL_RC="$HOME/.bashrc" ;;
*/fish) SHELL_RC="$HOME/.config/fish/config.fish" ;;
esac

PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
if [[ -n "$SHELL_RC" ]] && ! grep -q '.local/bin' "$SHELL_RC" 2>/dev/null; then
  echo "" >>"$SHELL_RC"
  echo "# spot-tty" >>"$SHELL_RC"
  echo "$PATH_LINE" >>"$SHELL_RC"
  warn "Added ~/.local/bin to PATH in $SHELL_RC — run: source $SHELL_RC"
fi

# ── 6. Spotify credentials ────────────────────────────────────────────────────
header "Spotify API credentials..."
mkdir -p "$CONFIG_DIR"

if [[ -f "$CONFIG_DIR/.env" ]] && grep -q "RSPOTIFY_CLIENT_ID=" "$CONFIG_DIR/.env"; then
  success "Credentials already set at $CONFIG_DIR/.env — skipping"
else
  echo ""
  echo -e "  You need a Spotify Developer app. Steps:"
  echo -e "    ${CYAN}1.${RESET} Go to ${BOLD}https://developer.spotify.com/dashboard${RESET}"
  echo -e "    ${CYAN}2.${RESET} Create an app (any name)"
  echo -e "    ${CYAN}3.${RESET} In app Settings → Redirect URIs → add: ${BOLD}http://127.0.0.1:8888/callback${RESET}"
  echo -e "    ${CYAN}4.${RESET} Copy your Client ID and Client Secret"
  echo ""

  read -rp "  Client ID:     " CLIENT_ID
  read -rp "  Client Secret: " CLIENT_SECRET

  if [[ -z "$CLIENT_ID" || -z "$CLIENT_SECRET" ]]; then
    error "Client ID and Secret cannot be empty"
  fi

  cat >"$CONFIG_DIR/.env" <<EOF
RSPOTIFY_CLIENT_ID=$CLIENT_ID
RSPOTIFY_CLIENT_SECRET=$CLIENT_SECRET
RSPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback
EOF
  success "Credentials saved to $CONFIG_DIR/.env"
fi

# ── 7. Neovim plugin (optional) ───────────────────────────────────────────────
header "Neovim plugin (optional)..."
echo ""

INSTALL_NVIM=false
if command -v nvim &>/dev/null; then
  read -rp "  Neovim detected — install spot-tty.nvim plugin? [Y/n] " REPLY
  REPLY="${REPLY:-Y}"
  [[ "$REPLY" =~ ^[Yy]$ ]] && INSTALL_NVIM=true
else
  info "Neovim not found — skipping plugin install"
fi

if $INSTALL_NVIM; then
  # Copy plugin files
  mkdir -p "$NVIM_PLUGIN_DIR/plugin" "$NVIM_PLUGIN_DIR/lua/spot-tty"

  cat >"$NVIM_PLUGIN_DIR/plugin/spot-tty.lua" <<'LUAEOF'
if vim.g.loaded_spot_tty then return end
vim.g.loaded_spot_tty = 1
vim.api.nvim_create_user_command("SpotTty", function()
  require("spot-tty").toggle()
end, { desc = "Toggle spot-tty music player" })
LUAEOF

  cat >"$NVIM_PLUGIN_DIR/lua/spot-tty/init.lua" <<'LUAEOF'
local M = {}
local state = { buf = nil, win = nil }

M.config = {
  binary    = "spot-tty",
  width     = 0.85,
  height    = 0.85,
  border    = "rounded",
  keymap    = "<leader>ts",
  title     = " 󰝚  spot-tty ",
  title_pos = "center",
}

local function is_open()
  return state.win ~= nil and vim.api.nvim_win_is_valid(state.win)
end

local function calc_size()
  local w = math.floor(vim.o.columns * M.config.width)
  local h = math.floor(vim.o.lines   * M.config.height)
  return { width = w, height = h,
           row = math.floor((vim.o.lines - h) / 2),
           col = math.floor((vim.o.columns - w) / 2) }
end

local function open()
  local sz  = calc_size()
  local cfg = M.config
  state.buf = vim.api.nvim_create_buf(false, true)
  state.win = vim.api.nvim_open_win(state.buf, true, {
    relative = "editor", width = sz.width, height = sz.height,
    row = sz.row, col = sz.col, style = "minimal",
    border = cfg.border, title = cfg.title, title_pos = cfg.title_pos,
  })
  vim.wo[state.win].winblend       = 0
  vim.wo[state.win].cursorline     = false
  vim.wo[state.win].number         = false
  vim.wo[state.win].relativenumber = false
  vim.wo[state.win].signcolumn     = "no"
  vim.fn.termopen(cfg.binary, {
    env     = { SPOT_TTY_NVIM = "1" },
    on_exit = function() vim.schedule(function() M.close() end) end,
  })
  vim.cmd("startinsert")
  vim.keymap.set("n", "q",     function() M.close() end, { buffer = state.buf, noremap = true, silent = true })
  vim.keymap.set("t", "<C-q>", function() M.close() end, { buffer = state.buf, noremap = true, silent = true })
end

function M.close()
  if is_open() then vim.api.nvim_win_close(state.win, true) end
  state.win = nil
  if state.buf ~= nil and vim.api.nvim_buf_is_valid(state.buf) then
    vim.api.nvim_buf_delete(state.buf, { force = true })
  end
  state.buf = nil
end

function M.toggle()
  if is_open() then M.close() else open() end
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
  if M.config.keymap then
    vim.keymap.set("n", M.config.keymap, M.toggle,
      { noremap = true, silent = true, desc = "Toggle spot-tty music player" })
  end
end

return M
LUAEOF

  success "Plugin files written to $NVIM_PLUGIN_DIR"

  # Write lazy.nvim spec if lua/plugins exists or user confirms
  mkdir -p "$NVIM_LAZY_DIR"
  LAZY_SPEC="$NVIM_LAZY_DIR/spot-tty.lua"

  if [[ -f "$LAZY_SPEC" ]]; then
    warn "Lazy spec already exists at $LAZY_SPEC — not overwriting"
  else
    cat >"$LAZY_SPEC" <<LAZYEOF
return {
  dir = vim.fn.expand("$NVIM_PLUGIN_DIR"),
  config = function()
    require("spot-tty").setup({
      binary = vim.fn.expand("$INSTALL_DIR/spot-tty"),
      keymap = "<leader>ts",
    })
  end,
}
LAZYEOF
    success "Lazy spec written to $LAZY_SPEC"
    info "Run :Lazy sync inside Neovim to activate the plugin"
  fi
fi

# ── Done ──────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}${GREEN}  ✓ spot-tty installed!${RESET}"
echo ""
echo -e "  ${BOLD}Terminal:${RESET}  spot-tty"
if $INSTALL_NVIM; then
  echo -e "  ${BOLD}Neovim:${RESET}    <leader>ts  or  :SpotTty"
  echo -e "           (run :Lazy sync first if the keymap isn't working)"
fi
echo ""
echo -e "  On first launch, a browser window will open to authenticate with Spotify."
echo -e "  Tip: if your app is in Spotify's Development Mode, add your email at"
echo -e "       ${CYAN}https://developer.spotify.com/dashboard${RESET} → app → Settings → User Management"
echo ""
