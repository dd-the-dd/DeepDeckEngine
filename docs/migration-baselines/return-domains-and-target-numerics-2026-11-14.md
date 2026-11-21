# Domaines de cibles et références numériques — 14 novembre 2026

Checkpoint parser EEE accepté après revue et preuves ciblées. Le corpus local
contient désormais **3 568 cartes non vides entièrement natives et 17 224 capacités
natives**, soit +4 cartes/+5 capacités depuis BA3F. Les 17 219 anciennes IR sont
structurellement identiques, avec comparaison sensible aux types et à la casse.
Les quatre cartes sont Venser, Shaper Savant ; Sink into Stupor // Soporific Springs ;
Unsubstantiate ; Reprieve. Failure // Comply gagne une capacité, pas sa carte entière.

Les dix sets locaux figés restent à 287 cartes complètes/1 525 capacités natives ;
aucun gain supplémentaire dans cette sélection. Les 3 144 entrées sans capacité
sont comptées séparément. Depuis le départ de la refonte : +159 cartes et
+350 capacités natives. Ces mesures certifient le parsing, pas la simulation
individuelle de toutes ces cartes.

## Frontières partagées

- Le prédicat de sélection existant est projeté en domaines sort/permanent pour
  Return. Une cible mixte conserve une seule décision avec candidats union ; les
  contrôleurs de branches restent dans leur portée. Les requêtes homogènes
  conservent leur IR antérieure. La distribution mixte est bornée à 32 alternatives
  et signale explicitement son dépassement. Les filtres numériques de pile restent plafonnés.
- La capacité de lire des cibles de pile est explicite dans le contexte de
  reconnaissance. Les anciens lecteurs gardent Battlefield par défaut. Seuls les
  parents utilisant le lowerer partagé vérifié activent SpellOrBattlefield.
  Cette seconde revue a fermé une fuite de domaine vers d'anciens parents.
- Le parent zone-change/life conserve son enveloppe compatible mais utilise le
  lowerer de séquence partagé ; il ne duplique plus les décisions et les actions.
- Une référence numérique qualifiée garde son nom de type jusqu'à validation
  contre la cible originale. La même récursion arithmétique abaisse les feuilles
  liées ou indépendantes. P/T exigent une cible garantie Creature ; la mana value
  accepte son domaine plus large. Les possessifs ASCII/Unicode et majuscules
  conservent les octets et plages du texte original.
- Toute lecture de propriété après changement de zone reste plafonnée. Feed the
  Swarm n'est pas migrée : sa reproduction moteur perd encore la mana value de
  la cible détruite. Ni une prélecture inconditionnelle ni zéro ne prouvent la LKI.

## Preuves et temps

476 cas DATA parser passent dans chacun des modes NativeOnly et Full : 475 dans
le gate principal puis un cas de séquence à deux cibles mixtes. Celui-ci vérifie
targetObject1/targetObject2 distincts. Privés ciblés : numeric66, domains44,
lexical5, arithmetic16, graveyard3 ; ces partitions se recoupent et ne s'additionnent pas.

Neuf fixtures moteur utilisent l'IR réellement émise, comparée sans normalisation :
sept scénarios Return et deux références numériques vivantes synthétiques.
61 assertions passent sans build en 0,060 s. Les scénarios Return distinguent
spellReturned de spellCountered et contrôlent les cibles interdites. Les deux
scénarios numériques ne constituent pas une preuve de Feed the Swarm.
372 contrats de cartes/IR passent ; l'artefact moteur demeure en cache,
features=[], lors de leur build ciblé de 2,319 s, exécution 0,763 s.

Build release final : 28,948 s ; link analyseur : 1,328 s. Gate parser final :
compilation 14,148 s, exécution 3,326 s ; Full sans rebuild : 0,622 s. La mesure
native chevauchait le replay global, donc elle ne mesure pas une régression de
vitesse isolée. Le replay global de ce passage prend 16,537 s dans ce même contexte.
Deux boucles correctives root sont conservées : transfert de propriété Rust
object.take(), puis remplacement du lowerer spécialisé qui masquait les diagnostics
LKI. Les premiers échecs ne sont pas réécrits. La garde architecture trouve
48 kinds primitifs, zéro kind composite, zéro violation et zéro avertissement.

Analyseur immuable : `.tmp/night-return-target-numeric-02/after-analyzer.exe`, SHA
`EEE422D3BF64A86A538BC871FD197FDA1CFBCA5EFF637D3E49F279677D92C6AB`.
Preuves : `accepted-source-freeze.json`, `global-comparison.json`,
`return-candidate-comparison.json`, `ten-set-replay/comparison.json`,
`runtime-emitted-root.json`, `contracts-root.json` dans le même dossier.
Liens des entrées réelles/IR et hash :
`.tmp/night-return-domain-optin-review-01/proof-links/emitted-runtime-links-EEE.json`.
Le candidat B42D intermédiaire reste conservé, sans être un checkpoint accepté.

La cohorte précédente de 100 contient désormais deux cartes complètes : Garruk's
Uprising et Sink. Le refill proposé conserve exactement les 98 autres et ajoute
Impact Tremors et Rapid Hybridization. La migration et les correctifs runtime
source/incarnation et cible/LKI continuent ; les caps correspondants restent actifs.
