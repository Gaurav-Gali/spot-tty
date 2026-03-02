# spot-tty — Architecture & Engineering Deep Dive

---

## 1. High-Level System Architecture

```mermaid
graph TB
    subgraph USER["User Environment"]
        TERM["Terminal\n(Kitty / WezTerm / iTerm2 / any)"]
        NVIM["Neovim\n(:terminal emulator)"]
    end

    subgraph APP["spot-tty Binary"]
        direction TB
        MAIN["main.rs\nEvent Loop + Tick (33ms)"]

        subgraph STATE["Redux-style State Layer"]
            APPSTATE["AppState\n(single source of truth)"]
            EVENTS["AppEvent\n(typed event enum)"]
            REDUCER["reduce()\n(pure state transitions)"]
        end

        subgraph UI["UI Render Layer (ratatui)"]
            SIDEBAR["sidebar.rs\nPlaylists + Account"]
            EXPLORER["explorer.rs\nTrack Table + Detail"]
            PLAYER["player.rs\nPlayback + Visualizer"]
            COVER["cover.rs\nCover Art Protocol"]
            SEARCH["search.rs\nFuzzy Search Overlay"]
            TRACKMENU["trackmenu.rs\nTrack Actions"]
            PROFILE["profile.rs\nUser Stats + Commands"]
        end

        subgraph SERVICES["Async Service Layer"]
            AUTH["auth.rs\nOAuth PKCE + Token Cache"]
            SPOTIFY["spotify.rs\nSpotify REST API"]
        end
    end

    subgraph SPOTIFY_CLOUD["Spotify Platform"]
        OAUTH["OAuth 2.0\nAuthorize Endpoint"]
        WEBAPI["Web API\nREST Endpoints"]
    end

    subgraph STORAGE["Local Storage"]
        TOKEN["~/.config/spot-tty/\ntoken.json"]
        ENV[".env\nAPI Credentials"]
        IMGCACHE["~/.cache/spot-tty/\nCover Art (JPEG/PNG)"]
    end

    TERM <-->|"crossterm events\n+ ratatui render"| MAIN
    NVIM <-->|"SPOT_TTY_NVIM=1\n+ floating :terminal"| MAIN
    MAIN --> EVENTS
    EVENTS --> REDUCER
    REDUCER --> APPSTATE
    APPSTATE --> UI
    MAIN -->|"tokio::spawn\nasync tasks"| SERVICES
    SERVICES -->|"AppEvent via\nmpsc channel"| EVENTS
    AUTH <-->|"PKCE flow\nTCP callback :8888"| OAUTH
    SPOTIFY <-->|"Bearer token\nREST calls"| WEBAPI
    AUTH <--> TOKEN
    AUTH <--> ENV
    COVER <--> IMGCACHE
```

---

## 2. Event-Driven State Machine (Redux Architecture)

```mermaid
flowchart LR
    subgraph INPUT["Input Sources"]
        KB["Keyboard\n(crossterm)"]
        TICK["33ms Tick\n(tokio interval)"]
        NET["Network\n(Spotify API)"]
    end

    subgraph DISPATCH["Event Dispatch"]
        TX["mpsc::Sender\nAppEvent"]
        RX["mpsc::Receiver\nAppEvent"]
    end

    subgraph CORE["Core Loop (main thread)"]
        REDUCE["reduce(state, event)\nPure function — no I/O"]
        STATE["AppState\n(immutable snapshot)"]
        DRAW["terminal.draw()\nratatui render"]
    end

    subgraph ASYNC["Async Tasks (tokio::spawn)"]
        T1["Initial fetch\nuser + playlists + liked"]
        T2["Playback poll\nevery 2 seconds"]
        T3["Cover fetch\nlazy per visible track"]
        T4["Search\n400ms debounce"]
        T5["Playback cmd\nplay/pause/skip/queue"]
    end

    KB -->|"KeyEvent"| TX
    TICK -->|"Tick"| TX
    NET -->|"API response"| TX
    TX --> RX
    RX --> REDUCE
    REDUCE --> STATE
    STATE --> DRAW
    REDUCE -->|"side-effect\ntokio::spawn"| ASYNC
    ASYNC -->|"AppEvent\nvia tx.clone()"| TX
```

---

## 3. OAuth PKCE Authentication Flow

```mermaid
sequenceDiagram
    participant U as User
    participant A as spot-tty
    participant B as Browser
    participant S as Spotify OAuth
    participant T as TCP :8888

    A->>A: Load .env credentials
    A->>A: Check token.json cache
    alt Token valid & all scopes present
        A->>A: Use cached token ✓
    else No token / missing scopes
        A->>A: Generate PKCE code_verifier + challenge
        A->>B: open::that(authorize_url)
        B->>S: GET /authorize?code_challenge=...
        S->>U: Login + consent screen
        U->>S: Approve scopes
        S->>T: GET /callback?code=xxx (redirect)
        A->>T: TcpListener::accept() catches callback
        T->>B: HTTP 200 "✓ Authenticated!"
        A->>S: POST /token (code + verifier)
        S->>A: access_token + refresh_token
        A->>A: Write token.json cache
    end
    A->>A: Start main event loop
```

---

## 4. UI Component Tree & Render Pipeline

```mermaid
graph TD
    ROOT["Terminal Frame\n(full screen Rect)"]

    ROOT --> LAYOUT["Horizontal Layout\n[sidebar 24col | explorer Min | player 22row]"]

    LAYOUT --> SB["Sidebar\n━━━━━━━━"]
    LAYOUT --> EX["Explorer\n━━━━━━━━"]
    LAYOUT --> PL["Player\n━━━━━━━━"]

    SB --> ACC["Account panel\n(username)"]
    SB --> PLIST["Playlist list\n(thumbnails + names)"]
    SB --> LIKED["Liked Songs\n(fixed bottom)"]

    EX --> EXLAYOUT["Horizontal split\n[row covers | table | detail]"]
    EXLAYOUT --> RC["Row covers\n6×3 cell thumbnails"]
    EXLAYOUT --> TABLE["Track table\n(# Title Artist Album Time)"]
    EXLAYOUT --> DETAIL["Detail panel\n(cover 38×19 + metadata)"]

    PL --> PROG["Progress bar\n(local interpolation)"]
    PL --> VIZ["Visualizer\n(10 bars, 4-layer sine, 30fps)"]
    PL --> INFO["Now playing\n(track + artist + breadcrumb)"]

    ROOT --> OVERLAYS["Overlays (conditional)"]
    OVERLAYS --> SRCH["Search overlay\n(fuzzy / catalog)"]
    OVERLAYS --> TMENU["Track menu\n(play / queue)"]
    OVERLAYS --> PROF["Profile overlay\n(stats / commands / logout)"]
    OVERLAYS --> TOAST["Toast notification\n(2s auto-dismiss)"]
```

---

## 5. Cover Art Rendering Pipeline

```mermaid
flowchart TD
    FETCH["fetch_cover(url)\nreqwest::get()"]
    DISK["Disk cache\n~/.cache/spot-tty/\n{url_hash}.jpg"]
    DECODE["image::load_from_memory()\nDynamicImage"]
    DETECT["detect_protocol()"]

    FETCH -->|"cache miss"| HTTP["HTTP download"]
    FETCH -->|"cache hit"| DISK
    HTTP --> SAVECACHE["Save to disk"]
    SAVECACHE --> DISK
    DISK --> DECODE
    DECODE --> DETECT

    DETECT -->|"SPOT_TTY_NVIM=1"| HB
    DETECT -->|"TERM=xterm-kitty"| KITTY
    DETECT -->|"TERM_PROGRAM=iTerm.app"| ITERM
    DETECT -->|"fallback"| HB

    subgraph KITTY["Kitty Protocol"]
        KQ["queue_kitty()\nBase64 PNG escape"]
        KF["flush after draw()\nwrite to stdout"]
        KQ --> KF
    end

    subgraph ITERM["iTerm2 Protocol"]
        IQ["queue_iterm2()\nBase64 raw escape"]
        IF["flush after draw()"]
        IQ --> IF
    end

    subgraph HB["Half-block Unicode"]
        CACHE["RenderCache.halfblock\n(kitty_id, w, h) → Vec<u8>"]
        RESIZE["resize_exact(w, h×2)\nLanczos3 — once only"]
        RENDER["▀ per cell\nfg=top pixel\nbg=bottom pixel\n2× vertical res"]
        CACHE -->|"hit"| RENDER
        CACHE -->|"miss"| RESIZE
        RESIZE --> CACHE
    end
```

---

## 6. Async Concurrency Model

```mermaid
graph TB
    subgraph MAIN_THREAD["Main Thread (single-threaded event loop)"]
        EVENTLOOP["select! loop\n• crossterm events\n• mpsc channel drain\n• 33ms tick"]
        REDUCER2["reduce(state, event)"]
        RENDER["terminal.draw()"]
        EVENTLOOP --> REDUCER2 --> RENDER --> EVENTLOOP
    end

    subgraph TOKIO["Tokio Runtime (multi-threaded)"]
        direction LR
        T_INIT["spawn: initial_fetches\nuser + playlists + liked\n(parallel join!)"]
        T_POLL["spawn: playback_poll\nevery 2s interval"]
        T_COVER["spawn: cover_fetcher\nlazy, per visible URL"]
        T_SEARCH["spawn: search_debounce\n400ms after keystroke"]
        T_CMD["spawn: playback_cmd\nplay/pause/skip/queue"]
    end

    REDUCER2 -->|"side effects via\ntokio::spawn"| TOKIO
    TOKIO -->|"tx.send(AppEvent)\nmpsc channel"| EVENTLOOP

    style MAIN_THREAD fill:#1e1e2e,color:#cdd6f4
    style TOKIO fill:#181825,color:#cdd6f4
```

---

## 7. Neovim Plugin Architecture

```mermaid
graph TD
    subgraph NEOVIM["Neovim Process"]
        LAZYCFG["lazy.nvim spec\nlua/plugins/spot-tty.lua"]
        PLUGIN["plugin/spot-tty.lua\n:SpotTty command"]
        INIT["lua/spot-tty/init.lua\nM.toggle() / M.setup()"]
        FLOATWIN["nvim_open_win()\nfloating window"]
        TERMBUF["nvim_create_buf()\n:terminal buffer"]
        TERMOPEN["vim.fn.termopen()\nspawns spot-tty"]
    end

    subgraph SPOTTTY["spot-tty Process"]
        ENVCHECK["SPOT_TTY_NVIM=1\nenv var detected"]
        NOALTSCR["Skip EnterAlternateScreen\n(would corrupt nvim display)"]
        FORCEHB["Force HalfBlock\nimage protocol"]
        MAINLOOP["Normal event loop\n(crossterm on pty)"]
    end

    LAZYCFG --> PLUGIN
    PLUGIN -->|"require spot-tty"| INIT
    INIT -->|"leader+ts"| FLOATWIN
    FLOATWIN --> TERMBUF
    TERMBUF --> TERMOPEN
    TERMOPEN -->|"SPOT_TTY_NVIM=1\nspawn"| SPOTTTY
    ENVCHECK --> NOALTSCR
    ENVCHECK --> FORCEHB
    NOALTSCR --> MAINLOOP
    FORCEHB --> MAINLOOP

    TERMOPEN -->|"on_exit callback"| CLOSE["M.close()\nbuf_delete force"]
```

---

## 8. Install & Distribution Pipeline

```mermaid
flowchart TD
    subgraph REPO["GitHub Repo\ngithub.com/Gaurav-Gali/spot-tty"]
        SRC["Rust source code"]
        ISH["install.sh\n(macOS / Linux)"]
        IPS1["install.ps1\n(Windows)"]
        README2["README.md"]
    end

    subgraph MACOS_LINUX["macOS / Linux User"]
        CURL["curl -fsSL ... | bash"]
        CHECK_RUST["Check cargo installed"]
        INSTALL_RUST["rustup-init.sh\n(if missing)"]
        GIT_CLONE["git clone --depth=1"]
        CARGO_BUILD["RUSTFLAGS='-A warnings'\ncargo build --release"]
        BIN["~/.local/bin/spot-tty"]
        CREDS["Prompt: Client ID + Secret\n→ OS config dir / .env"]
        NVIM_Q["Detect nvim?\nInstall plugin?"]
        PLUGIN_FILES["~/.config/nvim/plugins/\nspot-tty.nvim/"]
    end

    subgraph WINDOWS["Windows User"]
        PS1RUN["irm ... | iex\n(PowerShell)"]
        WIN_RUST["rustup-init.exe\n(if missing)"]
        WIN_BUILD["cargo build --release"]
        WIN_BIN["%USERPROFILE%\\.local\\bin\\spot-tty.exe"]
        WIN_CREDS["Prompt: credentials\n→ %APPDATA%\\spot-tty\\.env"]
    end

    subgraph RUNTIME["First Launch"]
        ENV_LOAD["Load .env from OS config dir"]
        TOKEN_CHECK["Check token.json cache"]
        BROWSER["open browser → Spotify OAuth"]
        TCP["TcpListener :8888\ncatch redirect"]
        READY["App ready ✓"]
    end

    CURL --> CHECK_RUST
    CHECK_RUST -->|"missing"| INSTALL_RUST
    CHECK_RUST -->|"present"| GIT_CLONE
    INSTALL_RUST --> GIT_CLONE
    GIT_CLONE --> CARGO_BUILD
    CARGO_BUILD --> BIN
    BIN --> CREDS
    CREDS --> NVIM_Q
    NVIM_Q -->|"yes"| PLUGIN_FILES

    PS1RUN --> WIN_RUST --> WIN_BUILD --> WIN_BIN --> WIN_CREDS

    BIN --> ENV_LOAD
    WIN_BIN --> ENV_LOAD
    ENV_LOAD --> TOKEN_CHECK
    TOKEN_CHECK -->|"no token"| BROWSER
    BROWSER --> TCP --> READY
    TOKEN_CHECK -->|"valid token"| READY
```

---

## 9. Data Flow — Track Playback

```mermaid
sequenceDiagram
    participant U as User (Enter key)
    participant M as main.rs
    participant R as reducer.rs
    participant S as spotify.rs
    participant SP as Spotify API

    U->>M: KeyCode::Enter
    M->>M: tx.send(AppEvent::Enter)
    M->>R: reduce(state, Enter)
    R->>R: state.key_mode = Playing
    R->>R: state.now_playing = track (optimistic)
    R->>R: state.progress_ms = 0
    M->>M: tokio::spawn(play_task)
    M->>S: play_track(spotify, track, context_uri)
    S->>SP: PUT /me/player/play
    SP-->>S: 204 No Content
    Note over M: 2s poll tick
    M->>S: fetch_playback_state()
    S->>SP: GET /me/player
    SP-->>S: JSON PlaybackState
    S-->>M: tx.send(PlaybackStateUpdated)
    M->>R: reduce(state, PlaybackStateUpdated)
    R->>R: Update progress, is_playing, device_id
```

---

## 10. Visualizer — Signal Processing

```mermaid
flowchart LR
    TICK2["33ms tick\n(~30 fps)"]
    PHASE["phase += 1\n(integer counter)"]
    T["t = phase × 0.033\n(seconds equivalent)"]

    subgraph BAR["Per bar (10 bars total)"]
        X["x = bar_index / 10.0"]
        S1["sin(t×1.7 + x×6.3)"]
        S2["sin(t×π + x×4.1) × 0.6"]
        S3["sin(t×2.3 + x×8.7) × 0.4"]
        S4["sin(t×φ + x×5.5) × 0.3\n(φ=1.618 golden ratio)"]
        SUM["sum → normalize → [0,1]"]
        HEIGHT["height = sum × max_h\n(sub-cell: ▁▂▃▄▅▆▇█)"]
        COLOR["gradient\ngreen→yellow→orange→red\n(based on height %)"]
    end

    TICK2 --> PHASE --> T --> BAR
    S1 --> SUM
    S2 --> SUM
    S3 --> SUM
    S4 --> SUM
    SUM --> HEIGHT
    HEIGHT --> COLOR
```

---

## DevOps & Engineering Concepts Used

| Concept | Implementation |
|---|---|
| **Unidirectional data flow** | Redux pattern: AppEvent → reduce() → AppState → render() |
| **Actor model (lite)** | tokio::spawn tasks communicate only via mpsc channels |
| **Optimistic UI updates** | State updated immediately on user action; API confirms async |
| **Protocol detection** | Runtime capability sniffing (Kitty / iTerm2 / half-block fallback) |
| **Lazy loading** | Covers fetched only for visible items per frame |
| **Disk cache** | SHA-256 URL → filename, covers persisted across sessions |
| **In-memory render cache** | Half-block pixel buffers keyed by (image_id, w, h) — resize once |
| **OAuth 2.0 PKCE** | Proof Key for Code Exchange — no client secret in auth flow |
| **Local HTTP server** | TcpListener on :8888 catches OAuth redirect without a web server |
| **Token validation** | Scope diff on load — re-auth if scopes expanded since last login |
| **Scroll debounce** | 120ms settle timer before fetching cover for selected track |
| **Search debounce** | 400ms after last keystroke before hitting Spotify catalog API |
| **Parallel init fetches** | tokio::join! for user + playlists + liked tracks simultaneously |
| **Platform abstraction** | dirs::config_dir() → correct path on macOS / Linux / Windows |
| **Environment detection** | SPOT_TTY_NVIM=1 → skip alt-screen, force half-block, adjust layout |
| **CI-free distribution** | Source distribution via install.sh / install.ps1 — users build locally |
| **Frame deduplication** | RenderCache skips re-sending Kitty escape sequences if unchanged |
| **Vim-style navigation** | Modal key handling (Normal / Search / Menu / Profile modes) |
