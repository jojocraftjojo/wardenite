//! Server-Templates: Schema, Parsing, Validierung und additiver Diff.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TemplateError {
    #[error("ungueltiges JSON: {0}")]
    Parse(String),
    #[error("Template hat keinen Namen")]
    EmptyName,
    #[error("Kanal oder Kategorie referenziert unbekannte Rolle: {0}")]
    UnknownRole(String),
    #[error("doppelter Name: {0}")]
    Duplicate(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Template {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub roles: Vec<RoleSpec>,
    #[serde(default)]
    pub categories: Vec<CategorySpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoleSpec {
    pub name: String,
    /// RGB als Hex ohne '#', z. B. "5865F2"
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub hoist: bool,
    #[serde(default)]
    pub mentionable: bool,
    /// Discord-Permission-Namen, z. B. "KICK_MEMBERS"
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategorySpec {
    pub name: String,
    /// Nur diese Rollen sehen die Kategorie. Leer = oeffentlich.
    #[serde(default)]
    pub visible_to: Vec<String>,
    #[serde(default)]
    pub channels: Vec<ChannelSpec>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Text,
    Voice,
    Announcement,
    Forum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSpec {
    pub name: String,
    pub kind: ChannelKind,
    #[serde(default)]
    pub topic: Option<String>,
    /// Nur Lesen fuer @everyone (z. B. Regeln, Ankuendigungen).
    #[serde(default)]
    pub read_only: bool,
}

impl Template {
    pub fn from_json(s: &str) -> Result<Self, TemplateError> {
        let t: Template =
            serde_json::from_str(s).map_err(|e| TemplateError::Parse(e.to_string()))?;
        t.validate()?;
        Ok(t)
    }

    pub fn validate(&self) -> Result<(), TemplateError> {
        if self.name.trim().is_empty() {
            return Err(TemplateError::EmptyName);
        }
        let mut roles = HashSet::new();
        for r in &self.roles {
            if !roles.insert(r.name.to_lowercase()) {
                return Err(TemplateError::Duplicate(r.name.clone()));
            }
        }
        let mut cats = HashSet::new();
        for c in &self.categories {
            if !cats.insert(c.name.to_lowercase()) {
                return Err(TemplateError::Duplicate(c.name.clone()));
            }
            for role in &c.visible_to {
                if role != "@everyone" && !roles.contains(&role.to_lowercase()) {
                    return Err(TemplateError::UnknownRole(role.clone()));
                }
            }
        }
        Ok(())
    }
}

/// Zustand des Servers, so wie der Bot ihn von Discord liest.
#[derive(Debug, Clone, Default)]
pub struct GuildSnapshot {
    pub roles: Vec<String>,
    pub categories: Vec<(String, Vec<String>)>,
}

/// Additiver Plan: erstellt nur, was fehlt. Loescht nie etwas.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub create_roles: Vec<String>,
    pub create_categories: Vec<String>,
    /// (Kategorie, Kanalname)
    pub create_channels: Vec<(String, String)>,
    pub skipped: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.create_roles.is_empty()
            && self.create_categories.is_empty()
            && self.create_channels.is_empty()
    }
}

pub fn plan(template: &Template, guild: &GuildSnapshot) -> Plan {
    let norm = |s: &str| s.to_lowercase();
    let have_roles: HashSet<String> = guild.roles.iter().map(|r| norm(r)).collect();
    let mut out = Plan::default();

    for r in &template.roles {
        if have_roles.contains(&norm(&r.name)) {
            out.skipped.push(format!("Rolle {}", r.name));
        } else {
            out.create_roles.push(r.name.clone());
        }
    }
    for cat in &template.categories {
        let existing = guild
            .categories
            .iter()
            .find(|(n, _)| norm(n) == norm(&cat.name));
        let have_channels: HashSet<String> = existing
            .map(|(_, ch)| ch.iter().map(|c| norm(c)).collect())
            .unwrap_or_default();
        if existing.is_none() {
            out.create_categories.push(cat.name.clone());
        } else {
            out.skipped.push(format!("Kategorie {}", cat.name));
        }
        for ch in &cat.channels {
            if have_channels.contains(&norm(&ch.name)) {
                out.skipped.push(format!("Kanal {}/{}", cat.name, ch.name));
            } else {
                out.create_channels
                    .push((cat.name.clone(), ch.name.clone()));
            }
        }
    }
    out
}

/// Eingebaute Templates (zur Compile-Zeit eingebettet).
pub fn builtin() -> Vec<Template> {
    [
        include_str!("../../../templates/community.json"),
        include_str!("../../../templates/gaming.json"),
        include_str!("../../../templates/support.json"),
    ]
    .iter()
    .map(|s| Template::from_json(s).expect("eingebettetes Template ist gueltig"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_templates_are_valid() {
        let all = builtin();
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|t| !t.categories.is_empty()));
    }

    #[test]
    fn rejects_unknown_role() {
        let j =
            r#"{"name":"x","description":"","categories":[{"name":"A","visible_to":["Ghost"]}]}"#;
        assert_eq!(
            Template::from_json(j),
            Err(TemplateError::UnknownRole("Ghost".into()))
        );
    }

    #[test]
    fn rejects_duplicate_roles() {
        let j = r#"{"name":"x","description":"","roles":[{"name":"A"},{"name":"a"}]}"#;
        assert!(matches!(
            Template::from_json(j),
            Err(TemplateError::Duplicate(_))
        ));
    }

    #[test]
    fn plan_is_additive_and_idempotent() {
        let t = &builtin()[0];
        let empty = plan(t, &GuildSnapshot::default());
        assert!(!empty.is_empty());
        // Nach "Anwendung" erzeugt ein zweiter Plan nichts mehr.
        let applied = GuildSnapshot {
            roles: t.roles.iter().map(|r| r.name.clone()).collect(),
            categories: t
                .categories
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        c.channels.iter().map(|ch| ch.name.clone()).collect(),
                    )
                })
                .collect(),
        };
        assert!(plan(t, &applied).is_empty());
    }

    #[test]
    fn plan_skips_existing_case_insensitively() {
        let t = Template::from_json(
            r#"{"name":"x","description":"","roles":[{"name":"Mod"}],"categories":[{"name":"Info","channels":[{"name":"regeln","kind":"text"}]}]}"#,
        )
        .unwrap();
        let g = GuildSnapshot {
            roles: vec!["mod".into()],
            categories: vec![("INFO".into(), vec![])],
        };
        let p = plan(&t, &g);
        assert!(p.create_roles.is_empty() && p.create_categories.is_empty());
        assert_eq!(
            p.create_channels,
            vec![("Info".to_string(), "regeln".to_string())]
        );
    }
}
