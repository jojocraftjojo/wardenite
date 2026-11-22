//! Server-Designer: wendet ein Template additiv auf einen Discord-Server an.
use crate::config::BoxError;
use poise::serenity_prelude as serenity;
use serenity::{ChannelId, ChannelType, CreateChannel, EditRole, GuildId, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};
use std::collections::HashMap;
use wardenite_core::template::{ChannelKind, GuildSnapshot, Plan, Template};

pub async fn snapshot(http: &serenity::Http, guild: GuildId) -> Result<GuildSnapshot, BoxError> {
    let roles = http.get_guild_roles(guild).await?;
    let channels = http.get_channels(guild).await?;
    let categories = channels
        .iter()
        .filter(|c| c.kind == ChannelType::Category)
        .map(|cat| {
            let children = channels
                .iter()
                .filter(|c| c.parent_id == Some(cat.id))
                .map(|c| c.name.clone())
                .collect();
            (cat.name.clone(), children)
        })
        .collect();
    Ok(GuildSnapshot { roles: roles.iter().map(|r| r.name.clone()).collect(), categories })
}

fn permissions_from(names: &[String]) -> Permissions {
    names
        .iter()
        .filter_map(|n| Permissions::from_name(n))
        .fold(Permissions::empty(), |a, b| a | b)
}

fn overwrites(
    everyone: RoleId,
    visible_to: &[String],
    roles: &HashMap<String, RoleId>,
    read_only: bool,
) -> Vec<PermissionOverwrite> {
    let mut deny = Permissions::empty();
    if !visible_to.is_empty() {
        deny |= Permissions::VIEW_CHANNEL;
    }
    if read_only {
        deny |= Permissions::SEND_MESSAGES;
    }
    let mut out = Vec::new();
    if !deny.is_empty() {
        out.push(PermissionOverwrite { allow: Permissions::empty(), deny, kind: PermissionOverwriteType::Role(everyone) });
    }
    for name in visible_to {
        if let Some(id) = roles.get(&name.to_lowercase()) {
            out.push(PermissionOverwrite {
                allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY,
                deny: Permissions::empty(),
                kind: PermissionOverwriteType::Role(*id),
            });
        }
    }
    out
}

/// Fuehrt den Plan aus (nur Erstellen, nie Loeschen). Liefert ein Protokoll.
pub async fn apply(http: &serenity::Http, guild: GuildId, template: &Template, plan: &Plan) -> Result<Vec<String>, BoxError> {
    let mut log = Vec::new();
    let everyone = RoleId::new(guild.get());

    let mut role_ids: HashMap<String, RoleId> = http
        .get_guild_roles(guild)
        .await?
        .into_iter()
        .map(|r| (r.name.to_lowercase(), r.id))
        .collect();
    for spec in template.roles.iter().filter(|r| plan.create_roles.contains(&r.name)) {
        let mut b = EditRole::new()
            .name(&spec.name)
            .hoist(spec.hoist)
            .mentionable(spec.mentionable)
            .permissions(permissions_from(&spec.permissions));
        if let Some(hex) = &spec.color {
            if let Ok(c) = u32::from_str_radix(hex.trim_start_matches('#'), 16) {
                b = b.colour(c);
            }
        }
        let role = guild.create_role(http, b).await?;
        role_ids.insert(spec.name.to_lowercase(), role.id);
        log.push(format!("Rolle erstellt: {}", spec.name));
    }

    let mut category_ids: HashMap<String, ChannelId> = http
        .get_channels(guild)
        .await?
        .into_iter()
        .filter(|c| c.kind == ChannelType::Category)
        .map(|c| (c.name.to_lowercase(), c.id))
        .collect();
    for cat in &template.categories {
        if plan.create_categories.contains(&cat.name) {
            let b = CreateChannel::new(&cat.name)
                .kind(ChannelType::Category)
                .permissions(overwrites(everyone, &cat.visible_to, &role_ids, false));
            let created = guild.create_channel(http, b).await?;
            category_ids.insert(cat.name.to_lowercase(), created.id);
            log.push(format!("Kategorie erstellt: {}", cat.name));
        }
        let parent = category_ids.get(&cat.name.to_lowercase()).copied();
        for ch in &cat.channels {
            if !plan.create_channels.iter().any(|(c, n)| c == &cat.name && n == &ch.name) {
                continue;
            }
            let kind = match ch.kind {
                ChannelKind::Text => ChannelType::Text,
                ChannelKind::Voice => ChannelType::Voice,
                ChannelKind::Announcement => ChannelType::News,
                ChannelKind::Forum => ChannelType::Forum,
            };
            let mut b = CreateChannel::new(&ch.name)
                .kind(kind)
                .permissions(overwrites(everyone, &cat.visible_to, &role_ids, ch.read_only));
            if let Some(p) = parent {
                b = b.category(p);
            }
            if let (Some(topic), ChannelKind::Text | ChannelKind::Announcement) = (&ch.topic, ch.kind) {
                b = b.topic(topic);
            }
            guild.create_channel(http, b).await?;
            log.push(format!("Kanal erstellt: {}/{}", cat.name, ch.name));
        }
    }
    if log.is_empty() {
        log.push("Nichts zu tun: alles ist bereits vorhanden.".into());
    }
    Ok(log)
}
