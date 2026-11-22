//! Konfiguration aus Umgebungsvariablen (siehe .env.example).
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub bot_token: String,
    pub base_url: String,
    pub database_url: String,
    pub bind_addr: String,
}

impl Config {
    pub fn from_env() -> Result<Self, BoxError> {
        let get = |k: &str| std::env::var(k).map_err(|_| format!("Umgebungsvariable {k} fehlt"));
        Ok(Self {
            client_id: get("DISCORD_CLIENT_ID")?,
            client_secret: get("DISCORD_CLIENT_SECRET")?,
            bot_token: get("DISCORD_BOT_TOKEN")?,
            base_url: get("DASHBOARD_BASE_URL")?.trim_end_matches('/').to_string(),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://wardenite.db".into()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into()),
        })
    }
}
