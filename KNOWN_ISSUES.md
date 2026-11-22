# Problèmes connus

Liste des points restants après l'audit de sécurité et le nettoyage de novembre 2026.
Classés du plus important au moins important.

## Sécurité

### 1. Le site tourne en HTTP, sans chiffrement (critique)

Le formulaire de connexion envoie l'identifiant et le mot de passe EPITA en clair, tout comme le cookie de session. N'importe qui sur le même réseau (Wi-Fi de l'école, par exemple) peut les lire.

**À faire :** passer en HTTPS, avec un nom de domaine et un reverse proxy (Caddy obtient les certificats tout seul) ou un tunnel Cloudflare. Ensuite, mettre `SESSION_COOKIE_SECURE=true` et `TRUSTED_PROXY=true`, et ajouter l'en-tête `Strict-Transport-Security`.

### 2. Une session volée reste valable 30 jours

La déconnexion supprime seulement le cookie dans le navigateur ; le serveur ne garde pas de liste des sessions. Un cookie volé (voir point 1) fonctionne jusqu'à son expiration.

**À faire :** stocker un identifiant de session côté serveur (table `sessions`) pour pouvoir révoquer, et raccourcir la durée.

### 3. La politique CSP autorise les scripts inline

`script-src 'unsafe-inline'` est nécessaire parce que `base.html` et `login.html` contiennent des scripts inline. Cela affaiblit la protection du navigateur si une faille XSS apparaissait.

**À faire :** déplacer ces scripts dans `static/js/` puis retirer `'unsafe-inline'`.

### 4. Les dépendances n'ont pas été scannées automatiquement

Les versions des bibliothèques sensibles (rustls, ring, hyper, SQLite…) ont été vérifiées à la main, mais `cargo audit` n'a pas tourné.

**À faire :** `cargo install cargo-audit && cargo audit`, idéalement dans une GitHub Action.

### 5. La déconnexion passe par un lien GET

N'importe quel site peut déconnecter un utilisateur avec une simple image pointant vers `/logout`. Gênant, pas dangereux.

## Données

### 6. Le compte admin dépend du nom renvoyé par Keycloak

Depuis l'audit, la session prend le nom `preferred_username` du jeton Keycloak au lieu du nom saisi. Si Keycloak renvoie autre chose que `fernand.huttois` (un numéro d'étudiant, par exemple), le lien Admin disparaît. Vérifier après une connexion ; sinon, adapter `ADMIN_USERS`.

### 7. Les salles et groupes reviennent au fil des connexions

Jusqu'au commit `5dc710a`, les salles et groupes n'étaient pas lus depuis Auriga. Les cours importés entre mi-octobre et le 22 novembre n'en ont pas tant que quelqu'un de la promotion ne s'est pas reconnecté.

### 8. Les cours ne sont jamais supprimés

Un cours retiré d'Auriga reste dans `school.db`. Les cours annulés sont gardés avec `is_cancelled = 1` et masqués. Prévoir un nettoyage des cours qu'aucune connexion récente n'a confirmés.

### 9. Restes d'anciennes fonctionnalités dans `school.db`

- Table `recurring_courses` (282 lignes) et colonnes `courses.recurring_id` / `courses.pattern_key` : liées à l'ancienne vue « planning type », plus lues ni écrites.
- Colonnes `rooms.building` et `rooms.floor` : jamais remplies.
- Table `contributions` : compte les cours ajoutés à chaque connexion, rien ne la lit.

Les supprimer demande une migration (et une sauvegarde) ; sans urgence.

## Fonctionnalités

- **Plan des salles :** seul le 3e étage est dessiné (`static/js/floorplan.js`).
- **Calendriers des clubs :** l'API `/api/community/*` existe mais l'interface est désactivée (`EVENTS_ENABLED = false` dans `static/js/zeus_classic.js`).
- **Base commune :** elle ne contient que les cours des étudiants qui se sont connectés ; un groupe sans utilisateur n'apparaît pas dans Planning.

## Déploiement

- `epikairos.service` pointe vers `/home/fernand/playground/zeus-v2` ; sur le serveur, l'application est dans `~/epikairos`.
