# 🔐 Git-Authentifizierung einrichten

Dieses Dokument erklärt, wie du die automatische Git-Authentifizierung für dieses Repository einrichtest.

## Option 1: Personal Access Token (Empfohlen)

### Schritt 1: GitHub Personal Access Token erstellen

1. Gehe zu: https://github.com/settings/tokens
2. Klicke auf "Generate new token" → "Generate new token (classic)"
3. Gib einen Namen ein (z.B. "apt-ng-development")
4. Wähle die benötigten Scopes:
   - `repo` (für private Repositories) oder
   - `public_repo` (für öffentliche Repositories)
5. Klicke auf "Generate token"
6. **WICHTIG**: Kopiere den Token sofort (er wird nur einmal angezeigt!)

### Schritt 2: Token in .env speichern

```bash
cd /root/projects/apt-ng
cp .env.example .env
# Bearbeite .env und füge deinen Token ein
nano .env  # oder vim/editor deiner Wahl
```

Die `.env` Datei sollte so aussehen:
```env
GITHUB_TOKEN=ghp_dein_token_hier
GITHUB_USERNAME=KyoshiHikari
```

### Schritt 3: Authentifizierung einrichten

```bash
./scripts/setup-git-auth.sh
```

Das Script:
- Liest die `.env` Datei
- Konfiguriert Git Credential Helper
- Speichert die Credentials sicher

### Schritt 4: Testen

```bash
git ls-remote origin
```

Wenn das ohne Fehler funktioniert, ist die Authentifizierung erfolgreich!

## Option 2: SSH-Keys (Alternative)

### Schritt 1: SSH-Key generieren

```bash
ssh-keygen -t ed25519 -C "your_email@example.com"
# Drücke Enter für den Standard-Pfad
# Optional: Gib ein Passwort ein
```

### Schritt 2: Public Key zu GitHub hinzufügen

```bash
cat ~/.ssh/id_ed25519.pub
```

1. Kopiere den gesamten Output
2. Gehe zu: https://github.com/settings/keys
3. Klicke auf "New SSH key"
4. Füge den Key ein und speichere

### Schritt 3: Remote auf SSH umstellen

```bash
cd /root/projects/apt-ng
git remote set-url origin git@github.com:KyoshiHikari/apt-ng.git
```

### Schritt 4: Testen

```bash
ssh -T git@github.com
```

Du solltest eine Nachricht sehen wie: "Hi KyoshiHikari! You've successfully authenticated..."

## Verwendung

Nach der Einrichtung kannst du einfach pushen:

```bash
# Normaler Push
git push

# Oder mit dem Convenience-Script
./scripts/git-push.sh
```

## Sicherheit

- **NIEMALS** committe die `.env` Datei!
- Die `.env` Datei ist bereits in `.gitignore`
- Git Credentials werden in `~/.git-credentials` gespeichert (chmod 600)
- Für Produktionsumgebungen: Verwende SSH-Keys oder GitHub CLI

## Troubleshooting

### "Permission denied" Fehler

1. Prüfe ob der Token noch gültig ist
2. Prüfe ob der Token die richtigen Scopes hat
3. Führe `./scripts/setup-git-auth.sh` erneut aus

### "Could not read Username"

1. Stelle sicher, dass `GITHUB_USERNAME` in `.env` gesetzt ist
2. Prüfe ob `git config credential.helper` auf "store" gesetzt ist

### Token abgelaufen

1. Erstelle einen neuen Token auf GitHub
2. Aktualisiere die `.env` Datei
3. Führe `./scripts/setup-git-auth.sh` erneut aus

