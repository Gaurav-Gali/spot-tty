use anyhow::{Context, Result};
use rspotify::{prelude::*, scopes, AuthCodePkceSpotify, Credentials, OAuth};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
};
use tokio::fs;
use tracing::{info, warn};

pub fn token_cache_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("spot-tty")
        .join("token.json")
}

pub fn build_client(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> AuthCodePkceSpotify {
    let creds = Credentials {
        id: client_id.to_string(),
        secret: Some(client_secret.to_string()),
    };

    let oauth = OAuth {
        redirect_uri: redirect_uri.to_string(),
        scopes: scopes!(
            "user-read-private",
            "user-read-email",
            "user-library-read",
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-follow-read",
            "user-read-playback-state",
            "user-modify-playback-state",
            "user-read-currently-playing"
        ),
        ..Default::default()
    };

    let config = rspotify::Config {
        token_cached: true,
        token_refreshing: true,
        cache_path: token_cache_path(),
        ..Default::default()
    };

    AuthCodePkceSpotify::with_config(creds, oauth, config)
}

pub async fn authenticate(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<AuthCodePkceSpotify> {
    if let Some(parent) = token_cache_path().parent() {
        fs::create_dir_all(parent).await?;
    }

    let mut spotify = build_client(client_id, client_secret, redirect_uri);

    // ── Try cached token ──────────────────────────────────────────────────
    if token_cache_path().exists() {
        match spotify.read_token_cache(true).await {
            Ok(Some(token)) => {
                info!("Loaded token from cache");
                *spotify.token.lock().await.unwrap() = Some(token);

                match spotify.current_user().await {
                    Ok(_) => {
                        info!("Cached token is valid");
                        return Ok(spotify);
                    }
                    Err(e) => {
                        warn!("Cached token invalid ({e}), re-authenticating");
                    }
                }
            }
            Ok(None) => info!("No cached token found"),
            Err(e) => warn!("Failed to read token cache: {e}"),
        }
    }

    // ── Full PKCE flow ────────────────────────────────────────────────────
    let auth_url = spotify.get_authorize_url(None)?;

    if let Err(e) = open::that(&auth_url) {
        eprintln!(
            "\nCould not open browser automatically ({e}).\n\
             Please open this URL manually:\n\n  {auth_url}\n"
        );
    } else {
        eprintln!("\nOpening Spotify login in your browser…");
    }

    let code = wait_for_callback().context("OAuth callback server failed")?;

    spotify
        .request_token(&code)
        .await
        .context("Failed to exchange auth code for token")?;

    spotify
        .write_token_cache()
        .await
        .context("Failed to write token cache")?;

    info!("Authentication successful, token cached");
    Ok(spotify)
}

fn wait_for_callback() -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:8888")
        .context("Could not bind to 127.0.0.1:8888 — is something else using that port?")?;

    eprintln!("Waiting for Spotify to redirect back to http://127.0.0.1:8888/callback …");

    let (stream, _) = listener.accept()?;
    let mut reader = BufReader::new(&stream);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let code = parse_code_from_request(&request_line)
        .context("Spotify callback did not contain a `code` parameter")?;

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body style=\"font-family:sans-serif;padding:2rem\">\
        <h2>✓ spot-tty authenticated!</h2>\
        <p>You can close this tab and return to your terminal.</p>\
        </body></html>";

    (&stream).write_all(response.as_bytes())?;

    Ok(code)
}

fn parse_code_from_request(request_line: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;

    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some("code") {
            return kv.next().map(|v| v.to_string());
        }
    }
    None
}
