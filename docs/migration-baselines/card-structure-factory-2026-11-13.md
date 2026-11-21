# Factory des parties de carte avant la lecture Oracle

`plan_card_structure` dans `src/oracle/card_structure.rs` est le premier
contrôle du parser public. Le plan typé distingue les surfaces physiques,
moitiés split, portes Room, alternatives Adventure/Omen et sorts préparés.
Chaque partie garde son index, son identité et son rôle. Ses caractéristiques,
son coût et son Oracle restent dans la requête originale, sans fusion de texte.

Le parser expose ce plan dans `cardStructure`. `documents` parcourt ses parties
validées ; les automates Oracle restent partagés. Les raccordements existants
de `src/model.rs` utilisent le type du plan au lieu de refaire leur propre
sélection de layout. La compilation jouable refuse un préflight invalide avant
de pouvoir accepter par erreur une carte avec zéro règle.

Deux diagnostics bloquent la lecture des capacités :

- `card_face_factory_missing` : layout inconnu, multipart sans layout, ou
  profil de sort alternatif pas encore enregistré.
- `card_face_structure_invalid` : format connu avec nombre de parties,
  identités ou caractéristiques associées incohérents.

Ils précisent l'automate `card-face-factory`, l'état, le format attendu et les
métadonnées rencontrées. Aucun fallback vers un texte aplati n'est utilisé.

## Compatibilité

La stratégie split conserve les parties ordonnées et accepte les exemples
locaux à trois ou cinq parties. Room n'est choisi que pour deux parties Room.
Une Aventure n'exige pas une créature comme partie principale : Twice Upon a
Time / Unlikely Meeting a une partie principale Sorcery. Les Omens locaux ont
le layout `adventure` et se distinguent par le sous-type Omen.

Les formats auxiliaires connus (art series, tokens, etc.) restent descriptibles.
Un plan de composant meld ne prétend pas disposer des règles du partenaire ou
du résultat, qui sont des objets externes à la requête. Une structure reconnue
ne certifie pas les règles de lancement ou de résolution d'une carte.

## Vérifications

- 397 fixtures parser passent dans Native et Full, sans fixture invalide.
  Les 393 cas précédents gardent leurs attentes ; quatre cas vérifient le
  nouveau refus précoce, dont un vrai input Oracle complet.
- 4 tests privés couvrent les familles structurelles, rôles, identités,
  métadonnées invalides, diagnostics et conservation du contexte des parties.
- 2 tests de contrats couvrent les adaptateurs split, modal DFC, transform et
  prepare ainsi que le refus des structures inconnues/incomplètes.
- Le contrat existant d'Emeritus of Ideation passe avec le lien prepareSpell.
- Les deux scénarios moteur de préparation passent, avec 13 assertions.
- Garde d'architecture : zéro violation, zéro avertissement.

Replay du même input global immuable : 39 288 objets. 39 287 plans reconnus.
Toutes les capacités auparavant natives et leur IR sont conservées ; les
39 287 autres résultats par capacité sont identiques à ceux de l'analyseur
précédent. Un objet déjà entièrement unsupported change seulement de
diagnostic : Hela, Mistress of Darkness / Dark Tutor, oracle_id
`203f6f4a-1cf5-4ddc-b60f-676832eeff6b`, profil `Sorcery — Vengeance`, conservé
dans les données de prévisualisation. Il produit maintenant
`card_face_factory_missing`, état `factory.alternative-spell-profile`, avant
la lecture Oracle. Ce profil n'a pas été assimilé à Adventure.

Les trois blocs auparavant non reconnus de cet objet ne sont plus envoyés
aux automates. Il ne faut pas interpréter la baisse du nombre de blocs lus
comme une baisse de couverture native ou une reconnaissance du nouveau format.

Rapports, provenance des inputs et temps séparés de compilation/exécution :
`.tmp/card-structure-factory/`. Les deux analyseurs globaux ont des profils de
compilation et des projections JSON différents ; leurs durées ne constituent
pas un benchmark de coût de la factory.

## Consigne des futures migrations

Le skill installé `deepdeck-card-structure` définit ce préflight comme première
étape. `deepdeck-cover-card` et `AGENTS.md` l'imposent avant tout ajout de
grammaire. Les deux skills ont été validés avec le validateur de skill-creator.
