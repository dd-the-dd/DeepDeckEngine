# Prochain correctif générique — lot rotatif 5

Date : 2026-11-12  
Mode : audit `NativeOnly`, lecture seule  
Empreinte parseur : `9DFFE7AAB863D3A980387CB8C4CDCE6CC695CCE1209E81CA34CA4C5159465A45`

## Verdict

Après exclusion du chantier `permanentDied` déjà en cours pour Blood Artist et
Pitiless Plunderer, aucun correctif générique sûr ne complète deux cartes du lot
à lui seul. Les regroupements à deux cartes sont soit plusieurs sous-automates
différents, soit dépendants d'une sémantique moteur non certifiée.

Le meilleur prochain slice indépendant est donc :

> `You may play N additional land(s) on each of your turns.`

Il complète **Exploration**, touche aussi Icetill Explorer et réutilise un IR et
un comportement moteur déjà présents. Le lot contient 100 cartes et 201
capacités; son audit chaud a pris 0,113 s.

## Classement conservateur

| Rang | Correction précise | Cartes complètes | Capacités/cartes touchées | Risque moteur |
|---:|---|---:|---|---|
| 1 | `additionalLandPlay` statique par tour du contrôleur | 1 — Exploration | 2 cartes; Icetill reste bloqué par deux autres capacités | Faible |
| 2 | événement `stepBegan(upkeep, controller)` → `EffectSequence` | 1 — Phyrexian Arena | 1 | Faible à modéré |
| 3 | entrée d'un permanent contrôlé typé → `EffectSequence` | 1 — Tatyova | 4 touchées, mais trois cartes gardent d'autres bloqueurs | Faible à modéré |
| 4 | `that permanent's mana value` lié à la cible | 1 — Feed the Swarm | 1 | Modéré, LKI après déplacement |
| 5 | coût `sacrifice SELECTOR` → ajout de mana | 1 — Phyrexian Altar | 1 | Modéré |
| 6 | entrée engagée + anaphore du permanent déclencheur | 1 — Amulet of Vigor | 1 | Modéré |

La règle de comptage est stricte : une carte n'est ajoutée au gain que si la
même correction précise couvre tous ses bloqueurs. Une primitive partagée ou
une capacité touchée n'est pas une carte complète.

## Pourquoi `additionalLandPlay` est le meilleur choix

L'ancienne voie regex émet déjà le contrat canonique attendu :

```json
{
  "kind": "staticAbility",
  "source": { "kind": "self" },
  "modifiers": [{
    "kind": "additionalLandPlay",
    "player": {
      "kind": "controllerOf",
      "object": { "kind": "self" }
    },
    "count": { "kind": "integer", "value": 1 }
  }]
}
```

Le moteur sait déjà valider ce modificateur, additionner les permissions des
sources contrôlées présentes sur le champ de bataille et l'appliquer à la
légalité des actions de pose de terrain. Il ne faut donc aucun nouvel opcode.

Le sous-automate proposé lit, dans l'ordre : sujet `you`, permission `may`,
action `play`, quantité (`an` = 1 ou expression numérique), type `land(s)`, puis
période `on each of your turns`. Le parent statique fixe le joueur à
`controllerOf(self)`. Toute autre période, tout autre sujet ou tout désaccord
singulier/pluriel reste un échec structuré.

Dans le corpus global 9DFF, cette famille statique exacte ou paramétrée touche
22 capacités et compléterait quatre cartes si elle était corrigée seule :
Exploration, Azusa, Titania et The Explorer. Ce nombre global est un potentiel,
pas un gain du lot courant.

## Les faux groupes à deux cartes

`Ruinous Ultimatum` et `Supreme Verdict` partagent bien un déplacement de masse
et seraient deux gains complets. Le slice reste différé : détruire séquentiellement
une collection peut modifier les effets statiques ou de remplacement avant les
objets suivants. Il faut d'abord certifier un lot de destruction simultané.

`Amulet of Vigor` et `Tatyova` appartiennent tous deux à la famille des entrées
de permanents, mais pas au même manque précis :

- Tatyova demande un prédicat `Land` contrôlé et un corps `gainLife` puis
  `drawCards`;
- Amulet demande de capturer l'état « est entré engagé », de lier le permanent
  déclencheur et de l'envoyer à `untapPermanent`.

Les compter comme deux gains d'un seul correctif gonflerait artificiellement la
couverture.

## Nouvelles recrues du lot

- **Goblin Welder** : zéro gain par petit correctif. Il faut deux cibles liées au
  même joueur, une revalidation conjointe et un sacrifice/retour simultané.
- **Acererak the Archlich** : zéro gain par petit correctif. Ses deux capacités
  nécessitent respectivement l'état des donjons et une boucle par adversaire avec
  choix `unless sacrifice` lié à cet adversaire.
- **Skullclamp** : zéro gain par le chantier mort seul. Il reste un modificateur
  continu +1/-1 sur la créature équipée et un événement de mort utilisant la
  dernière relation d'attachement connue.

## Preuves à exiger pour le slice recommandé

1. carte exacte Exploration;
2. sibling numérique « two additional lands »;
3. near-miss d'accord et de période;
4. scénario moteur autorisant une pose normale puis une pose supplémentaire;
5. contrôle où la source quitte le champ de bataille et la seconde pose redevient
   illégale;
6. replay round5 et global avec comparaison de toutes les capacités natives et
   de leur IR, pas seulement des cartes entièrement natives.

## Sources de mesure

- cohorte : `.tmp/rotating-next-100-round5.json`, SHA-256
  `F3860456E8030490C19F0B22B938D9E0A01E7061E56707CAB5CE7B362738DA45`;
- audit lot : `.tmp/rotating-next-100-round5-nativeonly-9DFFE7AA.jsonl`,
  SHA-256 `B8A1ADE405A8C57A9F5A847746EB6B69E18D3753F4E4C865643E3DF56FF161DE`;
- audit global : `.tmp/oracle-global-nativeonly-9DFFE7AA.jsonl`, SHA-256
  `D72AF99CC59E6622AABD7748B8CF6D6B5BBB311A110029BE2A530BB8A20A00E4`.

Le rapport machine complet se trouve dans
`round5-next-generic-correction-2026-11-12.json`.
