# Problèmes connus

Points restants, classés du plus important au moins important. Mis à jour le 22 novembre 2026.

## Sécurité

### 1. Le port 5000 est encore ouvert en HTTP

Le site passe par `https://epikairos.org` (tunnel Cloudflare), mais l'ancienne adresse `http://176.174.155.8:5000` répond toujours, sans chiffrement, pour les anciens liens d'abonnement. Sur ce port, quelqu'un peut aussi contourner la limite de tentatives de connexion en falsifiant son adresse IP.

**À faire :** quand plus personne n'utilise les anciens liens, ajouter `HOST=127.0.0.1` au service et fermer le port 5000 sur la box. Ensuite, ajouter l'en-tête `Strict-Transport-Security`.

### 2. Une session volée reste valable 30 jours

La déconnexion supprime seulement le cookie dans le navigateur ; le serveur ne garde pas de liste des sessions.

**À faire :** stocker un identifiant de session côté serveur (table `sessions`) pour pouvoir révoquer, et raccourcir la durée.

### 3. La politique CSP autorise les scripts inline

`script-src 'unsafe-inline'` est nécessaire parce que `base.html` et `login.html` contiennent des scripts inline.

**À faire :** déplacer ces scripts dans `static/js/` puis retirer `'unsafe-inline'`.

### 4. Les dépendances n'ont pas été scannées automatiquement

**À faire :** `cargo install cargo-audit && cargo audit`, idéalement dans une GitHub Action.

### 5. Des anciens liens gardent un mot de passe chiffré

Les liens créés avant l'arrêt de l'enregistrement des mots de passe gardent le leur, chiffré avec `.server_secret`. Celui qui contrôle le serveur peut le déchiffrer.

**À faire :** supprimer les anciens liens inutilisés depuis le panneau des liens d'abonnement.

### 6. La déconnexion passe par un lien GET

N'importe quel site peut déconnecter un utilisateur avec une simple image pointant vers `/logout`. Gênant, pas dangereux.

## Données

### 7. Le compte admin dépend du nom renvoyé par Auriga

La session prend le nom `preferred_username` du jeton Keycloak, pas le nom saisi. Si ce n'est pas `fernand.huttois`, le lien Admin disparaît ; adapter alors `ADMIN_USERS`.

### 8. Les cours ne sont jamais supprimés

Un cours retiré d'Auriga reste dans `school.db`, et donc dans les liens mis à jour par les camarades. Les cours annulés sont gardés avec `is_cancelled = 1` et masqués.

**À faire :** nettoyer les cours qu'aucune connexion récente n'a confirmés.

### 9. Restes d'anciennes fonctionnalités dans `school.db`

- Table `recurring_courses` et colonnes `courses.recurring_id` / `courses.pattern_key` : plus lues ni écrites.
- Colonnes `rooms.building` et `rooms.floor` : jamais remplies.
- Table `contributions` : rien ne la lit.

Les supprimer demande une migration et une sauvegarde ; sans urgence.

## Fonctionnalités

- Plan des salles : seul le 3e étage est dessiné (`static/js/floorplan.js`).
- Calendriers des clubs : l'API `/api/community/*` et les modérateurs de la page Admin existent encore, mais l'interface de Planning a été retirée. Un modérateur ne peut donc rien publier pour l'instant. L'ancienne interface est dans l'historique git.
- Base commune : elle ne contient que les cours des étudiants qui se sont connectés. Un cours suivi par un seul étudiant ne se met à jour dans son lien qu'à ses propres connexions.

## Déploiement

- `epikairos.service` pointe vers `/home/fernand/playground/zeus-v2` ; sur le serveur, l'application est dans `~/epikairos`, avec les réglages HTTPS dans `/etc/systemd/system/epikairos.service.d/https.conf`.
