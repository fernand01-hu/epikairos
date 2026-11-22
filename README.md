# EpiKairos

Site web pour consulter son emploi du temps EPITA à partir d'Auriga, voir les salles libres et s'abonner à son planning depuis Google Calendar.

## Pages

- Mon planning : vos cours de la semaine, filtrables par matière, avec export `.ics` ou lien d'abonnement.
- Planning : l'emploi du temps d'un groupe ou d'un professeur.
- Salles libres : le plan du 3e étage avec l'état de chaque salle.
- Admin : gestion des modérateurs, visible seulement par les comptes de `ADMIN_USERS`.

À chaque connexion, les cours récupérés depuis Auriga sont ajoutés à une base commune. C'est elle qui alimente Planning et Salles libres.

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

## Données

Deux bases SQLite :

- `school.db` : la base commune (cours, salles, professeurs, groupes, comptes).
- `feeds.db` : les liens d'abonnement, avec le mot de passe Auriga chiffré quand il est fourni.

Deux fichiers de clés sont créés au premier lancement : `.flask_secret` pour les sessions et `.server_secret` pour le chiffrement des mots de passe. Il faut les garder : sans `.server_secret`, les liens d'abonnement existants ne se mettent plus à jour.

## Tests

```
cargo test
```

Les tests dans `tests/` démarrent l'application complète avec des bases temporaires.

## Problèmes connus

Voir `KNOWN_ISSUES.md`. Le plus important : le site tourne encore en HTTP.
