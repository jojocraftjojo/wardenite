//! Datenbankzugriff (SQLite via sqlx, Laufzeit-Queries).
use crate::config::BoxError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use wardenite_core::moderation::{self, AutomodConfig, Threshold};

pub async fn connect(url: &str) -> Result<SqlitePool, BoxError> {
    let opts = SqliteConnectOptions::from_str(url)?.create_if_missing(true).foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}

#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct GuildSettings {
    pub guild_id: i64,
    pub modlog_channel: Option<i64>,
    pub welcome_channel: Option<i64>,
    pub welcome_message: Option<String>,
    pub autorole_id: Option<i64>,
    pub block_invites: bool,
    pub spam_max_messages: i64,
    pub banned_words: String,
    pub warn_thresholds: String,
}

impl GuildSettings {
    pub fn words(&self) -> Vec<String> {
        serde_json::from_str(&self.banned_words).unwrap_or_default()
    }
    pub fn thresholds(&self) -> Vec<Threshold> {
        serde_json::from_str(&self.warn_thresholds).unwrap_or_default()
    }
    pub fn thresholds_text(&self) -> String {
        moderation::format_thresholds(&self.thresholds())
    }
    pub fn automod(&self) -> AutomodConfig {
        AutomodConfig {
            block_invites: self.block_invites,
            banned_words: self.words(),
            spam_max_messages: self.spam_max_messages.max(0) as u32,
        }
    }
}

pub async fn get_settings(pool: &SqlitePool, guild: i64) -> Result<GuildSettings, BoxError> {
    let row = sqlx::query_as::<_, GuildSettings>(
        "SELECT guild_id, modlog_channel, welcome_channel, welcome_message, autorole_id, \
         block_invites, spam_max_messages, banned_words, warn_thresholds \
         FROM guild_settings WHERE guild_id = ?",
    )
    .bind(guild)
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or(GuildSettings {
        guild_id: guild,
        banned_words: "[]".into(),
        warn_thresholds: "[]".into(),
        ..Default::default()
    }))
}

pub async fn save_settings(pool: &SqlitePool, s: &GuildSettings) -> Result<(), BoxError> {
    sqlx::query(
        "INSERT INTO guild_settings (guild_id, modlog_channel, welcome_channel, welcome_message, autorole_id, \
         block_invites, spam_max_messages, banned_words, warn_thresholds) VALUES (?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(guild_id) DO UPDATE SET modlog_channel=excluded.modlog_channel, \
         welcome_channel=excluded.welcome_channel, welcome_message=excluded.welcome_message, \
         autorole_id=excluded.autorole_id, block_invites=excluded.block_invites, \
         spam_max_messages=excluded.spam_max_messages, banned_words=excluded.banned_words, \
         warn_thresholds=excluded.warn_thresholds",
    )
    .bind(s.guild_id)
    .bind(s.modlog_channel)
    .bind(s.welcome_channel)
    .bind(&s.welcome_message)
    .bind(s.autorole_id)
    .bind(s.block_invites)
    .bind(s.spam_max_messages)
    .bind(&s.banned_words)
    .bind(&s.warn_thresholds)
    .execute(pool)
    .await?;
    Ok(())
}
