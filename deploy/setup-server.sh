#!/usr/bin/env bash
# AUF DEM SERVER ausfuehren (als root oder mit sudo), nachdem die Dateien hochgeladen wurden.
# Legt Benutzer und Ordner an, installiert den systemd-Dienst. Startet ihn noch NICHT,
# denn zuerst muss die .env mit deinen Discord-Daten ausgefuellt werden.
set -euo pipefail
DIR=/opt/wardenite
id wardenite >/dev/null 2>&1 || useradd --system --home "$DIR" --shell /usr/sbin/nologin wardenite
mkdir -p "$DIR"
[ -f "$DIR/wardenite" ] || { echo "Fehlt: $DIR/wardenite (Binary hochladen, siehe deploy/DEPLOY.md)"; exit 1; }
chmod +x "$DIR/wardenite"
[ -f "$DIR/.env" ] || { cp "$DIR/.env.example" "$DIR/.env" 2>/dev/null || true; }
chown -R wardenite:wardenite "$DIR"
chmod 600 "$DIR/.env" 2>/dev/null || true
install -m 644 "$DIR/wardenite.service" /etc/systemd/system/wardenite.service
systemctl daemon-reload
systemctl enable wardenite
echo "Fertig. Jetzt $DIR/.env ausfuellen und dann: sudo systemctl start wardenite"
