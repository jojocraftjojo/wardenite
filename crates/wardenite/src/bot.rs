//! Discord-Bot: Slash-Commands, Automod, Tickets, Willkommen/Autorole.
use crate::config::BoxError;
use crate::db;
use poise::serenity_prelude as serenity;
use serenity::{
    ButtonStyle, ChannelId, ChannelType, CreateActionRow, CreateButton, CreateChannel,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, GuildId,
    PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};
use sqlx::SqlitePool;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use wardenite_core::moderation::{self, Punishment, Violation};
use wardenite_core::ticket::{channel_name, TicketError, TicketState};

type Error = BoxError;
type Context<'a> = poise::Context<'a, Data, Error>;
/// (id, state, claimed_by, closed_by, opener_id)
type TicketRow = (i64, String, Option<i64>, Option<i64>, i64);

pub struct Data {
    pub pool: SqlitePool,
    spam: Mutex<HashMap<(u64, u64), VecDeque<Instant>>>,
}

const SPAM_WINDOW: Duration = Duration::from_secs(10);

/// Schreibt die Aktion in die DB und – falls konfiguriert – in den Mod-Log-Kanal.
pub async fn record_action(
    pool: &SqlitePool,
    http: &serenity::Http,
    guild: GuildId,
    user: UserId,
    moderator: UserId,
    action: &str,
    reason: Option<&str>,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO mod_actions (guild_id, user_id, moderator_id, action, reason) VALUES (?,?,?,?,?)")
        .bind(guild.get() as i64)
        .bind(user.get() as i64)
        .bind(moderator.get() as i64)
        .bind(action)
        .bind(reason)
        .execute(pool)
        .await?;
    let settings = db::get_settings(pool, guild.get() as i64).await?;
    if let Some(ch) = settings.modlog_channel {
        let text = format!(
            "**{action}** · <@{}> durch <@{}> · {}",
            user.get(),
            moderator.get(),
            reason.unwrap_or("kein Grund")
        );
        let _ = ChannelId::new(ch as u64).say(http, text).await;
    }
    Ok(())
}

async fn apply_punishment(
    http: &serenity::Http,
    guild: GuildId,
    user: UserId,
    punishment: &Punishment,
    reason: &str,
) -> Result<(), Error> {
    match punishment {
        Punishment::Kick => guild.kick_with_reason(http, user, reason).await?,
        Punishment::Ban => guild.ban_with_reason(http, user, 0, reason).await?,
        Punishment::Timeout { minutes } => {
            let until = serenity::Timestamp::from_unix_timestamp(
                serenity::Timestamp::now().unix_timestamp() + i64::from(*minutes) * 60,
            )?;
            guild
                .edit_member(
                    http,
                    user,
                    serenity::EditMember::new().disable_communication_until_datetime(until),
                )
                .await?;
        }
    }
    Ok(())
}

/// Slash-Command: Nutzer bannen.
#[poise::command(slash_command, guild_only, required_permissions = "BAN_MEMBERS")]
pub async fn ban(
    ctx: Context<'_>,
    user: serenity::User,
    reason: Option<String>,
) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let why = reason.as_deref().unwrap_or("kein Grund");
    apply_punishment(ctx.http(), guild, user.id, &Punishment::Ban, why).await?;
    record_action(
        &ctx.data().pool,
        ctx.http(),
        guild,
        user.id,
        ctx.author().id,
        "ban",
        reason.as_deref(),
    )
    .await?;
    ctx.say(format!("{} wurde gebannt.", user.tag())).await?;
    Ok(())
}

/// Slash-Command: Nutzer kicken.
#[poise::command(slash_command, guild_only, required_permissions = "KICK_MEMBERS")]
pub async fn kick(
    ctx: Context<'_>,
    user: serenity::User,
    reason: Option<String>,
) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let why = reason.as_deref().unwrap_or("kein Grund");
    apply_punishment(ctx.http(), guild, user.id, &Punishment::Kick, why).await?;
    record_action(
        &ctx.data().pool,
        ctx.http(),
        guild,
        user.id,
        ctx.author().id,
        "kick",
        reason.as_deref(),
    )
    .await?;
    ctx.say(format!("{} wurde gekickt.", user.tag())).await?;
    Ok(())
}

/// Slash-Command: Nutzer fuer X Minuten stummschalten.
#[poise::command(slash_command, guild_only, required_permissions = "MODERATE_MEMBERS")]
pub async fn timeout(
    ctx: Context<'_>,
    user: serenity::User,
    minutes: u32,
    reason: Option<String>,
) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let why = reason.as_deref().unwrap_or("kein Grund");
    apply_punishment(
        ctx.http(),
        guild,
        user.id,
        &Punishment::Timeout { minutes },
        why,
    )
    .await?;
    record_action(
        &ctx.data().pool,
        ctx.http(),
        guild,
        user.id,
        ctx.author().id,
        "timeout",
        reason.as_deref(),
    )
    .await?;
    ctx.say(format!(
        "{} ist {minutes} Minuten stummgeschaltet.",
        user.tag()
    ))
    .await?;
    Ok(())
}

/// Slash-Command: Verwarnung, loest ggf. automatische Strafe aus.
#[poise::command(slash_command, guild_only, required_permissions = "MODERATE_MEMBERS")]
pub async fn warn(
    ctx: Context<'_>,
    user: serenity::User,
    reason: Option<String>,
) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let pool = &ctx.data().pool;
    sqlx::query("INSERT INTO warns (guild_id, user_id, moderator_id, reason) VALUES (?,?,?,?)")
        .bind(guild.get() as i64)
        .bind(user.id.get() as i64)
        .bind(ctx.author().id.get() as i64)
        .bind(&reason)
        .execute(pool)
        .await?;
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM warns WHERE guild_id = ? AND user_id = ?")
            .bind(guild.get() as i64)
            .bind(user.id.get() as i64)
            .fetch_one(pool)
            .await?;
    record_action(
        pool,
        ctx.http(),
        guild,
        user.id,
        ctx.author().id,
        "warn",
        reason.as_deref(),
    )
    .await?;
    let settings = db::get_settings(pool, guild.get() as i64).await?;
    let thresholds = settings.thresholds();
    let mut msg = format!("{} verwarnt (Verwarnung Nr. {count}).", user.tag());
    if let Some(p) = moderation::escalation(count as u32, &thresholds) {
        apply_punishment(ctx.http(), guild, user.id, p, "Warn-Schwelle erreicht").await?;
        msg.push_str(&format!(" Automatische Strafe: {p:?}."));
    }
    ctx.say(msg).await?;
    Ok(())
}

/// Slash-Command: Verwarnungen eines Nutzers anzeigen.
#[poise::command(slash_command, guild_only, required_permissions = "MODERATE_MEMBERS")]
pub async fn warns(ctx: Context<'_>, user: serenity::User) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT created_at, reason FROM warns WHERE guild_id = ? AND user_id = ? ORDER BY id DESC LIMIT 10",
    )
    .bind(guild.get() as i64)
    .bind(user.id.get() as i64)
    .fetch_all(&ctx.data().pool)
    .await?;
    if rows.is_empty() {
        ctx.say("Keine Verwarnungen.").await?;
    } else {
        let lines: Vec<String> = rows
            .iter()
            .map(|(at, r)| format!("{at}: {}", r.as_deref().unwrap_or("kein Grund")))
            .collect();
        ctx.say(lines.join("\n")).await?;
    }
    Ok(())
}

/// Slash-Command: Die letzten N Nachrichten loeschen (max. 100).
#[poise::command(slash_command, guild_only, required_permissions = "MANAGE_MESSAGES")]
pub async fn purge(ctx: Context<'_>, count: u8) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    let limit = count.clamp(1, 100);
    let channel = ctx.channel_id();
    let msgs = channel
        .messages(ctx.http(), serenity::GetMessages::new().limit(limit))
        .await?;
    let ids: Vec<_> = msgs.iter().map(|m| m.id).collect();
    if ids.len() >= 2 {
        channel.delete_messages(ctx.http(), ids.clone()).await?;
    } else if let Some(id) = ids.first() {
        channel.delete_message(ctx.http(), *id).await?;
    }
    record_action(
        &ctx.data().pool,
        ctx.http(),
        guild,
        ctx.author().id,
        ctx.author().id,
        "purge",
        Some(&format!("{} Nachrichten", ids.len())),
    )
    .await?;
    ctx.say(format!("{} Nachrichten geloescht.", ids.len()))
        .await?;
    Ok(())
}

/// Slash-Command: Ticket-Panel in diesem Kanal posten.
#[poise::command(slash_command, guild_only, required_permissions = "MANAGE_GUILD")]
pub async fn ticketpanel(
    ctx: Context<'_>,
    support_role: Option<serenity::Role>,
    category: Option<serenity::GuildChannel>,
) -> Result<(), Error> {
    let guild = ctx.guild_id().ok_or("nur auf Servern")?;
    post_ticket_panel(
        &ctx.data().pool,
        ctx.http(),
        guild,
        ctx.channel_id(),
        category.map(|c| c.id.get() as i64),
        support_role.map(|r| r.id.get() as i64),
    )
    .await?;
    ctx.say("Panel erstellt.").await?;
    Ok(())
}

/// Postet das Ticket-Panel (wird auch vom Dashboard genutzt).
pub async fn post_ticket_panel(
    pool: &SqlitePool,
    http: &serenity::Http,
    guild: GuildId,
    channel: ChannelId,
    category: Option<i64>,
    support_role: Option<i64>,
) -> Result<(), Error> {
    let msg = CreateMessage::new()
        .content("**Support**\nKlicke auf den Button, um ein Ticket zu erstellen.")
        .components(vec![CreateActionRow::Buttons(vec![CreateButton::new(
            "ticket_open",
        )
        .label("Ticket erstellen")
        .style(ButtonStyle::Primary)])]);
    let sent = channel.send_message(http, msg).await?;
    sqlx::query("INSERT INTO ticket_panels (guild_id, channel_id, message_id, category_id, support_role_id) VALUES (?,?,?,?,?)")
        .bind(guild.get() as i64)
        .bind(channel.get() as i64)
        .bind(sent.id.get() as i64)
        .bind(category)
        .bind(support_role)
        .execute(pool)
        .await?;
    Ok(())
}

/// Slash-Command: Button-Panel, ueber das sich Mitglieder eine Rolle selbst geben/nehmen.
#[poise::command(slash_command, guild_only, required_permissions = "MANAGE_ROLES")]
pub async fn rolepanel(
    ctx: Context<'_>,
    role: serenity::Role,
    label: Option<String>,
) -> Result<(), Error> {
    if role.permissions.contains(Permissions::ADMINISTRATOR) {
        ctx.say("Rollen mit Administrator-Recht koennen nicht per Button vergeben werden.")
            .await?;
        return Ok(());
    }
    let text = label.unwrap_or_else(|| role.name.clone());
    let msg = CreateMessage::new()
        .content(format!(
            "Klicke, um die Rolle **{}** zu erhalten oder zu entfernen.",
            role.name
        ))
        .components(vec![CreateActionRow::Buttons(vec![CreateButton::new(
            format!("role_{}", role.id.get()),
        )
        .label(text)
        .style(ButtonStyle::Secondary)])]);
    ctx.channel_id().send_message(ctx.http(), msg).await?;
    ctx.say("Rollen-Panel erstellt.").await?;
    Ok(())
}

/// Schreibt ein Ereignis in den Mod-Log-Kanal (falls konfiguriert).
async fn log_event(
    pool: &SqlitePool,
    http: &serenity::Http,
    guild: GuildId,
    text: String,
) -> Result<(), Error> {
    let settings = db::get_settings(pool, guild.get() as i64).await?;
    if let Some(ch) = settings.modlog_channel {
        let _ = ChannelId::new(ch as u64).say(http, text).await;
    }
    Ok(())
}

pub fn commands() -> Vec<poise::Command<Data, Error>> {
    vec![
        ban(),
        kick(),
        timeout(),
        warn(),
        warns(),
        purge(),
        ticketpanel(),
        rolepanel(),
    ]
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    data: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Message { new_message } => on_message(ctx, new_message, data).await?,
        serenity::FullEvent::GuildMemberAddition { new_member } => {
            on_member_join(ctx, new_member, data).await?
        }
        serenity::FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
            log_event(
                &data.pool,
                &ctx.http,
                *guild_id,
                format!(
                    "Mitglied hat den Server verlassen: {} (<@{}>)",
                    user.name,
                    user.id.get()
                ),
            )
            .await?;
        }
        serenity::FullEvent::MessageDelete {
            channel_id,
            guild_id: Some(guild_id),
            ..
        } => {
            log_event(
                &data.pool,
                &ctx.http,
                *guild_id,
                format!("Nachricht geloescht in <#{}>", channel_id.get()),
            )
            .await?;
        }
        serenity::FullEvent::InteractionCreate {
            interaction: serenity::Interaction::Component(c),
        } => {
            on_component(ctx, c, data).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn on_message(
    ctx: &serenity::Context,
    msg: &serenity::Message,
    data: &Data,
) -> Result<(), Error> {
    if msg.author.bot {
        return Ok(());
    }
    let Some(guild) = msg.guild_id else {
        return Ok(());
    };
    let pool = &data.pool;

    // Ticket-Transkript: Nachrichten in offenen Ticket-Kanaelen mitschreiben.
    let ticket: Option<i64> =
        sqlx::query_scalar("SELECT id FROM tickets WHERE channel_id = ? AND state != 'closed'")
            .bind(msg.channel_id.get() as i64)
            .fetch_optional(pool)
            .await?;
    if let Some(tid) = ticket {
        sqlx::query("INSERT INTO ticket_messages (ticket_id, author_id, author_name, content, sent_at) VALUES (?,?,?,?,?)")
            .bind(tid)
            .bind(msg.author.id.get() as i64)
            .bind(&msg.author.name)
            .bind(&msg.content)
            .bind(msg.timestamp.to_string())
            .execute(pool)
            .await?;
    }

    // Automod (Teammitglieder mit "Nachrichten verwalten" sind ausgenommen).
    let exempt = msg
        .member
        .as_ref()
        .and_then(|m| m.permissions)
        .is_some_and(|p| {
            p.contains(Permissions::MANAGE_MESSAGES) || p.contains(Permissions::ADMINISTRATOR)
        });
    if exempt {
        return Ok(());
    }
    let settings = db::get_settings(pool, guild.get() as i64).await?;
    let recent = {
        let mut map = data.spam.lock().map_err(|_| "spam-lock vergiftet")?;
        let q = map.entry((guild.get(), msg.author.id.get())).or_default();
        let now = Instant::now();
        q.push_back(now);
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) > SPAM_WINDOW)
        {
            q.pop_front();
        }
        q.len() as u32
    };
    if let Some(v) = moderation::check_message(&settings.automod(), &msg.content, recent) {
        let _ = msg.delete(&ctx.http).await;
        let label = match v {
            Violation::Invite => "Einladungslinks sind hier nicht erlaubt.",
            Violation::BannedWord(_) => "Diese Nachricht enthaelt ein verbotenes Wort.",
            Violation::Spam => "Bitte nicht spammen.",
        };
        let _ = msg
            .channel_id
            .say(&ctx.http, format!("<@{}> {label}", msg.author.id.get()))
            .await;
        let bot_id = ctx.cache.current_user().id;
        record_action(
            pool,
            &ctx.http,
            guild,
            msg.author.id,
            bot_id,
            "automod",
            Some(label),
        )
        .await?;
    }
    Ok(())
}

async fn on_member_join(
    ctx: &serenity::Context,
    member: &serenity::Member,
    data: &Data,
) -> Result<(), Error> {
    let settings = db::get_settings(&data.pool, member.guild_id.get() as i64).await?;
    log_event(
        &data.pool,
        &ctx.http,
        member.guild_id,
        format!("Mitglied beigetreten: <@{}>", member.user.id.get()),
    )
    .await?;
    if let Some(role) = settings.autorole_id {
        let _ = member.add_role(&ctx.http, RoleId::new(role as u64)).await;
    }
    if let Some(ch) = settings.welcome_channel {
        let template = settings
            .welcome_message
            .unwrap_or_else(|| "Willkommen {user}!".into());
        let text = template.replace("{user}", &format!("<@{}>", member.user.id.get()));
        let _ = ChannelId::new(ch as u64).say(&ctx.http, text).await;
    }
    Ok(())
}

async fn respond(
    ctx: &serenity::Context,
    c: &serenity::ComponentInteraction,
    text: &str,
) -> Result<(), Error> {
    c.create_response(
        &ctx.http,
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .content(text)
                .ephemeral(true),
        ),
    )
    .await?;
    Ok(())
}

fn state_from_row(state: &str, claimed_by: Option<i64>, closed_by: Option<i64>) -> TicketState {
    match state {
        "claimed" => TicketState::Claimed {
            by: claimed_by.unwrap_or(0) as u64,
        },
        "closed" => TicketState::Closed {
            by: closed_by.unwrap_or(0) as u64,
        },
        _ => TicketState::Open,
    }
}

async fn is_staff(
    pool: &SqlitePool,
    c: &serenity::ComponentInteraction,
    guild: GuildId,
) -> Result<bool, Error> {
    let Some(member) = &c.member else {
        return Ok(false);
    };
    if member.permissions.is_some_and(|p| {
        p.contains(Permissions::MANAGE_MESSAGES) || p.contains(Permissions::ADMINISTRATOR)
    }) {
        return Ok(true);
    }
    let role: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT support_role_id FROM ticket_panels WHERE guild_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(guild.get() as i64)
    .fetch_optional(pool)
    .await?;
    Ok(role
        .flatten()
        .is_some_and(|r| member.roles.contains(&RoleId::new(r as u64))))
}

async fn on_component(
    ctx: &serenity::Context,
    c: &serenity::ComponentInteraction,
    data: &Data,
) -> Result<(), Error> {
    let Some(guild) = c.guild_id else {
        return Ok(());
    };
    let pool = &data.pool;
    if let Some(id) = c
        .data
        .custom_id
        .strip_prefix("role_")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
    {
        let Some(member) = &c.member else {
            return Ok(());
        };
        let role = RoleId::new(id);
        let result = if member.roles.contains(&role) {
            member
                .remove_role(&ctx.http, role)
                .await
                .map(|_| "Rolle entfernt.")
        } else {
            member
                .add_role(&ctx.http, role)
                .await
                .map(|_| "Rolle hinzugefuegt.")
        };
        return match result {
            Ok(msg) => respond(ctx, c, msg).await,
            Err(_) => {
                respond(
                    ctx,
                    c,
                    "Die Rolle konnte nicht vergeben werden (Rollen-Hierarchie des Bots pruefen).",
                )
                .await
            }
        };
    }
    match c.data.custom_id.as_str() {
        "ticket_open" => {
            let open: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tickets WHERE guild_id = ? AND opener_id = ? AND state != 'closed'")
                .bind(guild.get() as i64)
                .bind(c.user.id.get() as i64)
                .fetch_one(pool)
                .await?;
            if open > 0 {
                return respond(ctx, c, "Du hast bereits ein offenes Ticket.").await;
            }
            let panel: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
                "SELECT category_id, support_role_id FROM ticket_panels WHERE guild_id = ? ORDER BY id DESC LIMIT 1",
            )
            .bind(guild.get() as i64)
            .fetch_optional(pool)
            .await?;
            let (category, support_role) = panel.unwrap_or((None, None));
            let number: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(number), 0) + 1 FROM tickets WHERE guild_id = ?",
            )
            .bind(guild.get() as i64)
            .fetch_one(pool)
            .await?;

            let view = Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::READ_MESSAGE_HISTORY;
            let mut overwrites = vec![
                PermissionOverwrite {
                    allow: Permissions::empty(),
                    deny: Permissions::VIEW_CHANNEL,
                    kind: PermissionOverwriteType::Role(RoleId::new(guild.get())),
                },
                PermissionOverwrite {
                    allow: view,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(c.user.id),
                },
                PermissionOverwrite {
                    allow: view,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(ctx.cache.current_user().id),
                },
            ];
            if let Some(r) = support_role {
                overwrites.push(PermissionOverwrite {
                    allow: view,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Role(RoleId::new(r as u64)),
                });
            }
            let mut builder = CreateChannel::new(channel_name(number as u32))
                .kind(ChannelType::Text)
                .permissions(overwrites);
            if let Some(cat) = category {
                builder = builder.category(ChannelId::new(cat as u64));
            }
            let channel = guild.create_channel(&ctx.http, builder).await?;
            sqlx::query(
                "INSERT INTO tickets (guild_id, number, channel_id, opener_id) VALUES (?,?,?,?)",
            )
            .bind(guild.get() as i64)
            .bind(number)
            .bind(channel.id.get() as i64)
            .bind(c.user.id.get() as i64)
            .execute(pool)
            .await?;
            channel
                .id
                .send_message(
                    &ctx.http,
                    CreateMessage::new()
                        .content(format!(
                            "<@{}> willkommen! Beschreibe dein Anliegen, das Team meldet sich.",
                            c.user.id.get()
                        ))
                        .components(vec![CreateActionRow::Buttons(vec![
                            CreateButton::new("ticket_claim")
                                .label("Uebernehmen")
                                .style(ButtonStyle::Success),
                            CreateButton::new("ticket_close")
                                .label("Schliessen")
                                .style(ButtonStyle::Danger),
                        ])]),
                )
                .await?;
            respond(ctx, c, &format!("Ticket erstellt: <#{}>", channel.id.get())).await
        }
        "ticket_claim" | "ticket_close" => {
            let row: Option<TicketRow> = sqlx::query_as(
                "SELECT id, state, claimed_by, closed_by, opener_id FROM tickets WHERE channel_id = ?",
            )
            .bind(c.channel_id.get() as i64)
            .fetch_optional(pool)
            .await?;
            let Some((id, state, claimed_by, closed_by, opener)) = row else {
                return respond(ctx, c, "Das ist kein Ticket-Kanal.").await;
            };
            let current = state_from_row(&state, claimed_by, closed_by);
            let staff = is_staff(pool, c, guild).await?;
            let user = c.user.id.get();
            if c.data.custom_id == "ticket_claim" {
                if !staff {
                    return respond(ctx, c, "Nur das Team kann Tickets uebernehmen.").await;
                }
                match current.claim(user) {
                    Ok(_) => {
                        sqlx::query(
                            "UPDATE tickets SET state = 'claimed', claimed_by = ? WHERE id = ?",
                        )
                        .bind(user as i64)
                        .bind(id)
                        .execute(pool)
                        .await?;
                        respond(ctx, c, "Ticket uebernommen.").await?;
                        c.channel_id
                            .say(&ctx.http, format!("<@{user}> hat das Ticket uebernommen."))
                            .await?;
                        Ok(())
                    }
                    Err(
                        e @ (TicketError::AlreadyClaimed(_)
                        | TicketError::AlreadyClosed
                        | TicketError::NotClaimed),
                    ) => respond(ctx, c, &e.to_string()).await,
                }
            } else {
                if !staff && opener as u64 != user {
                    return respond(ctx, c, "Nur Ticket-Ersteller oder Team koennen schliessen.")
                        .await;
                }
                match current.close(user) {
                    Ok(_) => {
                        sqlx::query("UPDATE tickets SET state = 'closed', closed_by = ?, closed_at = datetime('now') WHERE id = ?")
                            .bind(user as i64)
                            .bind(id)
                            .execute(pool)
                            .await?;
                        respond(ctx, c, "Ticket wird in 5 Sekunden geschlossen. Das Transkript steht im Dashboard.").await?;
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        c.channel_id.delete(&ctx.http).await?;
                        Ok(())
                    }
                    Err(e) => respond(ctx, c, &e.to_string()).await,
                }
            }
        }
        _ => Ok(()),
    }
}

/// Startet den Bot (blockiert bis zum Ende).
pub async fn run(token: String, pool: SqlitePool) -> Result<(), Error> {
    let intents = serenity::GatewayIntents::GUILDS
        | serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MEMBERS;
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands(),
            event_handler: |ctx, event, _framework, data| Box::pin(event_handler(ctx, event, data)),
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    pool,
                    spam: Mutex::new(HashMap::new()),
                })
            })
        })
        .build();
    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;
    client.start().await?;
    Ok(())
}
