# Cycle 4 — déplacement ciblé de cartes entre zones

Eternal Witness, Buried Ruin et Noxious Revival sont maintenant compilées par
un même sous-automate token/FSM de déplacement ciblé. Le parent choisit le
contexte — déclenchement, activation ou sort — tandis que l'enfant reconnaît le
sélecteur de carte, la zone source et la destination. L'IR reste composée de
`chooseTargets`, `moveCards` et, pour Eternal Witness, `optionalEffects`.

## Frontière partagée

Les formes promues sont :

- `Return target … card from your graveyard to your hand` ;
- `Put target … card from a graveyard on [the] top of its owner's library` ;
- les mêmes actions après une vraie frontière d'activation `COSTS: EFFECT`.

Le sélecteur réutilise le prédicat objet partagé ; `card` seul représente le
domaine Card, tandis que les types et leurs unions restent des prédicats. Les
cibles reçoivent des identifiants ordinaux (`targetCard1`, `targetCard2`) dans
une séquence. Une cible Card ne peut pas satisfaire silencieusement une anaphore
de Permanent telle que `untap it`.

L'optionalité ETB actuelle est volontairement bornée à un seul effet atomique.
Une phrase comme « you may return … . Draw a card. » est refusée avec l'état
`trigger.source-entry-sequence.optional-scope` plutôt que de rendre la pioche
optionnelle. Une future généralisation pourra représenter Optional comme enfant
typé de la séquence.

## Trois preuves verticales

- Eternal Witness : cible obligatoire lors de la mise en pile, choix `may` à la
  résolution, acceptation, refus et absence de cible légale.
- Buried Ruin : paiement `{2}`, engagement, sacrifice, filtre Artifact et
  déplacement de son propre cimetière vers sa main.
- Noxious Revival : cible dans n'importe quel cimetière, revalidation à la
  résolution, propriétaire résolu avant déplacement et carte placée au-dessus
  de la bibliothèque de ce propriétaire. Le champ `from` est omis, car le
  moteur ne peut pas exécuter une zone abstraite `anyGraveyard` comme origine.

Résultats finaux :

- parseur NativeOnly : 88/88 cas, zéro fixture invalide ; 12 cas appartiennent
  directement à cette famille ;
- contrats permanents du dépôt : 170/170, dont les trois contrats complets de
  cartes ;
- runtime ciblé : 7/7 scénarios, 49 assertions ;
- IR parser = IR contrat pour toutes les capacités des trois cartes ;
- IR de la capacité exécutée = IR de la vraie source runtime pour chaque carte.

Le moteur revalide la zone déclarée des cibles Card pour les zones prises en
charge ; ce correctif ne généralise pas encore la revalidation des prédicats
dynamiques. Le cas d'un déplacement aller-retour vers la même zone demeure un
gap moteur documenté ; il n'est pas annoncé couvert par ce cycle.

## Gate global et correction de collision

Le premier replay global du snapshot `87D9BC87…` a détecté sept capacités
natives perdues. Les claims étaient trop larges : ils interceptaient notamment
Karakas et Seal of Removal (`Return target creature … owner's hand`) ainsi que
Reanimate (`Put … onto the battlefield`). L'automate de déplacement n'était pas
en cause ; sa frontière de promotion l'était.

Les claims ont été resserrés sur `card(s) from` et sur la destination propre à
la famille. Des fixtures exactes Karakas/Reanimate et un near-miss depuis l'exil
protègent cette frontière. Le snapshot final
`9DFFE7AAB863D3A980387CB8C4CDCE6CC695CCE1209E81CA34CA4C5159465A45`
conserve les 14 302 capacités natives antérieures, sans perte ni changement
d'IR.

## Généralisation mesurée

| Corpus | Capacités antérieures préservées | Nouvelles capacités natives | Nouvelles cartes entièrement natives | Régressions |
|---|---:|---:|---:|---:|
| Lot round 4, 100 cartes | 27/27 | 3 | 3 | 0 |
| File rotative, 450 cartes | 239/239 | 3 | 3 | 0 |
| Sept ensembles, 2 069 cartes | 1 120/1 120 | 1 | 0 | 0 |
| Global local, 39 288 cartes | 14 302/14 302 | 67 | 47 | 0 |

Le gain des sept ensembles est la capacité ETB de Graveshifter ; la carte garde
un autre bloqueur. Les 47 gains globaux incluent notamment Regrowth, Recollect,
Raise Dead, Auramancer, Gravedigger et plusieurs activations de récupération.
Ils prouvent la généralisation de reconnaissance/IR. Les preuves runtime
verticales nouvelles de ce cycle restent les trois cartes prioritaires.
Les règles complètes réelles des 47 gains passent aussi `rule_is_executable`
dans une population générée supplémentaire (47/47, distincte des 170 contrats
du dépôt). Cette preuve confirme un IR accepté par le moteur ; elle ne remplace
pas 47 scénarios comportementaux individuels.

Par layout, ces gains sont 44 `normal`, un `modal_dfc`, un `prepare` et un
`vanguard`. Gix est l'objet Vanguard du set `pvan` : le gain global est donc 46
cartes ordinaires et une variante, sans objet vanilla/zero-ability. Il ne faut
pas ajouter 47 au dénominateur des cartes ordinaires.

## Temps mesurés

| Étape | Temps | Description |
|---|---:|---|
| Implémentation initiale | 896,667–900,497 s | Trois trackers parallèles du même concept ; inclut coordination et revues |
| Build combiné, essai 1 | 40,312 s | Échec de compilation : import et type de cardinalité, conservé séparément |
| Build combiné, essai 2 | 48,993 s | Parseur, contrat et moteur liés avec succès |
| Runtime ciblé | 0,114 s | 7 scénarios, 49 assertions |
| Build du correctif de claims, essai 1 | 7,293 s | Échec de compilation dans le snapshot partagé |
| Build du correctif de claims, essai 2 | 14,619 s | Compilation réussie ; la fixture invalide a ensuite été corrigée sans rebuild |
| Runner NativeOnly final | 0,252 s | 88/88 cas, aucun Cargo |
| Liaison analyseur final | 1,256 s | `rustc` direct contre le rlib 9DFF |
| Replay du lot de 100 | 0,127 s | 200 capacités |
| Replay de la file de 450 | 0,471 s | 837 capacités |
| Replay des sept ensembles | 2,062 s | 4 300 capacités |
| Replay global | 30,022 s | 67 420 capacités |

Quatre boucles correctives sont enregistrées par carte : portée de
l'optionalité, domaine de l'anaphore, erreurs mécaniques du premier build et
collision découverte par le gate global.

## Lot suivant

Le lot round 5 conserve exactement 97 cartes non couvertes du round 4. Les
trois cartes terminées sont remplacées, dans l'ordre de la file et en restaurant
le quota 34 recent / 33 priority / 33 league, par Goblin Welder, Acererak the
Archlich et Skullclamp. Les 100 `oracle_id` sont uniques et leur ordre correspond
exactement au fichier Oracle d'entrée. L'audit NativeOnly frais contient 201
capacités et prend 0,113 s.

## Artefacts

- comparaison finale globale : `.tmp/oracle-global-2ABB-to-9DFF-comparison.json` ;
- vérification globale indépendante :
  `.tmp/targeted-zone-move-global-root-verification.json` ;
- contrats exécutables des 47 gains :
  `.tmp/targeted-zone-move-generalized-contract-results.jsonl` ;
- comparaison du lot : `.tmp/rotating-next-100-round4-2ABB-to-9DFF-comparison.json` ;
- égalité parser/contrat/runtime :
  `.tmp/targeted-zone-move-claims-fixed-equality-root.json` ;
- preuves runtime : `.tmp/targeted-zone-move-runtime-final.jsonl` ;
- audit rouge conservé : `.tmp/oracle-global-2ABB-to-87D9-comparison.json` ;
- trackers : `.tmp/card-migration-timings/round3-43-eternal-witness-zone-move.json`,
  `round3-54-buried-ruin-zone-move.json` et
  `round3-79-noxious-revival-zone-move.json`.
- lot suivant : `.tmp/rotating-next-100-round5.json` et
  `.tmp/rotating-next-100-round5-nativeonly-9DFFE7AA.jsonl`.
