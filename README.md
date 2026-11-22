# Wardenite

Open-Source-Discord-Bot mit Web-Dashboard, geschrieben in Rust. Moderation, Ticketsystem, Server-Designer mit Templates.

**Status: frueh.** Vorhanden und getestet (Unit-Tests): Template-Schema mit additivem Diff, Warn-Eskalation und Automod-Pruefungen, Ticket-Zustandsmaschine, DB-Schema, Deploy-Dateien. Noch **nicht** vorhanden: Bot-Anbindung an Discord (poise/serenity), Dashboard (axum), OAuth2-Login.

Details: [Design](docs/superpowers/specs/2026-11-22-wardenite-design.md).

## Struktur
- `crates/core`: Discord-unabhaengige Logik mit Tests
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
