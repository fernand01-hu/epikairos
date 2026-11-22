# EpiKairos

Application web auto-hébergeable pour les étudiants d'EPITA : connexion à **Auriga**, emploi du temps de la semaine, planning des groupes, salles libres sur le plan de l'étage, et export vers **Google Calendar**, **Apple Calendar** ou **Outlook**.

Backend en Rust (Axum, MiniJinja, SQLite).

## Pages

| Page | Adresse | Rôle |
|---|---|---|
| Mon planning | `/personal` | Vos cours en grille horaire, filtre par matière, export `.ics` et lien d'abonnement |
| Planning | `/planning` | Emploi du temps de n'importe quel groupe EPITA |
| Salles libres | `/rooms` | Plan du 3e étage : libre (vert), cours dans moins d'1 h (jaune), occupée (rouge) |
| Admin | `/admin` | Réservé aux comptes de `ADMIN_USERS` (404 pour les autres) |

## Démarrage

```bash
cargo run --release            # http://localhost:5000
PORT=8080 cargo run --release  # autre port
```

### Docker

```bash
docker compose up -d
```

Les bases SQLite et les clés (`.flask_secret` pour les sessions, `.server_secret` pour chiffrer les mots de passe des abonnements) sont dans `./data` et survivent aux reconstructions.

### Service systemd

```bash
cargo build --release
sudo cp epikairos.service /etc/systemd/system/   # adapter WorkingDirectory
sudo systemctl daemon-reload
sudo systemctl enable --now epikairos
```

Toutes les variables d'environnement sont documentées dans `.env.example`.

## Abonnement Google Calendar

1. Sur **Mon planning**, cliquez sur **Lien d'abonnement** et copiez l'URL.
2. Dans Google Calendar : **Autres agendas** → **+** → **À partir de l'URL**, collez l'URL.

Le serveur rafraîchit le flux depuis Auriga au plus toutes les 12 h (`FEED_CACHE_DURATION`). Google le relit à son propre rythme (souvent 12 à 24 h).

## Sécurité

- Connexion via le Keycloak d'Auriga (PKCE). La session (cookie signé HMAC, `HttpOnly`, `SameSite=Lax`, 30 jours) porte le nom renvoyé par Keycloak, pas celui saisi.
- Le mot de passe Auriga n'est jamais gardé dans le navigateur. Pour un lien d'abonnement auto-actualisé, il est stocké chiffré (Fernet) côté serveur.
- 10 tentatives de connexion par minute et par IP. Corps de requête limité à 2 Mo.
- Seules les données renvoyées par Auriga alimentent la base commune.
- Derrière un reverse proxy HTTPS : `TRUSTED_PROXY=true` et `SESSION_COOKIE_SECURE=true`.

## Administration

- `ADMIN_USERS` (par défaut `fernand.huttois`) liste les administrateurs.
- Depuis `/admin`, l'administrateur nomme des modérateurs.
- Les calendriers de clubs (API `/api/community/*`) sont désactivés dans l'interface pour l'instant (`EVENTS_ENABLED` dans `static/js/zeus_classic.js`).

## Tests

```bash
cargo test
```

- `tests/` : tests d'intégration qui lancent l'application complète en mémoire (pages, authentification, API, abonnements, salles, administration, sécurité).
- `src/` : tests unitaires des fonctions internes.

## Problèmes connus

Voir [KNOWN_ISSUES.md](KNOWN_ISSUES.md) : sécurité restante (HTTPS en priorité), données et déploiement.
