//! Dashboard: Discord-OAuth2-Login, Serverliste, Einstellungen, Tickets, Mod-Log, Server-Designer.
use crate::bot;
use crate::config::{BoxError, Config};
use crate::db::{self, GuildSettings};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use poise::serenity_prelude as serenity;
use serde::Deserialize;
use serenity::{ChannelId, ChannelType, GuildId};
use sqlx::SqlitePool;
use std::fmt::Write as _;
use std::sync::Arc;
use wardenite_core::moderation;
use wardenite_core::template::{self, Template};

/// Rechte des Bots bei der Einladung (Kanaele/Rollen verwalten, Kick, Ban, Timeout, Nachrichten).
const BOT_PERMISSIONS: u64 = 1_099_780_140_054;
const API: &str = "https://discord.com/api/v10";

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub cfg: Arc<Config>,
    pub http: Arc<serenity::Http>,
    pub client: reqwest::Client,
}

struct Session {
    username: String,
    access_token: String,
}

#[derive(Deserialize)]
struct UserGuild {
    id: String,
    name: String,
    owner: bool,
    permissions: String,
}

impl UserGuild {
    fn can_manage(&self) -> bool {
        let p: u64 = self.permissions.parse().unwrap_or(0);
        self.owner || p & 0x8 != 0 || p & 0x20 != 0
    }
}

// ---------- Helfer ----------

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{} · Wardenite</title><style>\
body{{font-family:system-ui,sans-serif;max-width:860px;margin:2rem auto;padding:0 1rem;background:#16181d;color:#e6e8ee}}\
a{{color:#7c9cff}}nav a{{margin-right:1rem}}input,select,textarea{{background:#22252d;color:inherit;border:1px solid #3a3f4b;border-radius:6px;padding:.4rem;width:100%;box-sizing:border-box}}\
label{{display:block;margin:.8rem 0 .2rem}}button{{background:#5865f2;color:#fff;border:0;border-radius:6px;padding:.5rem 1rem;cursor:pointer;margin-top:1rem}}\
table{{width:100%;border-collapse:collapse}}td,th{{padding:.4rem;border-bottom:1px solid #2c303a;text-align:left}}.muted{{color:#8b90a0}}\
</style></head><body><h1><a href=\"/\" style=\"text-decoration:none\">Wardenite</a></h1>{}</body></html>",
        esc(title),
        body
    ))
}

fn wrap(r: Result<Response, BoxError>) -> Response {
    r.unwrap_or_else(|e| {
        tracing::error!("Fehler im Dashboard: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, page("Fehler", &format!("<p>Interner Fehler: {}</p>", esc(&e.to_string())))).into_response()
    })
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').filter_map(|p| p.trim().split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
}

fn set_cookie(cfg: &Config, name: &str, value: &str, max_age: i64) -> HeaderValue {
    let secure = if cfg.base_url.starts_with("https") { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

async fn session(state: &AppState, headers: &HeaderMap) -> Result<Option<Session>, BoxError> {
    let Some(id) = cookie(headers, "wsid") else { return Ok(None) };
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT username, access_token FROM sessions WHERE id = ? AND expires_at > datetime('now')",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.map(|(username, access_token)| Session { username, access_token }))
}

async fn user_guilds(state: &AppState, s: &Session) -> Result<Vec<UserGuild>, BoxError> {
    let r = state
        .client
        .get(format!("{API}/users/@me/guilds"))
        .bearer_auth(&s.access_token)
        .send()
        .await?
        .error_for_status()?;
    Ok(r.json().await?)
}

fn invite_url(cfg: &Config, guild: Option<&str>) -> String {
    let mut u = format!(
        "https://discord.com/oauth2/authorize?client_id={}&scope=bot%20applications.commands&permissions={BOT_PERMISSIONS}",
        urlenc(&cfg.client_id)
    );
    if let Some(g) = guild {
        let _ = write!(u, "&guild_id={}&disable_guild_select=true", urlenc(g));
    }
    u
}

/// Prueft Login und Verwaltungsrecht; Err enthaelt die fertige Antwort (Redirect/403).
async fn authorize(state: &AppState, headers: &HeaderMap, gid: i64) -> Result<Result<(Session, GuildId), Response>, BoxError> {
    let Some(sess) = session(state, headers).await? else {
        return Ok(Err(Redirect::to("/login").into_response()));
    };
    let guilds = user_guilds(state, &sess).await?;
    let ok = gid > 0 && guilds.iter().any(|g| g.id == gid.to_string() && g.can_manage());
    if !ok {
        return Ok(Err((StatusCode::FORBIDDEN, page("Kein Zugriff", "<p>Dir fehlt die Berechtigung fuer diesen Server.</p>")).into_response()));
    }
    Ok(Ok((sess, GuildId::new(gid as u64))))
}

fn nav(gid: i64) -> String {
    format!(
        "<nav><a href=\"/dashboard\">Server</a><a href=\"/guild/{gid}\">Einstellungen</a><a href=\"/guild/{gid}/tickets\">Tickets</a>\
<a href=\"/guild/{gid}/modlog\">Mod-Log</a><a href=\"/guild/{gid}/designer\">Server-Designer</a></nav>"
    )
}

fn select(name: &str, options: &[(i64, String)], selected: Option<i64>) -> String {
    let mut s = format!("<select name=\"{name}\"><option value=\"\">(keiner)</option>");
    for (id, label) in options {
        let sel = if Some(*id) == selected { " selected" } else { "" };
        let _ = write!(s, "<option value=\"{id}\"{sel}>{}</option>", esc(label));
    }
    s.push_str("</select>");
    s
}

fn parse_id(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok().filter(|v| *v > 0)
}

/// Text-/Kategorie-Kanaele und Rollen eines Servers fuer Auswahlfelder.
async fn lists(state: &AppState, gid: GuildId) -> Result<(Vec<(i64, String)>, Vec<(i64, String)>, Vec<(i64, String)>), BoxError> {
    let channels = state.http.get_channels(gid).await?;
    let roles = state.http.get_guild_roles(gid).await?;
    let text = channels.iter().filter(|c| c.kind == ChannelType::Text).map(|c| (c.id.get() as i64, format!("#{}", c.name))).collect();
    let cats = channels.iter().filter(|c| c.kind == ChannelType::Category).map(|c| (c.id.get() as i64, c.name.clone())).collect();
    let roles = roles.iter().filter(|r| r.id.get() != gid.get()).map(|r| (r.id.get() as i64, r.name.clone())).collect();
    Ok((text, cats, roles))
}

// ---------- Router ----------

pub async fn serve(state: AppState) -> Result<(), BoxError> {
    let addr = state.cfg.bind_addr.clone();
    let app = Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/callback", get(callback))
        .route("/logout", get(logout))
        .route("/dashboard", get(dashboard))
        .route("/guild/:gid", get(settings_page).post(settings_save))
        .route("/guild/:gid/tickets", get(tickets_page))
        .route("/guild/:gid/tickets/panel", post(ticket_panel))
        .route("/guild/:gid/tickets/:tid", get(transcript))
        .route("/guild/:gid/modlog", get(modlog))
        .route("/guild/:gid/designer", get(designer_list))
        .route("/guild/:gid/designer/:name", get(designer_preview).post(designer_apply))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Dashboard auf http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

// ---------- Login ----------

async fn index(State(s): State<AppState>) -> Response {
    page(
        "Start",
        &format!(
            "<p>Open-Source-Bot fuer Moderation, Tickets und Server-Design.</p>\
<p><a href=\"/login\"><button>Mit Discord anmelden</button></a> <a href=\"{}\">Bot einladen</a></p>",
            esc(&invite_url(&s.cfg, None))
        ),
    )
    .into_response()
}

async fn login(State(s): State<AppState>) -> Response {
    let state_token = uuid::Uuid::new_v4().to_string();
    let redirect = format!("{}/callback", s.cfg.base_url);
    let url = format!(
        "https://discord.com/oauth2/authorize?client_id={}&response_type=code&scope=identify%20guilds&redirect_uri={}&state={}",
        urlenc(&s.cfg.client_id),
        urlenc(&redirect),
        state_token
    );
    let mut resp = Redirect::to(&url).into_response();
    resp.headers_mut().append(header::SET_COOKIE, set_cookie(&s.cfg, "wstate", &state_token, 600));
    resp
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
}

async fn callback(State(s): State<AppState>, headers: HeaderMap, Query(q): Query<CallbackQuery>) -> Response {
    wrap(callback_inner(s, headers, q).await)
}

async fn callback_inner(s: AppState, headers: HeaderMap, q: CallbackQuery) -> Result<Response, BoxError> {
    if cookie(&headers, "wstate").as_deref() != Some(q.state.as_str()) {
        return Ok((StatusCode::BAD_REQUEST, page("Fehler", "<p>Ungueltiger Login-Status. Bitte erneut versuchen.</p>")).into_response());
    }
    let redirect = format!("{}/callback", s.cfg.base_url);
    let token: TokenResponse = s
        .client
        .post(format!("{API}/oauth2/token"))
        .form(&[
            ("client_id", s.cfg.client_id.as_str()),
            ("client_secret", s.cfg.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", q.code.as_str()),
            ("redirect_uri", redirect.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let user: DiscordUser = s
        .client
        .get(format!("{API}/users/@me"))
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let sid = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (id, user_id, username, access_token, expires_at) VALUES (?,?,?,?, datetime('now', ?))")
        .bind(&sid)
        .bind(user.id.parse::<i64>()?)
        .bind(&user.username)
        .bind(&token.access_token)
        .bind(format!("+{} seconds", token.expires_in))
        .execute(&s.pool)
        .await?;
    let mut resp = Redirect::to("/dashboard").into_response();
    resp.headers_mut().append(header::SET_COOKIE, set_cookie(&s.cfg, "wsid", &sid, token.expires_in));
    resp.headers_mut().append(header::SET_COOKIE, set_cookie(&s.cfg, "wstate", "", 0));
    Ok(resp)
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(id) = cookie(&headers, "wsid") {
        let _ = sqlx::query("DELETE FROM sessions WHERE id = ?").bind(id).execute(&s.pool).await;
    }
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut().append(header::SET_COOKIE, set_cookie(&s.cfg, "wsid", "", 0));
    resp
}

// ---------- Serverliste ----------

async fn dashboard(State(s): State<AppState>, headers: HeaderMap) -> Response {
    wrap(dashboard_inner(s, headers).await)
}

async fn dashboard_inner(s: AppState, headers: HeaderMap) -> Result<Response, BoxError> {
    let Some(sess) = session(&s, &headers).await? else { return Ok(Redirect::to("/login").into_response()) };
    let guilds = user_guilds(&s, &sess).await?;
    let bot_guilds: Vec<u64> = s.http.get_guilds(None, Some(200)).await?.iter().map(|g| g.id.get()).collect();
    let mut rows = String::new();
    for g in guilds.iter().filter(|g| g.can_manage()) {
        let present = g.id.parse::<u64>().map(|id| bot_guilds.contains(&id)).unwrap_or(false);
        let action = if present {
            format!("<a href=\"/guild/{}\">Verwalten</a>", esc(&g.id))
        } else {
            format!("<a href=\"{}\">Bot einladen</a>", esc(&invite_url(&s.cfg, Some(&g.id))))
        };
        let _ = write!(rows, "<tr><td>{}</td><td>{action}</td></tr>", esc(&g.name));
    }
    let body = format!(
        "<p class=\"muted\">Angemeldet als {} · <a href=\"/logout\">Abmelden</a></p><h2>Deine Server</h2><table>{rows}</table>",
        esc(&sess.username)
    );
    Ok(page("Server", &body).into_response())
}

// ---------- Einstellungen ----------

async fn settings_page(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>) -> Response {
    wrap(settings_page_inner(s, headers, gid).await)
}

async fn settings_page_inner(s: AppState, headers: HeaderMap, gid: i64) -> Result<Response, BoxError> {
    let (_, guild) = match authorize(&s, &headers, gid).await? {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let Ok((text, _cats, roles)) = lists(&s, guild).await else {
        return Ok(page("Bot fehlt", &format!("<p>Der Bot ist nicht auf diesem Server. <a href=\"{}\">Jetzt einladen</a></p>", esc(&invite_url(&s.cfg, Some(&gid.to_string()))))).into_response());
    };
    let st = db::get_settings(&s.pool, gid).await?;
    let body = format!(
        "{nav}<h2>Einstellungen</h2><form method=\"post\">\
<label>Mod-Log-Kanal</label>{modlog}\
<label>Willkommenskanal</label>{welcome}\
<label>Willkommensnachricht ({{user}} wird zur Erwaehnung)</label><input name=\"welcome_message\" value=\"{wmsg}\">\
<label>Auto-Rolle fuer neue Mitglieder</label>{autorole}\
<label><input type=\"checkbox\" name=\"block_invites\" style=\"width:auto\" {inv}> Einladungslinks blockieren</label>\
<label>Spam: max. Nachrichten pro 10 s (0 = aus)</label><input name=\"spam_max_messages\" type=\"number\" min=\"0\" value=\"{spam}\">\
<label>Verbotene Woerter (Komma getrennt)</label><input name=\"banned_words\" value=\"{words}\">\
<label>Warn-Schwellen, z. B. 3:timeout:60,5:kick,7:ban</label><input name=\"warn_thresholds\" value=\"{th}\">\
<button>Speichern</button></form>",
        nav = nav(gid),
        modlog = select("modlog_channel", &text, st.modlog_channel),
        welcome = select("welcome_channel", &text, st.welcome_channel),
        wmsg = esc(st.welcome_message.as_deref().unwrap_or("")),
        autorole = select("autorole_id", &roles, st.autorole_id),
        inv = if st.block_invites { "checked" } else { "" },
        spam = st.spam_max_messages,
        words = esc(&st.words().join(",")),
        th = esc(&st.thresholds_text()),
    );
    Ok(page("Einstellungen", &body).into_response())
}

#[derive(Deserialize)]
struct SettingsForm {
    modlog_channel: String,
    welcome_channel: String,
    welcome_message: String,
    autorole_id: String,
    block_invites: Option<String>,
    spam_max_messages: String,
    banned_words: String,
    warn_thresholds: String,
}

async fn settings_save(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>, Form(f): Form<SettingsForm>) -> Response {
    wrap(settings_save_inner(s, headers, gid, f).await)
}

async fn settings_save_inner(s: AppState, headers: HeaderMap, gid: i64, f: SettingsForm) -> Result<Response, BoxError> {
    if let Err(r) = authorize(&s, &headers, gid).await? {
        return Ok(r);
    }
    let thresholds = match moderation::parse_thresholds(&f.warn_thresholds) {
        Ok(t) => t,
        Err(e) => return Ok((StatusCode::BAD_REQUEST, page("Fehler", &format!("<p>Warn-Schwellen: {}</p>", esc(&e)))).into_response()),
    };
    let words: Vec<String> = f.banned_words.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
    let msg = f.welcome_message.trim();
    let settings = GuildSettings {
        guild_id: gid,
        modlog_channel: parse_id(&f.modlog_channel),
        welcome_channel: parse_id(&f.welcome_channel),
        welcome_message: if msg.is_empty() { None } else { Some(msg.to_string()) },
        autorole_id: parse_id(&f.autorole_id),
        block_invites: f.block_invites.is_some(),
        spam_max_messages: f.spam_max_messages.trim().parse::<i64>().unwrap_or(0).clamp(0, 100),
        banned_words: serde_json::to_string(&words)?,
        warn_thresholds: serde_json::to_string(&thresholds)?,
    };
    db::save_settings(&s.pool, &settings).await?;
    Ok(Redirect::to(&format!("/guild/{gid}")).into_response())
}

// ---------- Tickets ----------

async fn tickets_page(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>) -> Response {
    wrap(tickets_page_inner(s, headers, gid).await)
}

async fn tickets_page_inner(s: AppState, headers: HeaderMap, gid: i64) -> Result<Response, BoxError> {
    let (_, guild) = match authorize(&s, &headers, gid).await? {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let (text, cats, roles) = lists(&s, guild).await?;
    let rows: Vec<(i64, i64, i64, String, String)> = sqlx::query_as(
        "SELECT id, number, opener_id, state, created_at FROM tickets WHERE guild_id = ? ORDER BY id DESC LIMIT 100",
    )
    .bind(gid)
    .fetch_all(&s.pool)
    .await?;
    let mut table = String::new();
    for (id, number, opener, state, created) in rows {
        let _ = write!(table, "<tr><td><a href=\"/guild/{gid}/tickets/{id}\">#{number:04}</a></td><td>{opener}</td><td>{}</td><td>{}</td></tr>", esc(&state), esc(&created));
    }
    let body = format!(
        "{nav}<h2>Ticket-Panel erstellen</h2><form method=\"post\" action=\"/guild/{gid}/tickets/panel\">\
<label>Kanal fuer das Panel</label>{ch}<label>Kategorie fuer neue Tickets</label>{cat}<label>Support-Rolle</label>{role}\
<button>Panel posten</button></form><h2>Tickets</h2><table><tr><th>Nr.</th><th>Ersteller-ID</th><th>Status</th><th>Erstellt</th></tr>{table}</table>",
        nav = nav(gid),
        ch = select("channel", &text, None),
        cat = select("category", &cats, None),
        role = select("support_role", &roles, None),
    );
    Ok(page("Tickets", &body).into_response())
}

#[derive(Deserialize)]
struct PanelForm {
    channel: String,
    category: String,
    support_role: String,
}

async fn ticket_panel(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>, Form(f): Form<PanelForm>) -> Response {
    wrap(ticket_panel_inner(s, headers, gid, f).await)
}

async fn ticket_panel_inner(s: AppState, headers: HeaderMap, gid: i64, f: PanelForm) -> Result<Response, BoxError> {
    let (_, guild) = match authorize(&s, &headers, gid).await? {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let Some(channel) = parse_id(&f.channel) else {
        return Ok((StatusCode::BAD_REQUEST, page("Fehler", "<p>Bitte einen Kanal waehlen.</p>")).into_response());
    };
    bot::post_ticket_panel(&s.pool, &s.http, guild, ChannelId::new(channel as u64), parse_id(&f.category), parse_id(&f.support_role)).await?;
    Ok(Redirect::to(&format!("/guild/{gid}/tickets")).into_response())
}

async fn transcript(State(s): State<AppState>, headers: HeaderMap, Path((gid, tid)): Path<(i64, i64)>) -> Response {
    wrap(transcript_inner(s, headers, gid, tid).await)
}

async fn transcript_inner(s: AppState, headers: HeaderMap, gid: i64, tid: i64) -> Result<Response, BoxError> {
    if let Err(r) = authorize(&s, &headers, gid).await? {
        return Ok(r);
    }
    let owned: Option<i64> = sqlx::query_scalar("SELECT number FROM tickets WHERE id = ? AND guild_id = ?")
        .bind(tid)
        .bind(gid)
        .fetch_optional(&s.pool)
        .await?;
    let Some(number) = owned else { return Ok((StatusCode::NOT_FOUND, page("Nicht gefunden", "<p>Ticket nicht gefunden.</p>")).into_response()) };
    let msgs: Vec<(String, String, String)> = sqlx::query_as("SELECT sent_at, author_name, content FROM ticket_messages WHERE ticket_id = ? ORDER BY id")
        .bind(tid)
        .fetch_all(&s.pool)
        .await?;
    let mut table = String::new();
    for (at, author, content) in msgs {
        let _ = write!(table, "<tr><td class=\"muted\">{}</td><td>{}</td><td>{}</td></tr>", esc(&at), esc(&author), esc(&content));
    }
    Ok(page("Transkript", &format!("{}<h2>Transkript Ticket #{number:04}</h2><table>{table}</table>", nav(gid))).into_response())
}

// ---------- Mod-Log ----------

async fn modlog(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>) -> Response {
    wrap(modlog_inner(s, headers, gid).await)
}

async fn modlog_inner(s: AppState, headers: HeaderMap, gid: i64) -> Result<Response, BoxError> {
    if let Err(r) = authorize(&s, &headers, gid).await? {
        return Ok(r);
    }
    let rows: Vec<(String, i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT created_at, user_id, moderator_id, action, reason FROM mod_actions WHERE guild_id = ? ORDER BY id DESC LIMIT 200",
    )
    .bind(gid)
    .fetch_all(&s.pool)
    .await?;
    let mut table = String::new();
    for (at, user, moderator, action, reason) in rows {
        let _ = write!(table, "<tr><td class=\"muted\">{}</td><td>{}</td><td>{user}</td><td>{moderator}</td><td>{}</td></tr>", esc(&at), esc(&action), esc(reason.as_deref().unwrap_or("")));
    }
    let body = format!("{}<h2>Mod-Log</h2><table><tr><th>Zeit</th><th>Aktion</th><th>Nutzer</th><th>Moderator</th><th>Grund</th></tr>{table}</table>", nav(gid));
    Ok(page("Mod-Log", &body).into_response())
}

// ---------- Server-Designer ----------

async fn designer_list(State(s): State<AppState>, headers: HeaderMap, Path(gid): Path<i64>) -> Response {
    wrap(designer_list_inner(s, headers, gid).await)
}

async fn designer_list_inner(s: AppState, headers: HeaderMap, gid: i64) -> Result<Response, BoxError> {
    if let Err(r) = authorize(&s, &headers, gid).await? {
        return Ok(r);
    }
    let mut cards = String::new();
    for t in template::builtin() {
        let _ = write!(
            cards,
            "<tr><td><b>{}</b><br><span class=\"muted\">{}</span></td><td><a href=\"/guild/{gid}/designer/{}\">Vorschau</a></td></tr>",
            esc(&t.name),
            esc(&t.description),
            urlenc(&t.name)
        );
    }
    let body = format!("{}<h2>Server-Designer</h2><p class=\"muted\">Templates werden additiv angewendet: es wird nur erstellt, was fehlt. Nichts wird geloescht.</p><table>{cards}</table>", nav(gid));
    Ok(page("Server-Designer", &body).into_response())
}

fn find_template(name: &str) -> Option<Template> {
    template::builtin().into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

async fn designer_preview(State(s): State<AppState>, headers: HeaderMap, Path((gid, name)): Path<(i64, String)>) -> Response {
    wrap(designer_preview_inner(s, headers, gid, name).await)
}

async fn designer_preview_inner(s: AppState, headers: HeaderMap, gid: i64, name: String) -> Result<Response, BoxError> {
    let (_, guild) = match authorize(&s, &headers, gid).await? {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    let Some(t) = find_template(&name) else { return Ok((StatusCode::NOT_FOUND, page("Nicht gefunden", "<p>Template unbekannt.</p>")).into_response()) };
    let snap = crate::designer::snapshot(&s.http, guild).await?;
    let plan = template::plan(&t, &snap);
    let list = |title: &str, items: Vec<String>| {
        if items.is_empty() {
            String::new()
        } else {
            format!("<h3>{title}</h3><ul>{}</ul>", items.iter().map(|i| format!("<li>{}</li>", esc(i))).collect::<String>())
        }
    };
    let mut body = format!("{}<h2>{}</h2><p>{}</p>", nav(gid), esc(&t.name), esc(&t.description));
    body.push_str(&list("Rollen, die erstellt werden", plan.create_roles.clone()));
    body.push_str(&list("Kategorien, die erstellt werden", plan.create_categories.clone()));
    body.push_str(&list("Kanaele, die erstellt werden", plan.create_channels.iter().map(|(c, n)| format!("{c} / {n}")).collect()));
    body.push_str(&list("Bereits vorhanden (uebersprungen)", plan.skipped.clone()));
    if plan.is_empty() {
        body.push_str("<p>Nichts zu tun: alles ist bereits vorhanden.</p>");
    } else {
        let _ = write!(
            body,
            "<form method=\"post\"><label><input type=\"checkbox\" name=\"confirm\" value=\"yes\" style=\"width:auto\" required> \
Ich bestaetige: die obigen Eintraege werden auf dem Server erstellt. Bestehendes bleibt unveraendert.</label><button>Template anwenden</button></form>"
        );
    }
    Ok(page("Vorschau", &body).into_response())
}

#[derive(Deserialize)]
struct ApplyForm {
    confirm: Option<String>,
}

async fn designer_apply(State(s): State<AppState>, headers: HeaderMap, Path((gid, name)): Path<(i64, String)>, Form(f): Form<ApplyForm>) -> Response {
    wrap(designer_apply_inner(s, headers, gid, name, f).await)
}

async fn designer_apply_inner(s: AppState, headers: HeaderMap, gid: i64, name: String, f: ApplyForm) -> Result<Response, BoxError> {
    let (_, guild) = match authorize(&s, &headers, gid).await? {
        Ok(v) => v,
        Err(r) => return Ok(r),
    };
    if f.confirm.as_deref() != Some("yes") {
        return Ok((StatusCode::BAD_REQUEST, page("Fehler", "<p>Bestaetigung fehlt.</p>")).into_response());
    }
    let Some(t) = find_template(&name) else { return Ok((StatusCode::NOT_FOUND, page("Nicht gefunden", "<p>Template unbekannt.</p>")).into_response()) };
    let snap = crate::designer::snapshot(&s.http, guild).await?;
    let plan = template::plan(&t, &snap);
    let log = crate::designer::apply(&s.http, guild, &t, &plan).await?;
    let items: String = log.iter().map(|l| format!("<li>{}</li>", esc(l))).collect();
    Ok(page("Angewendet", &format!("{}<h2>Template angewendet</h2><ul>{items}</ul>", nav(gid))).into_response())
}
