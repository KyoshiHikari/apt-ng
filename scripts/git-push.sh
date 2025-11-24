#!/bin/bash
# Convenience-Script zum Pushen mit automatischer Authentifizierung

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="$PROJECT_ROOT/.env"

# Lade .env falls vorhanden
if [ -f "$ENV_FILE" ]; then
    source "$ENV_FILE"
fi

cd "$PROJECT_ROOT"

# Prüfe ob wir auf dem richtigen Branch sind
CURRENT_BRANCH=$(git branch --show-current)
echo "🌿 Aktueller Branch: $CURRENT_BRANCH"

# Zeige Status
echo ""
echo "📊 Git Status:"
git status --short

# Frage nach Bestätigung
read -p "🚀 Änderungen pushen? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Abgebrochen"
    exit 1
fi

# Push
echo ""
echo "⬆️  Pushe nach origin/$CURRENT_BRANCH..."
git push -u origin "$CURRENT_BRANCH"

echo ""
echo "✅ Erfolgreich gepusht!"

