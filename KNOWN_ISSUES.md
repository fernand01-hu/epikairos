# Problèmes connus

Mis à jour le 22 novembre 2026.

## Une session copiée reste valable jusqu'à 30 jours

Le cookie de session ne peut plus être intercepté sur le réseau (HTTPS, cookie `Secure`) ni lu par un script de la page (`HttpOnly`). Il reste un cas : quelqu'un qui accède à l'appareil lui-même (ordinateur partagé resté connecté, logiciel malveillant) peut copier le cookie. La déconnexion le supprime seulement dans ce navigateur ; une copie reste valable jusqu'à son expiration, 30 jours après la connexion.

**À faire si besoin :** garder la liste des sessions côté serveur (table `sessions`) pour pouvoir les révoquer, et raccourcir la durée. Cela déconnecte tout le monde une fois.

## La base commune dépend des connexions

Elle ne contient que les cours des étudiants qui se sont connectés. Un groupe dont personne ne se connecte n'apparaît pas dans Planning, et un cours suivi par un seul étudiant ne se met à jour dans son lien d'abonnement qu'à ses propres connexions.

## Déploiement

`epikairos.service` pointe vers `/home/fernand/playground/zeus-v2` ; sur le serveur, l'application est dans `~/epikairos`, avec les réglages HTTPS dans `/etc/systemd/system/epikairos.service.d/https.conf`.
