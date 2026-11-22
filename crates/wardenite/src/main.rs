mod bot;
mod config;
mod db;
mod designer;
mod web;

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), config::BoxError> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();
    let cfg = config::Config::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;

    let state = web::AppState {
        pool: pool.clone(),
        http: Arc::new(poise::serenity_prelude::Http::new(&cfg.bot_token)),
        client: reqwest::Client::new(),
        cfg: Arc::new(cfg.clone()),
    };
    let web_task = tokio::spawn(web::serve(state));
    let bot_task = tokio::spawn(bot::run(cfg.bot_token.clone(), pool));

    tokio::select! {
        r = web_task => tracing::error!("Web beendet: {r:?}"),
        r = bot_task => tracing::error!("Bot beendet: {r:?}"),
    }
    Ok(())
}
