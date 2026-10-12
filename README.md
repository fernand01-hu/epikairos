# 📅 Auriga Calendar Web Service (EPITA)

Application web auto-hébergeable pour permettre à n'importe quel étudiant d'EPITA de se connecter à **Auriga**, de **prévisualiser et filtrer ses cours avec des expressions régulières (Regex)**, et d'exporter son emploi du temps vers **Google Calendar**, **Apple Calendar** ou **Outlook**.

Conçu pour tourner sur votre **serveur personnel à la maison** (PC Linux, Raspberry Pi, NAS, Docker ou VPS) avec une interface super simple et intuitive.

---

## ✨ Fonctionnalités clés

- 🔐 **Connexion sécurisée Keycloak PKCE** : Utilise vos identifiants EPITA (`prenom.nom` + mot de passe).
- ⚡ **Interface épurée et réactive** :
  - Connexion rapide en 1 clic.
  - Statistiques en direct (nombre de cours trouvés, répartition CM / TD / TP / Examens, cours annulés).
  - Tableau interactif avec sélection individuelle par cases à cocher.
- 🎯 **Filtrage puissant par Regex (Expressions Régulières)** :
  - **Filtre Regex d'Inclusion** : Ne garder que les cours correspondant au motif (ex: `ALGO|MATH|SYSTEME`).
  - **Filtre Regex d'Exclusion** : Retirer automatiquement les cours inutiles (ex: `Anglais|Sport|Soutenance`).
  - **Boutons presets en 1 clic** : `CM uniquement`, `TD uniquement`, `Examens`, `Pas d'anglais`.
  - **Filtre par types d'activités** : Chips cliquables pour inclure ou masquer `CM`, `TD`, `TP`, `EXAM`, etc.
  - **Interrupteur cours annulés** : Masquer ou afficher les cours annulés.
- 📆 **Exportation Google Calendar** :
  - **Téléchargement direct `.ics`** : Prêt à être importé dans Google Calendar.
  - **Lien d'abonnement auto-sync (URL Webcal)** : Google Calendar se met à jour automatiquement en arrière-plan sans intervention manuelle !
  - Conforme aux standards RFC 5545 (fuseau `Europe/Paris`, heures d'été/hiver, identifiants stables anti-doublons).
- 💻 **Mode CLI disponible** : Script en ligne de commande inclus (`auriga_to_ics.py`).

---

## 🚀 Démarrage rapide sur votre serveur à la maison

### Méthode 1 : Avec Python (Le plus simple)

1. **Installer les dépendances :**
   ```bash
   pip install -r requirements.txt
   ```

2. **Lancer le serveur web :**
   ```bash
   python3 app.py
   ```
   *L'application est accessible sur `http://localhost:5000` (ou l'adresse IP locale de votre serveur, ex: `http://192.168.1.50:5000`).*

Pour changer le port d'écoute :
```bash
PORT=8080 python3 app.py
```

---

### Méthode 2 : Avec Docker & Docker Compose

Lancez le service en tâche de fond avec une seule commande :

```bash
docker compose up -d
```

Pour voir les logs :
```bash
docker compose logs -f
```

---

### Méthode 3 : En service d'arrière-plan Linux (systemd)

Un fichier d'unité [auriga-calendar.service](file:///home/fernand/playground/zeus-v2/auriga-calendar.service) est fourni :

```bash
# Copier le fichier de service
sudo cp auriga-calendar.service /etc/systemd/system/

# Activer et démarrer au boot
sudo systemctl daemon-reload
sudo systemctl enable --now auriga-calendar

# Vérifier le statut
sudo systemctl status auriga-calendar
```

---

## 🔍 Exemples de filtres Regex utiles

| Besoin de l'étudiant | Filtre à utiliser | Type de filtre |
|---|---|---|
| Garder uniquement les Cours Magistraux | `^\[CM\]` ou `CM` | **Inclusion** |
| Garder uniquement les matières d'algo et maths | `ALGO|MATH` | **Inclusion** |
| Garder uniquement les partiels et examens | `EXAM|Partiel` | **Inclusion** |
| Retirer les cours d'anglais | `Anglais` | **Exclusion** |
| Retirer les soutenances et activités optionnelles | `Soutenance|EPS|Conférence` | **Exclusion** |
| Filtrer par enseignant spécifique | `Turing|Shannon` | **Inclusion** |
| Filtrer par salle | `Amphi Pasteur|Salle 402` | **Inclusion** |

---

## 📲 Comment importer dans Google Calendar

### Option A : Import ponctuel de fichier `.ics`
1. Connectez-vous sur votre site, ajustez vos filtres et cliquez sur **Télécharger .ics**.
2. Allez sur **[Google Calendar](https://calendar.google.com)**.
3. Cliquez sur l'icône ⚙️ **Paramètres** en haut à droite.
4. Dans le menu de gauche, cliquez sur **Importer et exporter**.
5. Sélectionnez le fichier `auriga_planning.ics` téléchargé et validez.

### Option B : Synchronisation automatique 24/7 (Lien d'abonnement)
1. Sur le site, cliquez sur **Lien d'abonnement Google Calendar**.
2. Copiez l'URL générée.
3. Sur Google Calendar (sur PC), dans le menu de gauche, cliquez sur le **+** à côté de **Autres agendas**.
4. Sélectionnez **À partir de l'URL**.
5. Collez l'URL : Google Calendar mettra à jour votre emploi du temps automatiquement !

---

## 🧪 Tests unitaires

Tous les tests unitaires peuvent être exécutés avec :

```bash
python3 test_exporter.py
python3 test_app.py
```
