# Référents typés et propriétés après départ : checkpoint90A8

Accepté : **3 576 cartes non vides entièrement natives,17 233 capacités natives**.
Gain depuisEEE : **8 cartes/+9 capacités**, avec les17 224 anciennes IR exactement
préservées. Les dix sets figés restent à287 cartes/1 525 capacités ; les3 144 objets
vides du corpus global restent séparés. Depuis le point initial3409/16874 de cette
refonte : +167 cartes/+359 capacités, sans attribuer ces gains au seul dernier lot.

Nouvelles cartes complètes : Rapid Hybridization, Divine Offering, Vendetta,
Sever Soul, Feed the Swarm, Serene Offering, Devour in Shadow et Terashi's Grasp.
Incubation // Incongruity gagne sa capacité Incongruity, mais reste incomplète.
Il s'agit de couverture parser ; les preuves dynamiques spécifiques de ce lot
portent sur Rapid et Feed, avec les primitives partagées éprouvées au checkpoint LKI.

## Frontières réutilisables

TypedTargetReference conserve l'identité du slot et un nominal éventuel. Numérique
et contrôleur utilisent le même enfant lexical, puis la même preuve que toutes
les branches du sélecteur original impliquent ce nominal. Aucun nom de carte ou
effet combiné n'est ajouté. Les anciens lecteurs gardent leur capacité Unqualified.
Seul le parent de séquence déjà capable de capturer le contrôleur avant destruction
active les nouveaux acteurs qualifiés pour la création de jetons.

Les lectures numériques post-zone sont permises seulement si le slot original
garantit un permanent. Le backend lit cet objet vivant ou sa LKI authentifiée.
Les références aux sorts en pile, mixtes Spell/Permanent et contrôleurs après
départ restent plafonnées. P/T exigent toujours un domaine Creature prouvé.
Le changement ne retire donc aucun garde-fou de nominal, variable, cardinalité
ou identité de cible.

## Preuves

- 500 DATA parser dans chacun des modes NativeOnly et Full, zéro fixture invalide.
  70 privés numeric et4 référents passent ; les partitions se recoupent et ne
  s'additionnent pas. Dix near misses préservent les caps des anciens consommateurs.
- Deux entrées Oracle réelles complètes, prises dans la cohorte active à hash
  vérifié, produisent leur IR effectivement émise. Comparaison JSON sensible aux
  types et à la casse, sans normalisation, avec toutes les règles attachées aux
  cartes sources des scénarios. Pour Feed, la règle top-level du fixture original
  était attachée par le runner via son action castSpell ; la copie émise l'explicite.
- Trois scénarios réels liés : contrôleur volé, cible indestructible et Feed
  MV5 détruite/lanceur20→15 ;29 assertions passent sans compilation en0,043s.
  Les374 contrats complets/IR passent en0,206s sans build.
- Régression permanente moteur :655/655 scénarios,4472assertions, sans build en2,852s.
  Les174 privés du backend restent le certificat parent ; aucun nouveau code moteur
  n'est ajouté par ce frontend.
- Préfixe historique145/145,49,829s dont34,321s de build parser audit. Les395 preuves
  compatibles restent certifiées en cache, zéro réexécution nécessaire. Les deux
  nouvelles cartes sont en plus certifiées séparément :2/2,5preuves fraîches,10,98s.
  Le registre suivi passe de155 à157 cartes, sans modifier les155 précédentes.
- Architecture :48 primitives, zéro composite, zéro violation/avertissement.

Le premier privé numeric fait69/70 : un nouveau test utilisait la propriété sans
son introducteur « equal to ». Le test est corrigé, son assertion de nominal reste
inchangée ; la grammaire production n'est pas élargie pour le faire passer.
Compilation initiale33,94s, correction6,20s ; exécution corrigée0,93s. Native500 :
build20,854s/exécution0,914s ; Full sans build0,668s. Release31,496s/link1,450s ;
audit global39 288 entrées/67 417 capacités9,313s. Aucun gain de vitesse n'est déduit
de ces passages seuls. Une redondance de feature audit-cli sera traitée séparément.

Certificat : `.tmp/night-frontends-rapid-feed-01/certificate.json`, freeze et
rapports dans le même dossier ; liens exacts `vertical-proof-links.json`.
Chronos : agent Rapid `night-rapid-controller-reference-01/timings/`, frontend
Feed `night-feed-permanent-lki-01/timings/`, root
`.tmp/card-migration-timings/night-feed-target-lki-root-02.json`. Les intervalles
root incluent compilation, orchestration et revues parallèles ; ils ne sont pas
du temps CPU actif par carte. L'événement root loop1 décrit une transition prévue
backend→frontend ; le véritable échec privé est consigné comme tel dans loop2.

Le pool750 passe de167nonvides+4vides/523capacités à169nonvides+4vides/525capacités.
La cohorte suivante conserve98 cartes non résolues et ajoute Professional Face-Breaker
et Explore. OptionalEffectProgram, outcomes typés, batch Damage/SourceLKI et entretien
du cache restent des paquets distincts en revue ; aucune couverture anticipée.
