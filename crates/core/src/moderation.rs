//! Moderationslogik: Warn-Eskalation und einfache Automod-Pruefungen.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Punishment {
    Timeout { minutes: u32 },
    Kick,
    Ban,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threshold {
    pub warns: u32,
    pub action: Punishment,
}

/// Liefert die Strafe, die bei genau `count` Verwarnungen ausgeloest wird.
pub fn escalation(count: u32, thresholds: &[Threshold]) -> Option<&Punishment> {
    thresholds.iter().find(|t| t.warns == count).map(|t| &t.action)
}

#[derive(Debug, Clone, Default)]
pub struct AutomodConfig {
    pub block_invites: bool,
    pub banned_words: Vec<String>,
    /// Max. Nachrichten pro Fenster, 0 = aus
    pub spam_max_messages: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Violation {
    Invite,
    BannedWord(String),
    Spam,
}

pub fn contains_invite(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("discord.gg/") || t.contains("discord.com/invite/") || t.contains("discordapp.com/invite/")
}

/// `recent_count` = Nachrichten des Users im aktuellen Zeitfenster (inkl. dieser).
pub fn check_message(cfg: &AutomodConfig, text: &str, recent_count: u32) -> Option<Violation> {
    if cfg.block_invites && contains_invite(text) {
        return Some(Violation::Invite);
    }
    let lower = text.to_lowercase();
    for w in &cfg.banned_words {
        let w = w.to_lowercase();
        if !w.is_empty() && lower.split(|c: char| !c.is_alphanumeric()).any(|tok| tok == w) {
            return Some(Violation::BannedWord(w));
        }
    }
    if cfg.spam_max_messages > 0 && recent_count > cfg.spam_max_messages {
        return Some(Violation::Spam);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn th() -> Vec<Threshold> {
        vec![
            Threshold { warns: 3, action: Punishment::Timeout { minutes: 60 } },
            Threshold { warns: 5, action: Punishment::Kick },
            Threshold { warns: 7, action: Punishment::Ban },
        ]
    }

    #[test]
    fn escalation_hits_exact_thresholds() {
        let t = th();
        assert_eq!(escalation(2, &t), None);
        assert_eq!(escalation(3, &t), Some(&Punishment::Timeout { minutes: 60 }));
        assert_eq!(escalation(7, &t), Some(&Punishment::Ban));
        assert_eq!(escalation(8, &t), None);
    }

    #[test]
    fn detects_invites() {
        assert!(contains_invite("join https://Discord.gg/abc"));
        assert!(!contains_invite("discord is nice"));
    }

    #[test]
    fn banned_words_match_whole_words_only() {
        let cfg = AutomodConfig { banned_words: vec!["ass".into()], ..Default::default() };
        assert_eq!(check_message(&cfg, "class is fun", 1), None);
        assert_eq!(check_message(&cfg, "you ass!", 1), Some(Violation::BannedWord("ass".into())));
    }

    #[test]
    fn spam_threshold() {
        let cfg = AutomodConfig { spam_max_messages: 5, ..Default::default() };
        assert_eq!(check_message(&cfg, "hi", 5), None);
        assert_eq!(check_message(&cfg, "hi", 6), Some(Violation::Spam));
    }
}
