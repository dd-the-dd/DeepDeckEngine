# Round 3 — prochain correctif générique après Attack Tax

## Décision

Le meilleur correctif indépendant et sûr de la cohorte courante est un enfant typé de **déplacement ciblé d'une carte entre zones**. Il termine trois cartes sur cent : Eternal Witness, Buried Ruin et Noxious Revival.

Attack Tax est exclu de ce classement parce que son intégration est en cours. `Destroy all` reste différé : son gain apparent de deux cartes dépend encore d'une sémantique de destruction simultanée non prouvée dans le moteur.

| Rang | Concept précis | Cartes touchées | Cartes entièrement terminées | Risque runtime |
|---:|---|---:|---:|---|
| 1 | Déplacement ciblé d'une carte entre zones | 3 | **3** | Faible à modéré |
| 2 | Jeu d'un terrain additionnel | 2 | 1 | Faible |
| 3 | Trigger d'entretien avec séquence draw + lose life | 1 | 1 | Faible |
| 4 | ETB d'un permanent engagé puis untap de l'objet lié | 1 | 1 | Modéré |
| 5 | Destroy ciblé + valeur/propriétaire après déplacement | 2 | 2 séparément, pas un même correctif | Modéré à élevé |

Le cinquième groupe n'est pas compté comme un gain de deux : Feed the Swarm et Pongify partagent `destroyPermanent`, mais leurs continuations exigent deux contrats différents — valeur numérique LKI pour le premier, interdiction de régénération et contrôleur LKI pour le second.

## Trois gains complets

### Eternal Witness

- Oracle ID : `30b24e8e-3b0e-4d8e-90f3-f66eb7c1858c`
- Cohorte : rang 43, provenance priority + league.
- Texte : `When this creature enters, you may return target card from your graveyard to your hand.`
- État F0 : une capacité non revendiquée; aucun autre bloqueur.
- Enveloppe : événement `enterBattlefield(self)` déjà partagé.
- Atom manquant : cible carte de `graveyard(controller)` vers `hand(controller)`.
- Modalité : le choix de la cible reste obligatoire à la mise en pile; `may` enveloppe seulement l'effet de résolution dans `optionalEffects`.

### Buried Ruin

- Oracle ID : `3644f316-f9a3-46c9-9b1e-747f86cf4ead`
- Cohorte : rang 54, provenance recent + league.
- Texte bloquant : `{2}, {T}, Sacrifice this land: Return target artifact card from your graveyard to your hand.`
- Autre capacité : `{T}: Add {C}.` est déjà native et canonique.
- Coûts disponibles : `payMana`, `tap(self)` et `sacrificePermanent(self)` sont déjà composables par l'automate de coûts d'activation.
- Atom manquant : le même mouvement que pour Eternal Witness, avec le prédicat `Artifact`.

### Noxious Revival

- Oracle ID : `97cabeda-9fe3-490d-99b4-4c8d87c17157`
- Cohorte : rang 79, provenance recent + priority + league.
- Texte bloquant : `Put target card from a graveyard on top of its owner's library.`
- Autre capacité : le rappel de mana phyrexian est déjà natif et canonique.
- Atom manquant : cible dans `anyGraveyard`, destination `library(ownerOf(chosenTarget), top)`.

Ces trois cartes n'ont aucun bloqueur résiduel après ce slice. Le gain complet conservateur est donc 3/100, et non une estimation fondée uniquement sur une occurrence textuelle.

## Frontière générique proposée

L'IR syntaxique existant `ReturnTargetCardNode` est actuellement borné à `your graveyard -> your hand`, tandis que l'automate activé de changement de zone ne sait promouvoir que `exile`. Il faut les faire converger vers un seul atom, sans créer une deuxième représentation :

```text
TargetCardMoveNode
├── selector: ObjectPredicate
├── source: CardZoneScope
│   ├── ControllerGraveyard
│   └── AnyGraveyard
└── destination: CardZoneDestination
    ├── ControllerHand
    └── TargetOwnerLibraryTop
```

L'optionalité, le trigger et les coûts restent la responsabilité des parents. L'atom ne contient ni nom de carte ni phrase Oracle complète.

Lowering canonique commun :

```json
{
  "declaration": {
    "decisions": [{
      "kind": "chooseTargets",
      "id": "targetGraveyardCard",
      "minimum": 1,
      "maximum": 1,
      "candidates": { "kind": "cards", "zone": "SOURCE", "where": "PREDICATE" }
    }]
  },
  "effects": [{
    "kind": "moveCards",
    "cards": { "kind": "chosenTargets", "id": "targetGraveyardCard" },
    "from": "SOURCE",
    "to": "DESTINATION"
  }]
}
```

Pour Noxious Revival, `DESTINATION.player` doit être `ownerOf(chosenTarget("targetGraveyardCard"))`; il ne faut pas approximer par le contrôleur de la capacité.

## Transitions manquantes exactes

1. L'enfant lit `return` ou `put` comme surface d'un même mouvement, puis délègue le prédicat de carte au sélecteur partagé.
2. Il distingue `from your graveyard` de `from a graveyard` sans texte brut.
3. Il reconnaît `to your hand` et `on top of its owner's library` comme destinations typées.
4. Le parent ETB étend sa frontière de claim à `you may return`, puis réutilise l'événement source-entry et enveloppe le mouvement dans `optionalEffects`.
5. Le parent activé réutilise `ActivationCostSequenceNode`; son claim passe de `exile target card(s)` seulement à l'action de zone typée `return target ... card`.
6. Le parent spell accepte l'atom `put target card ...` et exige la consommation complète.
7. Les trois parents abaissent la même déclaration de cible et le même `moveCards`; aucune primitive moteur nouvelle n'est nécessaire.

## Preuves nécessaires

- exact : les trois cartes de la cohorte;
- siblings : prédicats `creature`, `artifact or enchantment`, `land`, `instant or sorcery`;
- near-miss : mauvaise zone, destination non supportée, possessif incomplet, cible permanente du champ de bataille;
- runtime Eternal Witness : refuser puis accepter l'effet optionnel sans perdre la cible déclarée;
- runtime Buried Ruin : payer mana, engager et sacrifier la source avant de déplacer exactement la carte ciblée;
- runtime Noxious Revival : choisir une carte dans le cimetière adverse et la placer sur la bibliothèque de son propriétaire;
- contrôle d'identité : une cible déplacée avant résolution devient un no-op et ne déplace aucune autre carte;
- régression : toutes les capacités native+canonical F0, y compris celles des cartes mixtes, conservent leur IR.

## Risque runtime

Le moteur exécute déjà `moveCards`, les contraintes de zone source, `ownerOf` sur une référence d'objet et les destinations main/bibliothèque-top. Le risque résiduel est donc l'ordre des résolutions de références : le propriétaire doit être résolu à partir de la cible choisie avant que son déplacement rende la référence introuvable. Ce point demande un test runtime, mais pas une nouvelle opération combinée.

## Autres candidats

- **Exploration** (`0c2841bb-038c-4fbf-8360-bc0a1522b58d`) serait terminée par `additionalLandPlay`; Icetill Explorer partage la phrase mais conserverait deux autres bloqueurs. Gain complet : 1.
- **Phyrexian Arena** (`ee579a32-a048-4335-b966-231ba731cdea`) demande l'enveloppe générique `beginningOfUpkeep(controller)` autour de `drawCards(1)` puis `loseLife(1)`. Gain complet : 1.
- **Amulet of Vigor** (`dba16032-66c1-4ccb-9d65-d41ac550d182`) demande un événement d'entrée lié, le prédicat `entered tapped`, puis `untap(boundObject)`. Gain complet : 1; risque de binding supérieur.
- **Feed the Swarm** (`5825997b-10d7-4a36-972c-a80ddd90b8ed`) arrive déjà jusqu'à `numeric.expression.object-reference.card`; il manque `that permanent's mana value`. Le runtime doit conserver la valeur LKI après destruction.
- **Pongify** (`05849bd6-8f38-4031-be2b-e2aa03beb8cc`) arrive après `destroyPermanent`, puis échoue sur `It can't be regenerated`. Il faut représenter cette restriction et conserver le contrôleur LKI pour créer le token; l'ignorer serait incorrect.

## Potentiel global, non compté dans le gain round3

Un balayage lexical de la baseline globale F0 trouve 57 cartes portant une surface de cette famille et 40 sans autre bloqueur apparent. Ce nombre est un plafond exploratoire, pas une promesse de gain : plusieurs coûts activés comportent des alternatives, sacrifices de tiers, défausses ou exils qui doivent être validés par leurs automates respectifs. Le score de décision demeure donc **3 gains complets prouvables dans la cohorte active**.

## Sources de mesure

- Cohorte : `.tmp/rotating-next-100-round3.json`, SHA-256 `43799DBE064A992EDCDF99A6A776E0EC281D38F0B4A6299281F5E5F28BEF13FB`.
- Audit F0 : `.tmp/rotating-next-100-round3-nativeonly-F0FAC6BD.jsonl`, SHA-256 `9AE332552E26770DA87763FF274C40D40CF7378D468F90D733B1794DD1F1A614`.
- Snapshot parseur : `F0FAC6BDDD6C994FA1FFFAB4EEE73D9A901A719B42C757EEEEA2E898FE3A2887`.
- Temps de l'audit 100 cartes : 0,106 s; 200 capacités.
