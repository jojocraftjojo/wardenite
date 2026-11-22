#!/usr/bin/env bash
# Baut fuer Linux und laedt das Binary per SFTP auf den Server (ueber Tailscale).
# Voraussetzungen: WARDENITE_HOST (Tailscale-Name), WARDENITE_USER, Rust-Target
#   rustup target add x86_64-unknown-linux-musl   (+ passender Linker, z. B. cargo-zigbuild)
set -euo pipefail
: "${WARDENITE_HOST:?Tailscale-Hostname setzen}"
: "${WARDENITE_USER:?SSH-Benutzer setzen}"
TARGET="${TARGET:-x86_64-unknown-linux-musl}"
REMOTE_DIR="${REMOTE_DIR:-/opt/wardenite}"

cargo build --release --target "$TARGET" -p wardenite
BIN="target/$TARGET/release/wardenite"

sftp "$WARDENITE_USER@$WARDENITE_HOST" <<SFTP
put $BIN $REMOTE_DIR/wardenite.new
bye
SFTP
ssh "$WARDENITE_USER@$WARDENITE_HOST" \
  "chmod +x $REMOTE_DIR/wardenite.new && mv $REMOTE_DIR/wardenite.new $REMOTE_DIR/wardenite && sudo systemctl restart wardenite"
echo "Deployt auf $WARDENITE_HOST"
