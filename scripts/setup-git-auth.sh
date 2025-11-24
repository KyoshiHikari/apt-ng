#!/bin/bash
# Script zum Einrichten der Git-Authentifizierung mit Personal Access Token

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="$PROJECT_ROOT/.env"

echo "🔐 Git-Authentifizierung einrichten"
echo ""

# Prüfe ob .env Datei existiert
if [ ! -f "$ENV_FILE" ]; then
    echo "⚠️  .env Datei nicht gefunden. Erstelle Beispiel-Datei..."
    cp "$PROJECT_ROOT/.env.example" "$ENV_FILE"
    echo "📝 Bitte bearbeite .env und füge deinen GitHub Personal Access Token ein"
    echo "   Token erstellen: https://github.com/settings/tokens"
    exit 1
fi

# Lade .env Datei
source "$ENV_FILE"

if [ -z "$GITHUB_TOKEN" ] || [ "$GITHUB_TOKEN" = "your_personal_access_token_here" ]; then
    echo "❌ Bitte setze GITHUB_TOKEN in der .env Datei"
    exit 1
fi

if [ -z "$GITHUB_USERNAME" ]; then
    echo "❌ Bitte setze GITHUB_USERNAME in der .env Datei"
    exit 1
fi

echo "✅ Konfiguriere Git Credential Helper..."

# Konfiguriere Git Credential Helper für dieses Repository
cd "$PROJECT_ROOT"
git config credential.helper store
git config credential.https://github.com.username "$GITHUB_USERNAME"

# Erstelle Credential-Datei
CREDENTIAL_FILE="$HOME/.git-credentials"
CREDENTIAL_LINE="https://${GITHUB_USERNAME}:${GITHUB_TOKEN}@github.com"

# Füge Credentials hinzu (entferne alte Einträge für github.com)
if [ -f "$CREDENTIAL_FILE" ]; then
    grep -v "github.com" "$CREDENTIAL_FILE" > "${CREDENTIAL_FILE}.tmp" || true
    mv "${CREDENTIAL_FILE}.tmp" "$CREDENTIAL_FILE"
fi

echo "$CREDENTIAL_LINE" >> "$CREDENTIAL_FILE"
chmod 600 "$CREDENTIAL_FILE"

echo "✅ Git-Authentifizierung erfolgreich eingerichtet!"
echo ""
echo "📝 Teste die Verbindung:"
echo "   git ls-remote origin"

