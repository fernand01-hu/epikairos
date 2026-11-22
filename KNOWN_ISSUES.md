# Problèmes connus

Points restants, classés du plus important au moins important. Mis à jour le 22 novembre 2026.

Réglés récemment : passage en HTTPS et fermeture du port 5000, en-tête HSTS, adresse IP des visiteurs impossible à falsifier, déconnexion par formulaire (un lien vers `/logout` ne déconnecte plus), politique CSP sans scripts inline, scan automatique des dépendances (`.github/workflows/ci.yml`), nom de compte admin vérifié.

## Sécurité

### 1. Une session volée reste valable 30 jours

La déconnexion supprime seulement le cookie dans le navigateur ; le serveur ne garde pas de liste des sessions.

**À faire :** stocker un identifiant de session côté serveur (table `sessions`) pour pouvoir révoquer, et raccourcir la durée.

### 2. Des anciens liens gardent un mot de passe chiffré

Les liens créés avant l'arrêt de l'enregistrement des mots de passe gardent le leur, chiffré avec `.server_secret`. Celui qui contrôle le serveur peut le déchiffrer.

**À faire :** supprimer les anciens liens inutilisés depuis le panneau des liens d'abonnement.

### 3. L'application écoute encore sur toutes les interfaces

Le port 5000 est fermé sur la box, mais l'application répond encore en HTTP aux appareils du réseau local. Ils ne peuvent plus fausser leur adresse IP (seul le tunnel, qui se connecte en local, est cru), mais leurs échanges ne sont pas chiffrés.

**À faire :** ajouter `Environment=HOST=127.0.0.1` dans `/etc/systemd/system/epikairos.service.d/https.conf`, puis `sudo systemctl daemon-reload && sudo systemctl restart epikairos`.

## Données

### 4. Les cours ne sont jamais supprimés

Un cours retiré d'Auriga reste dans `school.db`, et donc dans les liens mis à jour par les camarades. Les cours annulés sont gardés avec `is_cancelled = 1` et masqués.

**À faire :** nettoyer les cours qu'aucune connexion récente n'a confirmés.

### 5. Restes d'anciennes fonctionnalités dans `school.db`

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
