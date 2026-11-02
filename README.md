# Bookwise Nickel — Readwise Books sur Kobo

Adaptation de [NickelHardcover](https://codeberg.org/StrayRose/NickelHardcover),
basée sur la révision `a176fd52510a6920e18014f8d54564a5c8d77e03` (licence MIT,
copyright Ava Johnson conservé dans `LICENSE`). Elle réutilise le plugin natif
Nickel/Qt, la CLI Rust et l'extraction ISBN de l'amont, et ajoute un fournisseur
**Readwise Books indépendant de Hardcover**.

**Version expérimentale. Firmware Kobo 5.x non pris en charge.** La compilation
et les tests locaux ne remplacent pas une validation sur une vraie liseuse.
Les captures du dossier `screenshots/` proviennent de l'amont Hardcover et ne
constituent pas des captures de cette intégration.

## Fonctionnalités Readwise Books

- Sessions horodatées depuis l'ouverture de la vue de lecture jusqu'à sa sortie,
  la mise en veille, un changement de livre ou une synchronisation.
- Progression ebook convertie de 0–100 vers 0–1, sans inventer de durée entre deux
  connexions Wi-Fi. Les périodes hors de la vue de lecture sont exclues.
- File persistante sur la Kobo pour les sessions terminées hors ligne ; envoi
  manuel, selon les seuils/horaires configurés, ou à la reconnexion Wi-Fi.
- Identification par ISBN/ASIN, sinon titre et auteur ; association manuelle
  à un livre **déjà présent dans la bibliothèque Readwise Books**.
- Statuts « à lire », « en cours », « terminé », « abandonné », notes et critiques.
- Publication dans le fil Readwise Books activée par défaut
  (`readwise_publish_to_feed = true`). Définir ce réglage à `false` pour ne pas
  publier les sessions dans le fil ; elles comptent toujours dans les statistiques
  et les classements Readwise. Un `false` explicite dans une configuration existante
  reste respecté, et les sessions déjà en attente conservent leur choix de publication.

La documentation de référence est <https://readwise.io/books_api>, API v1
expérimentale consultée le 1er novembre 2026. **Elle n'expose pas de création de
surlignages/notes, de journal Hardcover, de recherche globale de catalogue ou de
statut « en pause »** : les commandes correspondantes ne sont pas proposées en
mode Readwise. Ce projet n'utilise ni Reader ni l'API classique des surlignages.
Le mode Hardcover reste disponible avec `provider = hardcover` (valeur par défaut).

## Configuration sur la liseuse

L'installation utilise les mêmes chemins que NickelHardcover et le remplace :
ne pas installer les deux plugins simultanément. Sauvegarder d'abord la base Kobo
et `.adds/NickelHardcover/`. Les releases de NickelHardcover ne contiennent pas
cette adaptation ; compiler ce dépôt pour obtenir son `KoboRoot.tgz`.

1. Copier le `KoboRoot.tgz` de cette version dans `.kobo/`, éjecter la Kobo et
   attendre son redémarrage.
2. Obtenir un **jeton personnel** sur <https://readwise.io/access_token>, avec un
   compte ayant accès à Readwise Books. Ne pas le partager dans un ticket ou un chat.
3. Par USB, créer/éditer `.adds/NickelHardcover/config.ini` en conservant les autres
   réglages éventuels :

   ```ini
   provider = readwise
   readwise_token = VOTRE_JETON_PERSONNEL
   readwise_publish_to_feed = true
   auto_sync_default = false
   retry_on_network = true
   sync_on_close = 1
   sync_on_read = 20
   sync_on_schedule = -1
   ```

4. Éjecter la liseuse et rouvrir le livre. Le jeton n'est jamais passé en argument
   de processus ; il reste en clair dans ce fichier privé sur le stockage Kobo.
5. Activer **Enable auto-sync** pour le livre, ou lire quelques pages puis choisir
   **Sync now** dans le menu ajouté à la barre de lecture. Une synchronisation
   manuelle envoie aussi les sessions en attente des autres livres.

Une association manuelle utilise l'identifiant **du livre de votre bibliothèque**,
pas l'identifiant du produit du catalogue. Les associations et pourcentages locaux
Hardcover/Readwise sont séparés. Après avoir changé une association, fermer puis
rouvrir le livre. Si aucun ISBN n'est disponible, une première synchronisation
avec titre/auteur crée le livre avant de pouvoir le noter.

Avant l'envoi d'une session, le plugin obtient le `book_id` par `POST /books`
avec `status: currently_reading`. Si l'identifiant est déjà connu, il vérifie le
livre par `GET /books?ids=…` au lieu de le recréer. Si cette vérification échoue,
la session reste en attente et n'est pas envoyée.

La lecture reprend après chaque intervalle envoyé. Les intervalles sans avancée
de pourcentage sont ignorés (y compris un retour en arrière) ; la résolution est
celle du pourcentage entier fourni par Kobo. La version ne devine pas si vous
lisez lorsque la vue reste ouverte sans action, ni si un saut de pages correspond
à une vraie lecture. Elle ne transforme pas 99 % en 100 % : si Kobo s'arrête à
99 %, utiliser le menu de statut pour marquer le livre terminé.

Les sessions terminées survivent aux redémarrages, mais une interruption brutale
avant leur fermeture peut perdre l'intervalle actif. Le fuseau du profil Readwise
est utilisé ; les instants envoyés sont en UTC. Les évaluations du widget amont
sont par demi-étoile ; la CLI accepte également les quarts d'étoile de l'API.

## Reprises et erreurs : ne pas créer de doublons

Readwise **ne déduplique pas `POST /sessions`**. Le plugin ne promet donc pas une
livraison « exactement une fois » en cas de panne réseau. Il privilégie l'absence
de doublons et conserve les cas à vérifier dans
`.adds/NickelHardcover/readwise-sessions/` :

| Extension | Signification | Action |
| --- | --- | --- |
| `.json` | Session non envoyée ou refusée explicitement (401/403/429) | Corriger le jeton/l'accès, attendre `Retry-After`, puis synchroniser |
| `.sending` | Envoi commencé, résultat incertain ou processus interrompu | Vérifier d'abord dans Readwise ; aucune relance automatique |
| `.sent` | Session acceptée, statut du livre restant à mettre à jour | Synchroniser ; seul le PATCH du livre est rejoué |
| `.failed` | Session explicitement rejetée, même si HTTP 200 | Corriger le problème avant de la remettre en `.json` |

Un fichier `.sending` ou `.failed` bloque les nouvelles sessions pour éviter de
réappliquer une progression ancienne après une plus récente. Faire une copie des
fichiers avant intervention et arrêter la synchronisation automatique :

- Si une session `.sending` existe dans Readwise, la renommer en `.sent` (le fichier
  contient son `book_id`). Si sa création n'a **certainement pas** eu lieu, la
  renommer en `.json`. En cas de doute, ne pas la renvoyer.
- Pour corriger l'association d'une session non envoyée, renseigner son
  `linked_id` avec l'ID du livre Readwise approprié. Ne pas réaffecter une session
  déjà acceptée à un autre livre.
- Une création de livre au résultat incertain bloque également les créations
  automatiques pour ce livre : le retrouver dans Readwise, l'associer manuellement
  et lancer **Sync now**. Le cache `readwise.sqlite` mémorise aussi les livres sans
  correspondance catalogue afin de ne pas les recréer à chaque synchronisation.

Avant de changer de **compte** Readwise, terminer/résoudre les envois du compte
précédent, sauvegarder puis retirer sa file et `readwise.sqlite`, et effacer ses
associations `readwise/` dans `library.ini`. Changer simplement le jeton n'effectue
pas une migration de compte.

## Limites de lecture des fichiers et couvertures

La recherche d'ISBN lit au maximum **8 Mio décompressés par entrée EPUB** et
**32 Mio au total**, métadonnées comprises. Au-delà, associer le livre manuellement ;
cela ne modifie pas le fichier EPUB ni sa lecture dans Kobo.

Les couvertures acceptent seulement JPEG/PNG via HTTPS sur les domaines explicitement
autorisés dans `hook/src/cover.cc`, y compris après redirection. Le chargement est limité
à 4 Mio, 4 194 304 pixels (4 096 par côté), trois redirections et 30 secondes au total.
Une couverture non prise en charge reste vide ; le livre reste sélectionnable.
Les cookies et identifiants d'API ne sont pas transmis aux serveurs de couvertures.

## Développement et vérifications

Depuis la racine, `.agents/setup` installe Rust et Qt pour les tests natifs Linux,
sans compte Readwise et sans liseuse. `scripts/check` exécute les tests Rust,
les tests de capture Qt et la vérification de compilation des sources C++.
Ces tests utilisent un serveur HTTP local qui simule le contrat documenté, pas le
service Readwise réel. Le rendu des écrans et les hooks Kobo nécessitent `libnickel`
et une vraie liseuse compatible ; aucun navigateur ne peut les reproduire.

Pour compiler l'archive ARM, utiliser la toolchain
[NickelTC](https://github.com/pgaskin/NickelTC), les sous-modules épinglés et les
tâches `mise.toml`. Le Dockerfile fournit Rust, mise et resvg dans cette toolchain :

```sh
git submodule update --init --recursive
docker build -t bookwise-nickeltc .
docker run --rm -v "$PWD:$PWD" -w "$PWD" -u "$(id -u):$(id -g)" \
  bookwise-nickeltc mise run build::package
```

Ne pas installer un binaire Linux x86 sur Kobo. Avant toute utilisation régulière,
vérifier sur appareil : ouverture/fermeture, veille/réveil, changement de livre,
reconnexion Wi-Fi, progression réelle et absence de doublons côté Readwise.
Pour désinstaller, sauvegarder les sessions à conserver, retirer
`.adds/NickelHardcover/` puis redémarrer manuellement la Kobo.
