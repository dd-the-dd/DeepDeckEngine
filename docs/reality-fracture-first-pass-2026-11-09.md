# Reality Fracture — premier passage de généralisation native

Audit parser uniquement, exécuté sur les 285 cartes uniques du set `FRA`
avec l'auditeur debug construit après la certification de Force of Negation.
Aucune correction de carte n'a été appliquée pendant cette mesure.

## Résultat

| Mesure | Cartes | Part du set |
|---|---:|---:|
| Entièrement natives | 1 | 0,35 % |
| Mixtes, au moins une capacité native | 9 | 3,16 % |
| Entièrement legacy | 274 | 96,14 % |
| Unsupported | 1 | 0,35 % |
| Canonical, natif ou legacy | 284 | 99,65 % |
| IR primitive | 276 | 96,84 % |
| IR composite legacy | 8 | 2,81 % |

La couverture au niveau capacité est de 11 capacités natives contre 551
capacités legacy, sans capacité non revendiquée. Une carte `canonical` n'est
donc pas considérée native si sa reconnaissance dépend encore du legacy.

La seule carte entièrement native est **Precise Redaction**.

Les neuf cartes mixtes sont :

- Hallway Heckler // Vicious Verse — 1 capacité native, 2 legacy;
- No Admittance — 1 native, 1 legacy;
- Wrath of the Bloodmane — 1 native, 1 legacy;
- Greenhouse Propagator — 1 native, 1 legacy;
- Stingerquill Voxmancer // Vicious Verse — 1 native, 1 legacy;
- Whiplash Wordsmith // Vicious Verse — 1 native, 2 legacy;
- Hall of Echoes — 1 native, 1 legacy;
- Hexhaven Dueling Arena — 1 native, 2 legacy;
- Roiling Canopy — 1 native, 2 legacy.

La seule carte unsupported est **Solve for Disappointment**, sur la capacité :

> Target opponent reveals their hand. You choose a nonland permanent card from
> it. That player discards that card.

Les huit cartes encore adossées à une opération composite legacy sont Emrakul,
the Exigent Doom; Prophesied End; Seasoned Cryomancer; Curse-Marred Demon; Null
Summoner; Uldaros Theorix; Fblthp, Impossibly Lost; et Chandra, Torch of Defiance.

## Performance

Deux passages consécutifs ont donné 18,04 s puis 17,44 s de temps parser pour
les 285 cartes. Le deuxième processus complet a pris 17,49 s, soit environ
16,3 cartes par seconde. Les cartes legacy les plus lentes doivent être
réévaluées après leur migration, car certaines dépassent actuellement 300 ms
et Emrakul, the Exigent Doom dépasse une seconde à elle seule.

Cette mesure est la baseline `avant`. La comparaison finale du set devra
réexécuter exactement le même corpus et rapporter les deltas native/mixte/
legacy/unsupported, primitive/composite et temps total.

## Prochain jalon intermédiaire

Relancer le même audit sur les 285 cartes lorsque le préfixe strict de la file
prioritaire atteindra **40 cartes**, soit après 20 migrations supplémentaires
à partir de Cultivate, certifiée au rang 20. Comparer à cette baseline :

- cartes entièrement natives, mixtes, legacy et unsupported;
- capacités natives et legacy;
- IR primitives et composites;
- familles nouvellement généralisées dans Reality Fracture;
- temps parser chaud total et cartes par seconde.

## Jalon intermédiaire — préfixe strict 40

Le même corpus de 285 cartes a été réaudité après la certification de Heroic
Intervention au rang 40. Le rapport machine lisible est
`.tmp/reality-fracture-prefix40.json`.

| Mesure | Baseline | Préfixe 40 | Delta |
|---|---:|---:|---:|
| Entièrement natives | 1 | 3 | +2 |
| Mixtes, au moins une capacité native | 9 | 36 | +27 |
| Entièrement legacy | 274 | 241 | -33 |
| Unsupported | 1 | 5 | +4 |
| Canonical, natif ou legacy | 284 | 280 | -4 |
| IR primitive | 276 | 272 | -4 |
| IR composite legacy | 8 | 8 | 0 |
| Capacités natives | 11 | 45 | +34 |
| Capacités legacy | 551 | 517 | -34 |

Les trois cartes entièrement natives sont **Precise Redaction**, **Artifist
Acumen** et **Karn, Argent Defender**. Les gains ne viennent donc pas seulement
de cartes FRA explicitement migrées : les automates partagés reconnaissent
34 capacités supplémentaires et font passer 27 autres cartes de legacy à
mixte. Les familles les plus visibles dans ce corpus sont les capacités de
mana, les coûts et séquences d'effets atomiques, les sélecteurs typés, les
conditions d'entrée et la suppression générique de déclencheurs.

La mesure expose aussi une régression réelle de portée. En plus de **Solve for
Disappointment**, quatre cartes sont maintenant refusées : **Carnivorous
Cultivator // Enroot**, **Heartwood Crafter // Soul Tether**, **Vigorbloom
Charm** et **Gideon's Memorial**. Elles sont confiées au chantier de
généralisation; elles ne doivent pas être maquillées par quatre exceptions.
Deux IR primitives historiques restent non exécutables : **Yargle, Goliath of
Otaria** et **Yargle, Glutton of Urborg**. Les huit composites sont inchangés.

Les cinq refus se regroupent en quatre fondations réutilisables :

| Fondation | Refus réparés directement | Portée FRA observée |
|---|---|---:|
| `CardCategory::Permanent` dans le sélecteur partagé | Solve for Disappointment | 4 textes `permanent card` |
| `ZoneDestination` partagé | Carnivorous Cultivator // Enroot | 8 recherches vers main/cimetière |
| `ManaSpendConstraint` lié au mana produit | Heartwood Crafter // Soul Tether; Gideon's Memorial | 3 contraintes de dépense |
| Conteneur modal natif + bindings d'effets | Vigorbloom Charm | 17 cartes `Choose one` |

Le plan interdit les opérations combinées propres à ces cartes. Par exemple,
le mode compteur puis combat doit rester `putCounters` suivi de `fightPermanents`
avec un binding de cible; la recherche vers le cimetière reste `chooseCards`,
`moveCards`, `shuffleZone` et `recordLibrarySearch`.

### Performance du jalon

L'audit release chaud prend **2,74 s**, soit environ **104 cartes/s**. Le cycle
complet, incluant la vérification de 272 IR primitives, prend **31,19 s**; le
contrat en occupe 17,45 s. Un passage debug comparable à la baseline prend
**24,23 s**, soit **11,8 cartes/s**, contre 17,44 s et 16,3 cartes/s au premier
passage. Le debug a donc ralenti d'environ 39 %. Le gain release n'est pas
présenté comme un gain apples-to-apples : l'optimisation du chemin debug reste
un chantier mesuré.
