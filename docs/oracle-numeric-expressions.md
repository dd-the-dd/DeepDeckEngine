# Expressions numériques (C7)

Une seule grammaire arithmétique (`automata/arithmetic_expression.rs`) lit
les quantités : atomes, opérateurs, `twice`, parenthèses, et, pour les
lectures de définition (`where X is …`, `equal to …`), les quantités
composées. Ce document décrit les opérandes ajoutés et ce qui reste.

## Opérandes ajoutés

- `your devotion to COLOR` (CR 700.5) → `{"kind":"devotion","player":…,
  "color":…}`. Une dévotion à deux couleurs compte une fois un symbole
  hybride (CR 700.5a) : refusée explicitement (`numeric.devotion.color`).
  Moteur : la dévotion comptait les couleurs des permanents ; elle compte
  maintenant les symboles de mana de la couleur dans leurs coûts (un
  symbole hybride compte pour chacune de ses couleurs).
- `half EXPR, rounded up|down` (CR 107.1a) : la moitié de n'importe quel
  opérande de la même grammaire, avec l'arrondi écrit (`divide` / `round`).
  Sans arrondi écrit, échec explicite (`numeric.arithmetic.half.rounding`).
  La forme établie `half the number of cards in your library` est
  inchangée.
- `your life` est le total de points de vie (CR 119.1), comme `your life
  total`.

Preuves : `devotion_and_halves_are_operands_of_the_shared_arithmetic_grammar`
(analyse, sœurs, quasi-échecs) et
`devotion_counts_mana_symbols_and_halves_round_as_stated` (moteur).

## Mesure (`numeric.atom.start`, blocage unique, main après #57)

| Jeton | Cartes | Nature réelle |
|---|---|---|
| `your` | 79 | surtout `discard your hand` (action sans quantité), dévotion, vie |
| `any` | 79 | `any number of …` : choix déclaré (non traité) |
| `the` | 75 | `the number of …`, `the mana value of …`, `the top card` |
| `{e}` | 65 | paiement d'énergie (`pay {E}{E}`) : coût, pas une quantité |
| `all` | 61 | `remove all counters`, `discard all the cards` : actions totales |
| `its` | 50 | `where X is its power` : possessif résolu par l'environnement C2 |
| `half` | 17 | traité quand l'opérande est lisible et l'arrondi écrit |

Non traité dans ce lot : `any number of` (choix déclaré avec bornes par
action), le possessif `its` dans une définition (il appartient au
résolveur C2, `binding_environment.rs`), le paiement `pay half your life`
(grammaire des coûts, C5), et les actions totales (`discard your hand`,
`remove all counters`), qui ne sont pas des quantités.
