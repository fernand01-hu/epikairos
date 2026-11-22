# EpiKairos

Site web pour consulter son emploi du temps EPITA à partir d'Auriga, voir les salles libres et s'abonner à son planning depuis Google Calendar.

## Pages

- Mon planning : vos cours de la semaine, filtrables par matière, avec export `.ics` et liens d'abonnement.
- Planning : l'emploi du temps d'un groupe ou d'un professeur.
- Salles libres : le plan du 3e étage avec l'état de chaque salle.
- Admin : la liste des comptes qui se sont connectés, visible seulement par les comptes de `ADMIN_USERS`.

Le site est en français ou en anglais : le bouton EN / FR à côté du thème change la langue, retenue dans un cookie. Les textes anglais sont dans `src/i18n/en.json` ; un test vérifie qu'aucun texte n'a été oublié.

À chaque connexion, les cours récupérés depuis Auriga sont ajoutés à une base commune. C'est elle qui alimente Planning et Salles libres.

## Connexion

Deux façons de se connecter :

- Identifiant et mot de passe EPITA. Le serveur les envoie à Auriga pour récupérer les cours, puis les oublie. Le mot de passe n'est jamais enregistré, ni sur le serveur ni dans le navigateur.
- Jeton Auriga temporaire, pour ne jamais donner son mot de passe. Il se copie depuis les outils de développement du navigateur et n'est valable qu'une minute. La page de connexion explique comment faire.

## Liens d'abonnement

- Chaque compte peut avoir jusqu'à 3 liens, gérés depuis Mon planning : copier, mettre à jour avec les filtres actuels, supprimer.
- Aucun mot de passe n'est enregistré pour ces liens. Un lien se met à jour quand vous vous connectez, et aussi quand un camarade de votre groupe se connecte : ses cours arrivent dans la base commune et passent dans votre lien. Les cours que vous êtes seul à suivre se mettent à jour à vos propres connexions.

## Lancer le serveur

Il faut Rust (stable).

```
cargo run --release
```

Le site écoute sur `http://localhost:5000`. Les options (port, chemins des bases, admins) sont décrites dans `.env.example`.

Avec Docker :

```
docker compose up -d
```

Avec systemd, compiler avec `cargo build --release`, adapter les chemins dans `epikairos.service`, puis :

```
sudo cp epikairos.service /etc/systemd/system/
sudo systemctl enable --now epikairos
```

Derrière un proxy HTTPS (Cloudflare Tunnel, Caddy), mettre `SESSION_COOKIE_SECURE=true`, `TRUSTED_PROXY=true` et `SERVER_DOMAIN`.

## Données

Deux bases SQLite :

- `school.db` : la base commune (cours, salles, professeurs, groupes, comptes).
- `feeds.db` : les liens d'abonnement, avec leurs filtres et une copie des cours. Aucun mot de passe.

Le fichier `.flask_secret`, créé au premier lancement, signe les cookies de session. Il faut le garder, sinon tout le monde est déconnecté.

## Tests

```
cargo test
```

Les tests dans `tests/` démarrent l'application complète avec des bases temporaires.

## Problèmes connus

Voir `KNOWN_ISSUES.md`.
