# Wardenite – Design (Entwurf, wartet auf Freigabe)

## Ziel
Open-Source-Discord-Bot in Rust mit Web-Dashboard. Funktionsumfang orientiert an Galaxy Bot, Sapphire und Ticket Tool. Nutzer laden den Bot über das Dashboard ein und verwalten alles dort: Moderation, Ticketsystem, Server-Designer mit Templates.

## Nicht-Ziele (vorerst)
- Musik, Economy und Leveling-Spiele.
- Bezahlmodell oder Premium-Stufen.

## Annahmen (bitte bestätigen oder korrigieren)
- Der Bot ist öffentlich und mandantenfähig: ein Bot-Prozess bedient viele Server.
- Lizenz MIT, Repo öffentlich: https://github.com/jojocraftjojo/wardenite.
- Der Name ist vorläufig (Alternativen: Cordforge, Oxidia, Shellguard). Umbenennen ist mit `gh repo rename` möglich.

## Ansätze
1. **Ein Cargo-Workspace, ein Binary (Bot + API zusammen), SvelteKit-Frontend als statischer Build.** Einfachstes Deployment (eine Datei plus Static-Ordner per SFTP), gemeinsamer Zustand ohne IPC. Nachteil: Bot und Dashboard starten und crashen gemeinsam.
2. **Workspace mit zwei Binaries (`bot`, `api`) über gemeinsame Datenbank.** Sauber getrennt, unabhängig neustartbar. Nachteil: mehr Deployment-Aufwand, Events zwischen Bot und API brauchen Postgres LISTEN/NOTIFY oder Redis.
3. **Leptos (Rust-Fullstack) statt SvelteKit.** Eine Sprache im ganzen Projekt. Nachteil: kleineres Ökosystem, langsamere Builds, weniger UI-Bausteine.

**Empfehlung: Ansatz 2.** Er skaliert besser und entspricht dem Betrieb bei großen Bots. Beim Start nutzen wir PostgreSQL für alles inklusive Events. Falls du Einfachheit vorziehst, ist Ansatz 1 die Alternative.

## Technik (meine Entscheidungen)
- Bot: `poise` auf `serenity`. API: `axum`. Datenbank: PostgreSQL mit `sqlx`. Login: Discord OAuth2.
- Dashboard: SvelteKit mit Tailwind, als statischer Build vom API-Server ausgeliefert.
- Templates für den Server-Designer: YAML/JSON-Dateien im Repo (Kategorien, Kanäle, Rollen, Berechtigungen). Der Designer wendet sie über die Discord-API an und zeigt vorher eine Vorschau mit Diff.
- Der Designer-Assistent (Server "komplett designen lassen") nutzt zuerst nur Templates. KI-Generierung ist eine spätere Option.

## Deployment
- Linux-Server, Zugriff nur über Tailscale (SSH/SFTP). Binaries werden lokal oder in CI gebaut und per SFTP hochgeladen, dann `systemd`-Neustart.
- **Das Dashboard muss öffentlich erreichbar sein** (Einladung und OAuth2-Redirect). Optionen: Tailscale Funnel oder Reverse-Proxy (Caddy) mit Domain und HTTPS. Offen, siehe unten.

## Phasen
1. Gerüst: Workspace, Datenbank, Discord-Login, Dashboard-Grundlayout, Bot-Einladung.
2. Moderation: Kick/Ban/Timeout, Warns, Auto-Mod, Logs.
3. Tickets: Panels, Kategorien, Transkripte, Claim/Close.
4. Server-Designer und Templates.
5. Willkommensnachrichten, Reaction Roles, weiteres Logging.

## Offene Fragen an dich
1. Ansatz 2 (Empfehlung) oder Ansatz 1?
2. Serverzugang: Tailscale-Name/IP, Benutzer, Domain, Funnel oder Caddy?
3. Endgültiger Projektname?
4. Discord-Anwendung: Du legst im Developer Portal eine App an und gibst mir Client-ID. Token und Secret trägst du selbst in die Server-Konfiguration ein, ich frage sie nicht ab.
