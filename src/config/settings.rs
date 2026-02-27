use anyhow::{bail, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Settings
// ─────────────────────────────────────────────────────────────────────────────

pub struct Settings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl Settings {
    /// Reads credentials from `.env` (or the shell environment).
    /// Expects the same variable names rspotify uses natively:
    ///
    ///   RSPOTIFY_CLIENT_ID
    ///   RSPOTIFY_CLIENT_SECRET
    ///   RSPOTIFY_REDIRECT_URI
    pub fn load() -> Result<Self> {
        if let Err(e) = dotenvy::dotenv() {
            if !e.not_found() {
                eprintln!("Warning: could not read .env file: {e}");
            }
        }

        let client_id = std::env::var("RSPOTIFY_CLIENT_ID").unwrap_or_default();
        let client_secret = std::env::var("RSPOTIFY_CLIENT_SECRET").unwrap_or_default();
        let redirect_uri = std::env::var("RSPOTIFY_REDIRECT_URI")
            .unwrap_or_else(|_| "http://127.0.0.1:8888/callback".to_string());

        if client_id.is_empty() {
            bail!(
                "RSPOTIFY_CLIENT_ID is not set.\n\
                 \n\
                 Your .env file should contain:\n\
                 \n\
                 \tRSPOTIFY_CLIENT_ID=your_client_id_here\n\
                 \tRSPOTIFY_CLIENT_SECRET=your_client_secret_here\n\
                 \tRSPOTIFY_REDIRECT_URI=http://127.0.0.1:8888/callback\n\
                 \n\
                 Get these from https://developer.spotify.com/dashboard"
            );
        }

        Ok(Settings {
            client_id,
            client_secret,
            redirect_uri,
        })
    }
}
