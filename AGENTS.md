# Développement de Bookwise Nickel

Ce fichier regroupe les instructions pour les contributeurs et les agents, ainsi
que les procédures de maintenance avancées. Garder le [README](README.md) centré
sur l'installation et l'utilisation, avec les avertissements essentiels visibles.

## Origine et périmètre

- Fork de [StrayRose/NickelHardcover](https://codeberg.org/StrayRose/NickelHardcover),
  importé à la révision `a176fd52510a6920e18014f8d54564a5c8d77e03`.
- Conserver la licence MIT et le copyright Ava Johnson dans `LICENSE`.
- L'intégration Readwise Books est indépendante du fournisseur Hardcover, qui
  reste disponible et sélectionné par défaut (`provider = hardcover`).
  `provider = dual` active les deux services ; `provider = readwise` reste utilisable
  seul. Ne pas réintroduire un choix exclusif en mode double.
- Référence du contrat Readwise : <https://readwise.io/books_api>, API Books v1
  expérimentale consultée le 1er novembre 2026. Reconsulter cette documentation
  avant de modifier le contrat réseau ; ne pas confondre Books avec Reader ou
  l'API classique des surlignages.
- Cette API ne propose ni création de surlignages/notes, ni journal Hardcover,
  ni recherche globale de catalogue, ni statut « en pause ». Ne pas réactiver
  ces fonctions en mode Readwise sans support documenté.
- Seule exception : avec `readwise_sync_annotations`, les surlignages Kobo du
  livre synchronisé partent vers `POST /api/v2/highlights/` de l'
  [API classique](https://readwise.io/api_deets) (même jeton), après la file des
  sessions et même si elle est bloquée. Readwise déduplique par
  titre/auteur/texte/`source_url` : tout renvoyer à chaque synchronisation est
  sans risque. Cette clé ignore la note : comme pour le journal Hardcover, la CLI
  liste ensuite les surlignages du livre Readwise (`GET ?book_id=`, 20/min),
  les associe par texte et seconde de création, puis corrige les notes par
  `PATCH`. L'ID du livre vient de la réponse du `POST`, sinon de la table
  `highlight_books` de `readwise.sqlite`. Les suppressions sur la Kobo ne sont
  pas répercutées, et rien ne garantit que ces surlignages apparaissent dans
  Readwise Books. Ce réglage est distinct de `sync_annotations` (Hardcover).
- Le firmware Kobo 5.x n'est pas pris en charge. Les captures de `screenshots/`
  viennent d'une vraie Kobo en mode `dual` (build `33f2e66`) ; les refaire si
  l'interface change.

## Organisation du code

| Emplacement | Responsabilité |
| --- | --- |
| `hook/src/nickelhardcover.cc` | Intégration aux hooks et symboles de Nickel |
| `hook/src/synccontroller.cc`, `syncqueue.cc` | Cycle de lecture, déclencheurs et files indépendantes par service |
| `hook/src/readwisesessions.cc` | Capture et persistance des intervalles Readwise |
| `hook/src/settings.*`, `hook/res/config_example.ini` | Configuration Qt, réglages par livre et exemple distribué |
| `hook/src/cli.cc`, `cli/src/messages.rs` | Communication entre le plugin Qt et la CLI Rust |
| `cli/qt_codegen/` | Génération de `hook/src/messages.h` à partir des messages Rust |
| `cli/src/main.rs`, `cli/src/commands/` | Commandes et sélection du fournisseur |
| `cli/src/readwise.rs`, `cli/src/readwise/tests.rs` | Client Books, cache, livraison des sessions, envoi des surlignages (API v2) et tests HTTP locaux |
| `cli/src/hardcover.rs`, `cli/src/graphql/` | Intégration Hardcover conservée |
| `cli/src/epub.rs`, `cli/src/isbn.rs`, `cli/src/database.rs` | Métadonnées, identifiants et données Kobo |
| `hook/tests/readwisesessions.cc` | Tests natifs Qt de capture des sessions |
| `hook/tests/dualsync.cc`, `scripts/check-dual-sync` | Tests d'orchestration double avec vues Nickel et résultats CLI simulés |
| `mise.toml`, `Dockerfile`, `hook/Makefile` | Compilation ARM, génération et empaquetage |
| `scripts/nickel-emu/` | Nickel réel émulé dans un orb : installation, pilotage tactile, captures et journaux |

Utiliser les sous-modules épinglés `hook/NickelHook` et `hook/qrcodegen` ; ne pas
les mettre à jour implicitement. Modifier les messages dans `cli/src/messages.rs`,
puis régénérer le header C++ plutôt que de l'éditer manuellement.

## Préparer et vérifier en local

Depuis la racine, sur Linux avec `apt` et `sudo` :

```sh
.agents/setup
scripts/check
```

`.agents/setup` installe `build-essential`, `pkg-config`, `qtbase5-dev` et
`clang-format`, prépare Rust **1.99.1** avec rustfmt/clippy, initialise les
sous-modules et télécharge les dépendances Cargo verrouillées. Il prépare les
contrôles natifs Linux, **pas la toolchain de compilation pour Kobo**.

`scripts/check` n'a besoin ni d'un compte Readwise ni d'une liseuse. Il :

1. Exécute `cargo test --manifest-path cli/Cargo.toml --locked`.
2. Vérifie le formatage Rust avec `cargo fmt --manifest-path cli/Cargo.toml -- --check`.
3. Régénère `hook/src/messages.h` avec `cli/qt_codegen` ; relire le diff après exécution.
4. Compile et exécute les tests Qt de capture des sessions.
5. Exécute `scripts/check-dual-sync` : contrôleur, files, réglages et capture réels,
   avec chemins de stockage temporaires, vues Nickel et résultats CLI simulés.
6. Exécute `hook/tests/check-cli` : le vrai `hook/src/cli.cc` lance par `QProcess`
   une CLI factice (absente, non exécutable, plantée, ou émettant des erreurs JSON)
   et vérifie les dialogues, le mode silencieux et l'état des files.
7. Compile et exécute les tests de sécurité des couvertures (`hook/tests/cover.cc`).
8. Vérifie la syntaxe de toutes les sources C++ avec les bibliothèques Qt natives.

Le script ajoute `$HOME/.cargo/bin` au `PATH` et définit `VERSION=readwise-dev`
si cette variable est absente. Il utilise le toolchain Cargo actif ; si nécessaire,
imposer celui installé par le setup avec `RUSTUP_TOOLCHAIN=1.99.1 scripts/check`.
La compilation Rust requiert `VERSION` (`env!("VERSION")`). Pour un contrôle ciblé :

```sh
VERSION=readwise-dev cargo test --manifest-path cli/Cargo.toml --locked
VERSION=readwise-dev cargo clippy --manifest-path cli/Cargo.toml \
  --bin nickel-hardcover-cli --locked -- -D warnings
```

`mise run format` formate Rust et C++ ; vérifier son diff et éviter les changements
de formatage sans rapport avec la tâche. Les contrôles C++ traitent les avertissements
comme des erreurs, sauf la dépréciation `QFlags(0)` de l'amont sous Qt 5.15, qui reste
visible. Ne pas annoncer que tous les lints passent sur la seule base du contrôle
Clippy du binaire : il ne couvre pas `--all-targets`.

Les tests HTTP utilisent un serveur local simulant le contrat Readwise, **pas le
service réel**. Les contrôles natifs ne valident ni l'ABI Kobo ni le rendu. Les
écrans et hooks nécessitent `libnickel` : une liseuse compatible ou le
[Nickel émulé](#nickel-emule) ; un navigateur ne peut pas les reproduire.

## Compilation pour Kobo

Prérequis : Docker opérationnel, Git et les sous-modules. Depuis la racine :

```sh
git submodule update --init --recursive
docker build -t bookwise-nickeltc .
docker run --rm -v "$PWD:$PWD" -w "$PWD" -u "$(id -u):$(id -g)" \
  bookwise-nickeltc mise run build::package
```

Le Dockerfile part de `ghcr.io/pgaskin/nickeltc:1.0-9779f94.19` et ajoute Rust,
la cible `arm-unknown-linux-gnueabihf`, mise et resvg. `mise.toml` définit :

- `build::codegen` : génération du header Qt depuis les messages Rust ;
- `build::cli` : compilation Rust ARM en mode release, depuis `cli/` pour charger
  `cli/.cargo/config.toml` et son linker ;
- `build::hook` : génération puis compilation du plugin par NickelHook ;
- `build::package` : création de `KoboRoot.tgz` à la racine ;
- `clean` : suppression des produits de compilation.

La commande directe `mise run build` utilise l'image `strayrose/nickeltc` via la
tâche `toolchain`, pas l'image locale `bookwise-nickeltc` construite ci-dessus.
Ne pas confondre ces deux chemins de compilation.

`VERSION` (affichée dans Settings et par `cli --version`) vient de
`git describe --tags --always --dirty`. Une archive de release doit être compilée
depuis son tag, sans modification locale, pour afficher seulement `v0.1.0` ;
toute autre compilation est une version de test et affiche le commit, par exemple
`v0.1.0-3-gac38ef4` ou `…-dirty`. En compilant hors de mise, passer cette même
valeur au conteneur (`-e VERSION=…`), et supprimer `hook/libhardcover.so` et les
`.o` si elle change : `make` ne recompile pas pour un simple changement de version.

L'archive doit contenir les trois fichiers utiles suivants, sans jeton ni
configuration personnelle :

```text
usr/local/Kobo/imageformats/libhardcover.so
mnt/onboard/.adds/NickelHardcover/cli
mnt/onboard/.adds/NickelHardcover/config_example.ini
```

Contrôler l'architecture et le contenu avant distribution :

```sh
file hook/libhardcover.so cli/target/arm-unknown-linux-gnueabihf/release/nickel-hardcover-cli
tar -tzf KoboRoot.tgz
sha256sum KoboRoot.tgz
```

Les deux binaires doivent être ARM, et non Linux x86. L'archive conserve les
chemins NickelHardcover et remplace son plugin. Elle ne distribue que
`config_example.ini`, pas `config.ini`, afin de préserver la configuration existante.

<a id="etat-de-validation"></a>

### État de validation et particularités de l'environnement

Le [fil d'intégration](https://ampcode.com/threads/T-01a197ca-d75a-72f4-9df7-1cdf97aa50a5)
rapporte une archive ARM effectivement construite et installée sur une Kobo
en **4.46.23852**, une session réelle de **14 % à 15 %** et une synchronisation à
la fermeture confirmées. L'utilisateur a ensuite confirmé en mode Readwise seul
l'envoi manuel, l'envoi à la fermeture et la reprise après lecture hors ligne.
Ce constat historique ne valide pas toutes les Kobo ni les modifications futures.

Le [fil de double synchronisation](https://ampcode.com/threads/T-01a19c02-fd21-7447-bca1-618259ba3677)
a ensuite validé **30 tests Rust**, les tests Qt de capture et d'orchestration
double, les contrôles C++, `cargo test --workspace`, Clippy strict sur le binaire
et la compilation ARM/Qt 5.2.1. La nouvelle archive a été vérifiée : deux binaires
ARM EABI5 et configuration exemple, sans identifiants utilisateur.

Sur la même Kobo, l'archive du commit `02ae6dc` en mode `dual` a validé (essais
de l'utilisateur, novembre 2026, suivis dans le fil d'intégration) :

- **Sync now (both services)** : les deux services à jour, sans erreur ni doublon ;
- la synchronisation à la fermeture des deux services, une fois l'auto-sync
  activée par livre **pour chaque service** (Hardcover ne se synchronisait pas
  tant que seule l'auto-sync Readwise était active).

Lors du premier essai à la fermeture, Readwise a rejeté `POST /sessions` avec
`invalid_value` pour une session aux données normales (livre `5721999`, 40→41 %),
alors que deux sessions du même livre avaient été acceptées la veille. La même
charge utile, renvoyée une seule fois à l'identique avec l'accord de
l'utilisateur, a été acceptée (HTTP 200, `created`). **La cause reste inconnue** :
l'archive installée ne conservait que le code d'erreur. Les erreurs Readwise
conservent désormais la requête et les champs rejetés (voir
[Journaux et secrets](#journaux-et-secrets)) ; ce diagnostic n'a pas encore été
installé sur la Kobo. La documentation Readwise cite notamment, pour
`invalid_value` sur `POST /sessions`, un `book_id` désignant un extrait (*sample*) ;
aucune donnée ne le démontre pour ce livre.

**Non validé sur appareil** : reprise hors ligne en mode double, veille/réveil,
changement de livre, sessions successives, indépendance des services en cas
d'échec (seulement simulée par `scripts/check-dual-sync`), rendu de tous les
écrans. La file Hardcover reste en mémoire comme dans l'amont : sa perte au
redémarrage est une **limite héritée**, que l'essai hors ligne sans redémarrage
ne couvre pas. Ne pas présenter la persistance Readwise comme valable pour les
deux services.

Sur [Nickel émulé](#nickel-emule), l'archive de `5f4527e` en mode `dual` avec les
comptes de test a validé (novembre 2026,
[fil de revue](https://ampcode.com/threads/T-01a1d220-0a7a-768d-b795-2f82e193e992)) :

- **Sync now**, la fermeture du livre et la veille atteignent les deux services ;
- une fermeture hors ligne conserve la session Readwise en `.json`, sans
  dialogue, et un **Sync now** ultérieur l'envoie une seule fois ;
- Settings, **Update book status** et la recherche de **Manually link book**.

Ces essais ne retirent rien de la liste « non validé sur appareil ». Non couverts
sous émulation : critique et journal (HTTP 403, voir les limites de l'émulation),
`retry_on_network`, annotations, `sync_on_schedule`.

Les corrections de `ab0d832` (CLI) et `4405488` (plugin) ont ensuite été rejouées
sur Nickel émulé avec les comptes de test (14 novembre 2026, même fil de revue) :

- Hardcover seul injoignable en mode `dual` : un dialogue unique, Readwise reçoit
  sa session, la progression Hardcover attend puis part au **Sync now** suivant,
  sans doublon ;
- livre supprimé dans Readwise : `GET /books?ids=…` renvoie bien
  `status: "deleted"` ; `.json` puis `.sent` conservés avec un `GET` seul, reprise
  après restauration, une seule session créée ;
- fin de session environ 99 s en avance : Readwise répond HTTP 200 avec le rejet
  `invalid_value`, message « Invalid value for body: Value error, times must not
  be in the future » ; la session reste `.json` et part une fois l'heure passée ;
- JSON tronqué et session sans progression : renommés `.failed` à l'identique,
  session suivante intacte, Hardcover non bloqué.

Limites de ces essais : l'état `.sent` a été reconstruit à la main, un seul écart
d'horloge a été essayé (avec un décalage ajouté à `emushim.c` dans un autre orb,
non versionné), et les messages ont été reformulés après ces essais ; seuls les
refus locaux et celui du livre supprimé ont été recapturés avec leur nouveau
texte. **Une auto-sync bloquée par la file Readwise n'affiche rien** : le blocage
n'apparaît qu'au **Sync now** suivant. Ce défaut d'information est connu et non
corrigé.

L'archive du fil de double synchronisation a été compilée avec `strayrose/nickeltc:latest` (alors Cargo 1.97,
Qt 5.2.1, sans mise), et non par exécution de toute la recette Dockerfile ci-dessus.
Dans cet environnement, la compilation a utilisé depuis `cli/` :

```sh
cargo build --release --target arm-unknown-linux-gnueabihf --locked
make -C ../hook -j2
```

`VERSION` était fourni au conteneur ; la génération du header était déjà faite,
et le script d'empaquetage de `mise.toml` a été exécuté séparément. Utiliser
`sh -c` plutôt que `sh -lc` dans cette image pour conserver le `PATH` de la
toolchain. L'étiquette `latest` étant mutable, ne pas la présenter comme une
garantie de reproductibilité.

Avant de qualifier une version pour un usage régulier, vérifier sur appareil :
ouverture/fermeture, veille/réveil, changement de livre, reconnexion Wi-Fi,
progression réelle, session manuelle, reprise hors ligne et absence de doublons
côté Readwise. En mode double, vérifier aussi chaque service seul, les échecs dans
les deux sens, les associations distinctes et la conservation des fonctions
Hardcover. Ne pas attribuer aux tests natifs ni au Nickel émulé cette validation
matérielle.

<a id="nickel-emule"></a>

## Tester sur Nickel émulé depuis un orb

`scripts/nickel-emu/` exécute le vrai Nickel **4.46.23852** (Kobo Libra Colour,
`monza`, `mt8113t-ntx`) dans l'orb et y charge l'archive ARM non modifiée : QEMU
en mode utilisateur, chroot du rootfs du firmware, et `emushim.c` (`LD_PRELOAD`)
qui simule l'écran, le tactile et le contrôleur de la carte. Seul l'espace
utilisateur est émulé.

```sh
scripts/nickel-emu/setup.sh                     # une fois par orb
sudo docker build -t bookwise-nickeltc .        # une fois par orb
TC_IMAGE=bookwise-nickeltc scripts/nickel-emu/tc.sh 'mise run build::package'
scripts/nickel-emu/install.sh                   # KoboRoot.tgz -> émulation
scripts/nickel-emu/mkconfig.sh dual             # ou hardcover, readwise
scripts/nickel-emu/run.sh                       # stop.sh pour arrêter
```

- `setup.sh` est idempotent et fait ce que `.agents/setup` ne fait pas : paquets
  `qemu-user-static`, `binutils-arm-linux-gnueabihf` et `unzip`, enregistrement
  binfmt de qemu-arm, téléchargement du firmware Kobo (162 Mo, SHA-256 vérifié)
  et construction de l'émulation dans `$EMU` (défaut `~/emu`). Il démarre aussi
  Docker s'il est arrêté. `$EMU` est hors du dépôt : un nouvel orb repart de
  `setup.sh`.
- `mkconfig.sh` écrit `$EMU/onboard/.adds/NickelHardcover/config.ini` depuis les
  secrets du projet `Readwise_test_account` et `Hardcover_test_account`, avec
  `debug = true`, l'auto-sync et les annotations désactivées, `sync_on_close = 1`
  et `readwise_publish_to_feed = false`. N'utiliser que ces comptes de test ; ne
  pas copier ce fichier ni un jeton dans le dépôt, `.amp/in/` ou une sortie de
  commande.
- Après chaque compilation, relancer `install.sh` puis `run.sh` : le plugin est
  chargé au démarrage de Nickel. `install.sh` copie aussi `cli` sous le nom
  `CLI`, que lance le hook : le stockage FAT de la Kobo ignore la casse, pas l'orb.
- `run.sh` démarre le service d'orb `nickel` ; l'accueil s'affiche après 25 à
  45 s. Journal de Nickel : `$EMU/logs/nickel.log` (octets NUL : `grep -a`).

Importer un livre : Nickel ignore un `.epub` simple (pas d'Adobe RMSDK) et
n'importe qu'après un cycle USB.

```sh
python3 scripts/nickel-emu/mkepub.py "$HOME/emu/onboard/Frankenstein.kepub.epub" \
  Frankenstein "Mary Shelley"
scripts/nickel-emu/hw.sh usb plug add           # dialogue « Computer detected »
scripts/nickel-emu/touch.py tap 572 1168        # Connect
scripts/nickel-emu/hw.sh usb plug remove        # import : environ 20 s
```

Ce livre existe déjà dans les deux comptes de test (Readwise `8260170`,
Hardcover `28722`) : le réutiliser évite d'y créer d'autres livres. Il se rouvre
au début du chapitre, donc une session peut repartir d'un pourcentage plus bas :
artefact du fichier de test, pas du plugin.

Piloter et observer :

- `touch.py tap X Y`, `touch.py swipe X1 Y1 X2 Y2`, `touch.py key 116` (bouton
  d'alimentation : veille, puis réveil au second appui) ; `seq.sh "X Y attente_s" …`
  enchaîne des taps. Les coordonnées sont celles de la capture 1264×1680.
- `shot.sh <fichier.png>` capture l'écran ; `plog.sh [n]` affiche les dernières
  lignes syslog du plugin ; `in.sh <commande>` s'exécute dans l'émulation en cours.
- Confirmer chaque étape par une capture ou `plog.sh` avant le tap suivant :
  ouvrir un livre prend environ 25 s et le premier menu de lecture plus de 4 s ;
  un tap envoyé trop tôt est perdu.
- Bibliothèque : couverture du premier livre (208,550). Lecture : centre
  (632,840) pour afficher ou masquer le menu, page suivante (1150,840), icône du
  plugin (1104,128), « Back to My Books » (245,130).
- Menu du plugin en mode `dual`, x = 900 : Sync now 243, auto-sync Hardcover 334,
  auto-sync Readwise 425, Manually link 516, Update status 607, journal 698,
  critique 789, annotations 880, Settings 971. Sous-menu des statuts, x = 1000 :
  Want to Read 334, Currently Reading 425, Read 516, Paused 607, Did Not Finish 698.
- Dialogues : fermer (1183,80), page suivante (1155,1585), OK d'une erreur
  (352,1110). Recapturer ces coordonnées si l'interface ou le mode change.

Réseau et résultats :

- Nickel n'a pas de Wi-Fi et se croit hors ligne. Tant que
  `$EMU/rootfs/emu/online` existe (créé par `setup.sh`), le shim répond « en
  ligne » au plugin seulement. Supprimer ce fichier simule le hors-ligne ; le
  recréer avec `touch` rétablit la connexion.
- Le signal `networkConnected` de Nickel n'est jamais émis : `retry_on_network`
  n'est pas testable. Déclencher la reprise par **Sync now**.
- Simuler une panne de Hardcover seul : écrire `hosts: files dns` dans
  `$EMU/rootfs/etc/nsswitch.conf` et `127.0.0.1 api.hardcover.app` dans
  `$EMU/rootfs/etc/hosts`. La CLI échoue alors en `Network` sans toucher
  Readwise. Supprimer les deux fichiers pour rétablir. Ce procédé n'a pas été
  essayé sur `readwise.io`.
- Vérifier côté services sans afficher de jeton :
  `scripts/nickel-emu/rw.py GET '/books/api/v1/sessions?limit=100'` et
  `scripts/nickel-emu/hc.py '<requête GraphQL>'`. La file et les journaux de la
  CLI sont dans `$EMU/onboard/.adds/NickelHardcover/` ; la base Kobo est
  `$EMU/onboard/.kobo/KoboReader.sqlite` (module `sqlite3` de Python, l'orb n'a
  pas la commande `sqlite3`).
- Ces essais écrivent sur les comptes de test réels : la
  [récupération des sessions](#recuperation-des-sessions-readwise) s'applique,
  ne jamais renvoyer une session au résultat incertain.

Limites de l'émulation :

- Le jeton Hardcover de test n'a pas les droits `write:reviews` et
  `read:journal` : critique et journal échouent en HTTP 403, sans défaut du plugin.
- Non émulés : rendu et rafraîchissement e-ink, veille matérielle, horloge RTC,
  Wi-Fi, autres modèles et firmwares. L'émulation complète de la carte (noyau,
  pilotes) n'est pas possible dans un orb.

## Configuration et invariants de double synchronisation

- Le menu de gestion du livre reste celui de Hardcover en mode `dual` : OAuth/QR,
  catalogue/éditions, statuts dont pause, évaluations/critiques avec leurs options,
  annotations et journal. Les limites de l'API Readwise ne doivent pas masquer
  ces fonctions. Les statuts et critiques saisis dans ce menu ne sont pas copiés
  vers Readwise ; le mode Readwise seul conserve sa propre gestion du livre.
- `Settings::isReadwise()` signifie **Readwise seul**, pas « Readwise actif ».
  Utiliser `usesService(true)` ou la disponibilité de la file Readwise pour décider
  de capturer et synchroniser des sessions en double mode.
- `SyncController` possède deux `SyncQueue`, chacune liée à un service. Conserver
  leurs états d'envoi, d'erreur, de progression et de reprise indépendants. Une
  panne ou un compte déconnecté ne doit pas empêcher l'autre service de fonctionner.
- Les jetons Hardcover (`access_token`, `refresh_token`, `token_expires_at`) ne
  doivent pas être remplacés par `readwise_token`. Les entrées Hardcover de
  `library.ini` restent sans préfixe ; Readwise utilise `readwise/`. Passer
  explicitement le service aux accès de synchronisation, sans changer globalement
  le fournisseur pour exécuter une requête.
- L'activation automatique est propre à chaque livre **et service**. Les seuils,
  l'heure quotidienne et `retry_on_network` sont communs, mais évalués avec la
  progression du service concerné. La désactivation Hardcover à la fin du livre
  ne doit pas désactiver Readwise. Un envoi manuel ne requiert pas l'auto-sync.
- Hardcover conserve le minimum de 2 %, l'arrondi de 99 % à 100 % et la
  désactivation de son auto-sync à 100 %. Ne pas appliquer ces règles à Readwise.
- La file de reprise Hardcover reste en mémoire comme dans l'amont. La persistance
  et les états anti-doublons décrits ci-dessous concernent Readwise uniquement.
  Ne pas reconstruire des sessions Readwise à partir de l'historique Hardcover.
- Le hook lance `update --service hardcover` et `update --service readwise` avec
  les associations propres au service, dans des processus indépendants. Un
  callback réseau répété ne doit pas démarrer deux fois la même commande.
- Un appel CLI `update` sans `--service` utilise `provider`. En mode `dual`, il
  tente les deux services, même si le premier échoue, puis rapporte leurs erreurs.
  `--linked-id` appartient alors à Hardcover ; Readwise utilise les associations
  de ses sessions et de son cache. Avec `--service`, cet identifiant appartient au
  service explicitement ciblé. Ne jamais persister ce choix ponctuel dans `config.ini`.
- Le `error_code` d'une erreur CLI décide de l'action du hook (`hook/src/cli.cc`) ;
  les variantes sont dans `ExpectedError` (`cli/src/utils.rs`). `Unauthorized`
  ouvre le dialogue de connexion, `BookNotFound` propose de lier ou délier le
  livre du fournisseur principal (Hardcover en mode `dual`) et `InvalidConfig`
  propose une réinitialisation : la commande `update` de Readwise n'émet pas ces
  codes mais `ReadwiseAuth`, `ReadwiseBookNotFound` et `ReadwiseSync`, affichés
  sans action. `Network` (pannes de transport, délai de 30 s par requête, HTTP
  408/500/502/503/504 côté Hardcover) est le seul code qui conserve la
  progression en attente pour `retry_on_network` ; tout autre échec la retire de
  la file en mémoire du hook. Une synchronisation silencieuse n'ouvre aucun
  dialogue, quel que soit le code.
- Une reprise de la file Readwise ne doit ni rejouer une session acceptée ni
  acquitter la progression d'un intervalle encore actif. Les résultats incertains
  bloquent Readwise seulement, pas Hardcover.
- En double mode, les associations Readwise existantes sont conservées. Pour en
  modifier une via le menu de recherche, passer temporairement en mode Readwise
  seul puis revenir à `dual`, en fermant et rouvrant le livre à chaque changement.
  Les sessions déjà sauvegardées gardent leur association.

## Configuration et invariants Readwise

- Sur Kobo, les données du plugin sont sous `/mnt/onboard/.adds/NickelHardcover/`.
  La CLI lit `config.ini` à côté de son propre exécutable. La base Kobo par défaut
  est `/mnt/onboard/.kobo/KoboReader.sqlite`.
- `provider` accepte `hardcover`, `readwise` ou `dual`. Le jeton personnel Readwise vient
  de <https://readwise.io/access_token> et va dans `readwise_token`, pas
  `access_token`. Il reste en clair sur le stockage Kobo ; ne jamais le passer
  en argument de processus, l'inclure dans une archive ou l'afficher dans les logs.
- La connexion QR/OAuth du mode Hardcover ne constitue pas une connexion Readwise.
- Les valeurs de repli sont dans `cli/src/config.rs` et `hook/src/settings.*`.
  Ne pas les confondre avec l'exemple distribué : les reprises réseau, annotations
  et déclencheurs de synchronisation peuvent être désactivés si les clés manquent.
- `auto_sync_default = false` laisse l'activation à chaque livre. Les seuils
  `sync_on_close` et `sync_on_read` sont en points de pourcentage (1–99, `-1`
  désactive) ; `sync_on_schedule` est une heure de 0 à 23 (`-1` désactive).
- `readwise_publish_to_feed` vaut `true` par défaut. Respecter un `false` explicite
  existant. Chaque session conserve son choix de publication, même si le réglage
  change ensuite. `false` exclut le fil, pas les statistiques ou classements.
- `library.ini` sépare les réglages, associations et progressions Readwise avec
  le préfixe `readwise/` ; ne pas les confondre avec ceux de Hardcover. Après un
  changement de fournisseur ou d'association, fermer puis rouvrir le livre.
- Une association manuelle est un **ID de livre de la bibliothèque Readwise**,
  pas un ID de produit du catalogue. La recherche est limitée à cette bibliothèque.
- Avant d'envoyer une session, créer/résoudre le livre par `POST /books` avec
  `status: currently_reading`, ou vérifier un ID connu par `GET /books?ids=…`.
  Si la vérification échoue, conserver la session sans l'envoyer.
- L'identification utilise ISBN/ASIN, sinon titre/auteur. Sans ISBN, une première
  synchronisation fournissant le titre et l'auteur peut être nécessaire avant
  une notation. Le cache `readwise.sqlite` conserve aussi les livres sans
  correspondance catalogue pour éviter leur recréation.
- Capturer les intervalles dans la vue de lecture ; les fermer à sa sortie,
  à la veille, au changement de livre ou à une synchronisation. Commencer un
  nouvel intervalle si la lecture continue. Ne pas inventer de durée entre deux
  connexions Wi-Fi et ne pas compter le temps hors de la vue de lecture.
- Persister à la fermeture seulement si Readwise est actif et connecté et si
  l'auto-sync Readwise du livre est activée, ou si la fermeture de l'intervalle
  est déclenchée par une synchronisation manuelle. Activer seulement l'auto-sync
  Hardcover ne suffit pas. `Sync now` envoie aussi les sessions Readwise persistées
  des autres livres.
- Convertir la progression Kobo entière 0–100 en fraction 0–1. Ignorer les
  intervalles sans avancée, y compris les retours en arrière. Ne pas arrondir
  99 % à 100 % ; le statut terminé automatique exige exactement 1.0.
- Les horodatages sont en UTC ; le fuseau du profil Readwise est utilisé côté
  service. La vue laissée ouverte sans lecture et les sauts de pages peuvent
  fausser la mesure. Un arrêt brutal peut perdre l'intervalle actif, pas les
  sessions déjà persistées.
- Le widget Qt note par demi-étoile ; en mode Readwise seul, la CLI accepte aussi
  les quarts d'étoile autorisés par l'API.

<a id="recuperation-des-sessions-readwise"></a>

## Récupération des sessions Readwise

**`POST /sessions` n'est pas dédupliqué par Readwise.** Ne pas promettre une
livraison « exactement une fois » ni réessayer aveuglément après une erreur réseau.
La file persistante est `.adds/NickelHardcover/readwise-sessions/` :

| Extension | Signification | Action |
| --- | --- | --- |
| `.json` | Non envoyée ou refusée avec reprise possible (401/403/429, fin de session dans le futur) | Corriger le jeton/l'accès ou l'horloge, respecter `Retry-After`, puis synchroniser |
| `.sending` | Envoi commencé, résultat incertain ou interruption | Vérifier dans Readwise ; aucune relance automatique |
| `.sent` | Session acceptée, statut du livre restant à mettre à jour | Synchroniser ; seul le `PATCH` du livre est rejoué |
| `.failed` | Rejet explicite, même si HTTP 200, ou fichier `.json` illisible ou invalide | Lire le motif (requête, code, champ, ou nom du fichier) dans le message d'erreur ou `nickelhardcover_*.log`, corriger, puis remettre le fichier en `.json` |

Un `.sending` ou `.failed` bloque la file Readwise, **pas Hardcover**, pour éviter
de réappliquer une ancienne progression après une plus récente. Le verrou `readwise-sync.lock` empêche des
vidages concurrents. Les délais de reprise après un 429 sont séparés :
`readwise-books-retry-at` pour l'API Books et `readwise-highlights-retry-at` pour
les surlignages (API v2). Un ancien `readwise-retry-at` reste respecté par les
deux jusqu'à son expiration.
Préserver l'ordre chronologique et la persistance de l'accusé de réception avant
la mise à jour du statut du livre.

Trois cas ne sont pas des rejets définitifs et ne doivent pas le devenir :

- **Horloge en avance.** Une session dont la fin est dans le futur n'est pas
  envoyée. Si Readwise la rejette avec `invalid_value` et le message « times must
  not be in the future », elle reste `.json` avec ses dates d'origine. Tout autre
  libellé reste `.failed` : ne pas élargir cette reconnaissance sans rejet observé.
- **Livre supprimé dans Readwise.** Pour une association déjà connue, la CLI lit
  le statut avant d'envoyer une session ou de rejouer le `PATCH` d'un `.sent` et
  refuse de restaurer un livre `deleted`. Les sessions sont conservées et
  bloquent la file jusqu'à une restauration explicite par l'utilisateur. Une
  suppression entre ce `GET` et le `PATCH` n'est pas couverte, ni la première
  résolution d'un livre sans cache.
- **Fichier illisible ou invalide.** Il est renommé `.failed` sans modification
  de ses octets. Le remettre en `.json` sans le corriger le renomme à nouveau.

Avant toute intervention, désactiver les synchronisations automatiques, attendre
la fin d'un éventuel envoi, connecter la Kobo en USB et sauvegarder les fichiers.

1. Si une session `.sending` existe dans Readwise, la renommer en `.sent` en
   conservant son `book_id`. Si sa création n'a **certainement pas** eu lieu,
   la renommer en `.json`. En cas de doute, ne pas la renvoyer.
2. Pour corriger l'association d'une session non envoyée, renseigner `linked_id`
   avec l'ID du livre Readwise approprié. Ne pas réaffecter une session acceptée.
3. Une création de livre au résultat incertain bloque aussi les nouvelles
   créations pour ce livre. Le retrouver dans Readwise, l'associer manuellement
   puis lancer **Sync now**, plutôt que supprimer aveuglément son cache.
4. Pour une session `.failed`, noter le message d'erreur complet : il indique la
   requête, le code et le champ rejetés, sans jeton. Corriger la cause (par
   exemple une mauvaise association, via `linked_id`), puis renommer le fichier
   en `.json` et lancer **Sync now**. Le rejet `invalid_value` décrit dans
   l'[état de validation](#etat-de-validation) a été accepté à l'identique lors
   d'une relance unique : ne pas en conclure qu'une relance suffit en général.

Les erreurs CLI renvoient vers le README : y conserver un avertissement et un
lien vers cette procédure même si les détails techniques restent ici.

## Changer de compte Readwise

Changer le jeton n'effectue **pas** une migration de compte. Avant le changement :

1. Terminer ou résoudre les envois de l'ancien compte.
2. Désactiver les synchronisations automatiques et sauvegarder le dossier du plugin.
3. Retirer la file `readwise-sessions/` et le cache `readwise.sqlite` de l'ancien compte.
4. Effacer ses entrées `readwise/` dans `library.ini`, sans toucher à celles de Hardcover.
5. Renseigner le nouveau jeton, éjecter la Kobo et rouvrir le livre avant de
   réassocier les livres et de réactiver la synchronisation.

## Diagnostic et tâches qui touchent la liseuse

- `mise run logs` lit les logs Kobo via SSH et filtre `NickelHardcover`.
- `mise run install` compile, copie l'archive par SSH et **redémarre la Kobo**.
  Ne pas l'utiliser comme simple vérification de compilation.
- Ces tâches requièrent `KOBO_SERVER` et `KOBO_PASSWORD`, ainsi que les outils
  employés dans `mise.toml` (`sshpass`, et `pv` pour l'installation). Ne pas
  versionner les identifiants. Certains firmwares imposent un `ForceCommand`
  SSH incompatible avec les commandes distantes ; le commentaire de `mise.toml`
  décrit ce point. Ne pas modifier la configuration SSH sans accord explicite.
- `mise run dump <fichier>` extrait les symboles bruts et démanglés de `libnickel`.
- `mise run schema` récupère le schéma GraphQL Hardcover avec
  `HARDCOVER_AUTHORIZATION` ; cette tâche ne concerne pas Readwise Books.

### Journaux et secrets

- La CLI met en mémoire **tous les niveaux** de log (`set_max_level(Debug)`,
  `BufferedLogger::enabled` toujours vrai) et écrit `nickelhardcover_*.log` à côté
  de son exécutable après toute erreur attendue ou panique, **même avec
  `debug = false`** ; `debug` n'ajoute que l'écriture après un succès. Les messages
  INFO sont aussi transmis au syslog par le hook. Traiter tout `log::*` comme
  persistant.
- `hardcover::handle_response` masque `access_token`, `refresh_token`, `id_token`,
  `device_code` et `user_code` avant de journaliser un corps de réponse ou de
  l'inclure dans une erreur ; un corps non JSON, éventuellement une réponse OAuth
  tronquée, n'est rapporté que par sa taille. `OAuthSet` a un `Debug` masqué et
  `send_msg!` ne journalise pas le contenu d'`OAuthDevice`. Ne pas réintroduire
  `log::debug!("{body}")` ni `derive(Debug)` sur des arguments secrets. ureq
  masque lui-même les en-têtes sensibles dans ses logs.
- Une erreur Readwise conserve la méthode, le chemin sans paramètres, puis le
  `code`, le `message` et chaque `details[]` (`location`, `field`, `code`,
  `message`) de l'objet Error documenté, y compris pour les réponses HTTP non 200.
  Ce texte est échappé (HTML des dialogues Nickel) et tronqué à 400 caractères.
  Readwise garantit que `message` ne répète pas les valeurs envoyées ; ne jamais y
  ajouter le corps de requête, les en-têtes ni le jeton.
- Les logs produits par les archives antérieures à ces corrections, dont celle de
  `02ae6dc`, peuvent contenir des jetons OAuth Hardcover : ne pas les partager.
- Même masqués, les logs contiennent les titres des livres et des données du
  compte Hardcover : les relire avant de les joindre à un rapport de bug.

Ne pas joindre de jeton, de base utilisateur ou de file de sessions privée à un
rapport de bug. Toute installation sur une vraie Kobo ou écriture sur un compte
Readwise réel nécessite l'accord de son propriétaire.
