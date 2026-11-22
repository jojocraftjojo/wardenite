# Wardenite auf deinem Linux-Server einrichten

Du laedst die Dateien selbst hoch (SFTP ueber Tailscale). Auf deinem Mac wird nichts installiert.

## 1. Dateien besorgen
1. **Binary:** auf GitHub unter *Actions* den neuesten gruenen Lauf oeffnen, unten das Artefakt
   `wardenite-linux-x86_64` herunterladen und entpacken (Datei `wardenite`, ca. 8 MB, x86_64 Linux, gebaut auf Ubuntu 22.04).
   Direktlink: https://github.com/jojocraftjojo/wardenite/actions
   - Laeuft dein Server auf ARM (z. B. Raspberry Pi)? Dann passt dieses Binary nicht, siehe Abschnitt "Selbst bauen".
2. Aus dem Repo die Dateien `deploy/wardenite.service`, `deploy/setup-server.sh` und `.env.example`.

## 2. Hochladen (SFTP)
Alles nach `/opt/wardenite/` auf dem Server legen (Ordner ggf. vorher mit `sudo mkdir -p /opt/wardenite` anlegen
und fuer deinen Upload-Benutzer beschreibbar machen):

```text
/opt/wardenite/wardenite            (Binary)
/opt/wardenite/wardenite.service
/opt/wardenite/setup-server.sh
/opt/wardenite/.env.example
```

## 3. Auf dem Server einrichten
```bash
sudo bash /opt/wardenite/setup-server.sh
sudo cp /opt/wardenite/.env.example /opt/wardenite/.env
sudo nano /opt/wardenite/.env
```

## 4. Discord-Anwendung (Developer Portal, discord.com/developers)
1. *New Application* anlegen.
2. **Bot** -> Token erzeugen (nur du siehst ihn) und **Message Content Intent** sowie **Server Members Intent** aktivieren.
3. **OAuth2** -> *Client ID* und *Client Secret* kopieren.
4. **OAuth2 -> Redirects**: `https://<DEIN-HOST>/callback` eintragen (siehe Schritt 5 fuer die URL).
5. In `/opt/wardenite/.env` eintragen: `DISCORD_CLIENT_ID`, `DISCORD_CLIENT_SECRET`, `DISCORD_BOT_TOKEN`,
   `DASHBOARD_BASE_URL=https://<DEIN-HOST>` (ohne `/` am Ende) und `BIND_ADDR=127.0.0.1:8080`.
   Token und Secret nie in Chats, Issues oder das Repo schreiben.

## 5. Oeffentlich erreichbar machen (Tailscale Funnel)
Das Dashboard muss aus dem normalen Internet erreichbar sein (Bot-Einladung, Discord-Login).
Auf dem Server:
```bash
sudo tailscale funnel --bg 8080
tailscale funnel status      # zeigt die URL, z. B. https://server.dein-tailnet.ts.net
```
Voraussetzung: *HTTPS* und *Funnel* sind in der Tailscale-Admin-Konsole fuer das Tailnet bzw. den Server aktiviert.
Die angezeigte URL ist dein `<DEIN-HOST>`. Alternative mit eigener Domain: `deploy/Caddyfile`.

## 6. Starten und pruefen
```bash
sudo systemctl start wardenite
sudo systemctl status wardenite
journalctl -u wardenite -f
```
Dann `https://<DEIN-HOST>` im Browser oeffnen, mit Discord anmelden, Server waehlen, Bot einladen.

## Updates
Neues Binary hochladen (z. B. als `wardenite.new`), dann
`sudo mv /opt/wardenite/wardenite.new /opt/wardenite/wardenite && sudo chmod +x /opt/wardenite/wardenite && sudo systemctl restart wardenite`.
Die Datenbank (`wardenite.db`) bleibt erhalten, Migrationen laufen beim Start automatisch.

## Selbst bauen (z. B. ARM-Server)
Auf dem Server:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/jojocraftjojo/wardenite && cd wardenite
cargo build --release -p wardenite      # Binary: target/release/wardenite
```

## Hinweis
Der Bot wurde bisher nicht gegen echtes Discord getestet. Fehler bitte als GitHub-Issue melden.
