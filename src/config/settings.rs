use anyhow::{bail, Result};

pub struct Settings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl Settings {
    /// Loads credentials in priority order:
    ///   1. Shell environment variables (already exported)
    ///   2. ~/.config/spot-tty/.env
    ///   3. .env in the current working directory (dev fallback)
    pub fn load() -> Result<Self> {
        // 1. Try ~/.config/spot-tty/.env first
        let config_env = dirs::config_dir().map(|p| p.join("spot-tty").join(".env"));

        if let Some(ref path) = config_env {
            if path.exists() {
                if let Err(e) = dotenvy::from_path(path) {
                    eprintln!("Warning: could not read {}: {e}", path.display());
                }
            }
        }

        // 2. Also try cwd .env (dev convenience — won't override already-set vars)
        let _ = dotenvy::dotenv();

        let client_id = std::env::var("RSPOTIFY_CLIENT_ID").unwrap_or_default();
        let client_secret = std::env::var("RSPOTIFY_CLIENT_SECRET").unwrap_or_default();
        let redirect_uri = std::env::var("RSPOTIFY_REDIRECT_URI")
            .unwrap_or_else(|_| "http://127.0.0.1:8888/callback".to_string());

        if client_id.is_empty() {
            let config_path = config_env
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "~/.config/spot-tty/.env".to_string());
            bail!(
                "RSPOTIFY_CLIENT_ID is not set.\n\
                 \n\
                 Add your Spotify credentials to:\n\
                 \n\
                 \t{config_path}\n\
                 \n\
                 The file should contain:\n\
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
