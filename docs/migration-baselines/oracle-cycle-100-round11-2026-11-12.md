# Cycle 11 — entrée d'un permanent vers une séquence d'effets

Ce cycle remplace une nouvelle tranche de déclenchements d'entrée par un parent
générique `permanentEntered → EffectSequence`. L'événement conserve séparément
le prédicat d'objet, le contrôleur, l'exclusion éventuelle de la source et le
corps d'effets. Il ne crée ni opération combinée, ni branche nommée d'après une
carte, ni liaison implicite vers l'objet entré.

Tatyova, Benthic Druid est la preuve verticale exacte : l'entrée d'un terrain
contrôlé gagne 1 point de vie puis pioche une carte; un terrain adverse ne
déclenche rien. Les mêmes frontières couvrent notamment Soul Warden, Wirewood
Savage et Aura Shards avec leurs portées respectives.

## Garde grammaticale

La requête d'objet réutilise le sélecteur partagé et exige à la fois le domaine
des permanents et une vraie tête nominale. Un sous-type comme `Beast` demeure
valide, sans être transformé en `Creature`; `legendary` seul reste refusé. Les
sept exclusions importantes vivent aussi dans une table JSON NativeOnly :
anaphore d'événement `it`, sujet composé, quantificateur `one or more`, événement
composé, contrôleur `your team`, supertype nu et portée optionnelle multi-action.

Les cas possédés passent 5/5 en NativeOnly et 5/5 en Full. Les sept exclusions
passent 7/7 en NativeOnly. Deux exclusions volontairement `NotClaimed` restent
reconnues par une ancienne famille en Full; elles ne sont donc pas artificiellement
forcées à échouer dans ce mode.

## Preuves

- Parseur FAAC58D9 : 152/152 cas, 32 fixtures, zéro régression.
- Runtime exact et siblings : 10/10 scénarios, 60 assertions; neuf scénarios
  exacts couvrent Tatyova, Warden, Wirewood et Aura Shards, puis le sibling sans
  cible complète la garde. Tatyova représente 14 assertions et le groupe
  générique 46.
- Le sibling sans cible légale est correctement retiré après
  `commit_pending_triggered_targets`; l'événement historique d'enqueue n'est pas
  utilisé comme preuve erronée d'une capacité restée en pile.
- Revalidation ciblée partagée : la fixture auparavant rouge passe 5 assertions;
  les huit tests unitaires dédiés passent aussi. Le gate runtime final couvre
  380/380 scénarios et 2 364 assertions en 1,615 s, après 0,204 s de build cache.
- Contrats du dépôt : 223/223. Contrats générés : 119/119 capacités gagnées,
  y compris les cartes partielles, puis 51/51 cartes complètes.
- Architecture : zéro violation et quatre avertissements historiques.

La revalidation dynamique d'une cible dont le type ou le contrôleur change
avant la résolution est maintenant partagée entre la résolution et le changement
de cible. Les 15 nouvelles capacités ciblées ont toutes une seule décision avec
`minimum = maximum = 1`; leur frontière runtime est certifiée. Cette vérification
de manifeste ne remplace toutefois pas quinze preuves runtime individuelles.

## Généralisation mesurée

Le replay immuable couvre 39 288 objets et 67 420 capacités. Les 14 533
capacités natives antérieures conservent exactement leur identité, texte, ordre,
type, IR et diagnostics. Le parent ajoute 119 capacités natives et 51 objets
entièrement reconnus par le parseur natif, sans régression. La population reconnue
complète est de 50 cartes ordinaires de layout `normal` et du jeton Zabu. Les
preuves runtime verticales nommées portent sur quatre cartes — Tatyova, Soul
Warden, Wirewood Savage et Aura Shards — plus des siblings; elles ne certifient
pas individuellement les 51 objets.

La correction ciblage conserve trois limites : les slots illégaux d'une vraie
sélection multi-cible ne sont pas encore filtrés un par un, la génération de
candidats de zone reste bornée aux références prises en charge et certains
prédicats d'événement lisent encore des caractéristiques imprimées.

Les sous-corpus confirment le même résultat : +1 carte dans le lot round9,
+2 dans la file de 450 et +8 dans les sept ensembles locaux. Decorated Champion
reste explicitement non supportée parce que `your team` n'est pas approximé en
`you control`.

## Lot suivant

Le script officiel conserve les 99 cartes encore non couvertes, retire Tatyova
et ajoute Farewell depuis la priorité. Le lot round10 respecte les quotas
34 récentes / 33 priorité / 33 ligue. Ses empreintes sont :

- lot : `3508AB53CF3483AF9AEF7A0F137BF8C9D40A66C335CD890E6C89B1E1F89821F0`;
- entrée : `00FFCC4FCBA8029723D10FC955F6E9F82FF190D022F993734B45DA254D3F5136`;
- audit FAAC : `38F4574F3E97F227D9EE4985BEEB227FEBADA6D7667345F2C6B6A8F29E7A97BF`.

Aucun correctif atomique confirmé n'achève plus d'une carte du nouveau lot. La
réduction statique de coût de Goblin Anarchomancer gagne le départage : une
carte complète dans le lot, zéro simple gain partiel, mais environ 35–37 formes
globales à réévaluer. Return of the Wildspeaker n'est pas comptée comme un gain
d'un seul correctif : son parent modal existe, mais il manque encore deux
enfants indépendants (agrégation de la plus grande force et modification d'une
collection non-Human). Farewell exige également plusieurs extensions distinctes.

## Temps chronologiques

L'ordre ci-dessous suit l'heure UTC de dernière écriture des artefacts. Il décrit
l'ordre de fin observé, pas nécessairement l'ordre de début de travaux qui ont pu
se chevaucher. Les deux boucles correctives ne portent pas un horodatage autonome;
leur position n'est établie que par les artefacts qu'elles séparent.

| Étape | Fin observée (UTC) | Temps | Description |
|---|---:|---:|---|
| Intégration initiale | ordre interne non établi | 115,098 s | Première fenêtre tracker : parent, lowering, sélecteur et fixture initiale |
| Boucle corrective 1 | entre intégration et build | — | `has_domain` remplacé par la tête nominale partagée |
| Compilation parseur | 20:30:41 | 30,209 s | Construction FAAC58D9, une seule fois |
| Première exécution | ordre interne non établi | 0,726 s | Schéma de diagnostic incomplet; le binaire était valide |
| Boucle corrective 2 | avant 20:37:18 | — | Ajout du diagnostic structuré à la fixture invalide |
| Liaison analyseur | 20:39:39 | 1,433 s | Liaison directe au rlib frais, sans Cargo |
| Audit lot / 450 / sept sets | 20:39:50–20:39:53 | 0,515 / 0,555 / 2,418 s | Audits immuables des sous-corpus |
| Audit global | 20:41:34 | 34,407 s mur | 28,259 s d'exécution analyseur |
| Runtime Tatyova | 20:43:07 | 0,117 s | 2 scénarios, 14 assertions |
| Contrats capacités | 20:44:22 | 13,033 s | Génération + 0,100 s d'exécution pour 119 cas |
| Contrats cartes | 20:45:27 | 13,633 s | Génération + 0,031 s d'exécution pour 51 cas |
| Runtime siblings final | 20:49:58 | 0,058 s | 8 scénarios, 46 assertions |
| Refill round10 | 20:50:31 | 1,479 s | 99 retenues, Farewell ajoutée |
| Audit round10 | 20:50:43 | 0,260 s mur | 100 cartes, 204 capacités |
| Vérification globale indépendante | 20:51:06 | — | Manifeste final +119 capacités / +51 cartes, sans perte |
| Régression parseur finale | 20:55:06 | 0,481 s | 152 cas, 32 fixtures, sans Cargo; postérieure aux premiers audits et au refill |
| Build revalidation ciblée | 21:01:49 | 50,238 s | Runner moteur; fixture rouge devenue verte, 5 assertions |
| Unitaires revalidation | 21:03:52 | 39,760 / 1,444 / 0,01 s | Compilation / wrapper / harnais, 8/8 tests |
| Gate runtime final | 21:04:46 | 0,204 / 1,615 s | Build cache / exécution, 380 scénarios et 2 364 assertions |
| Contrats finaux | 21:04:51 | 2,311 / 1,821 s | Build / exécution, 223/223 contrats |

La fenêtre d'intégration initiale n'inclut pas les corrections sémantiques et de
fixture ultérieures, dont la durée active n'a pas été minutée séparément. Le
tracker affiche 1 715,578 s dans la phase `focused-tests`, mais cette fenêtre
inclut principalement l'attente coordonnée des lanes root et des revues. Elle
n'est pas présentée comme du temps de codage ou d'exécution.

Artefacts principaux : `.tmp/permanent-entered-global-root-verification-FAAC58D9.json`,
`.tmp/permanent-entered-global-contract-run-FAAC58D9.json`,
`.tmp/permanent-entered-full-card-contract-run-FAAC58D9.json`,
`.tmp/target-legality-runtime-final.json`,
`.tmp/target-legality-unit-timing.json`,
`docs/migration-baselines/declared-target-legality-2026-11-12.md` et
`.tmp/card-migration-timings/permanent-entered-tatyova.json`.
