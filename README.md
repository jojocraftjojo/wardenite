# Wardenite

Open-Source-Discord-Bot mit Web-Dashboard, geschrieben in Rust. Moderation, Ticketsystem, Server-Designer mit Templates.

**Status: frueh, nicht gegen echtes Discord getestet.** Kompiliert, Clippy sauber, 14 Unit-Tests gruen (CI).

- Bot (poise/serenity): `/ban`, `/kick`, `/timeout`, `/warn` mit Eskalation, `/warns`, `/purge`, `/ticketpanel`, Automod (Einladungen, Woerter, Spam), Mod-Log, Willkommen, Auto-Rolle, Rollen-Buttons (`/rolepanel`), Event-Logging (Beitritt, Austritt, geloeschte Nachrichten), Tickets mit Transkript.
- Dashboard (axum): Discord-OAuth2-Login, Serverliste mit Bot-Einladung, Einstellungen, Ticket-Panel, Transkripte, Mod-Log, Server-Designer mit Vorschau (nur additiv, loescht nie).
- Noch offen: Praxistest mit Discord-App, Hosting auf dem Server.

Der Bot braucht im Developer Portal die Intents **Message Content** und **Server Members**.

Details: [Design](docs/superpowers/specs/2026-11-22-wardenite-design.md).

## Struktur
- `crates/core`: Discord-unabhaengige Logik mit Tests
- `crates/wardenite`: Binary mit Bot, Dashboard und Server-Designer
- `templates/`: Server-Templates (JSON)
- `migrations/`: SQLite-Schema
- `deploy/`: systemd, Caddy/Funnel, SFTP-Deploy-Skript

## Entwicklung
```bash
cp .env.example .env
cargo test --workspace
```

## Deployment (Tailscale + SFTP)
Siehe `deploy/`. Das Dashboard muss oeffentlich per HTTPS erreichbar sein (Bot-Einladung, OAuth2-Redirect): Caddy mit Domain oder Tailscale Funnel.

Lizenz: MIT
