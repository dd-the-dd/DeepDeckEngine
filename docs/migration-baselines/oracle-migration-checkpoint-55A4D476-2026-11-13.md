# Migration Oracle — checkpoint55A4D476

Le programme d’effets contextuel réutilise les mêmes coûts, cibles, nombres et séquences pour les sorts et activations. Le lecteur n’impose plus une liste de premiers verbes pour les anciennes activations targetless. Chaque action devient une primitive distincte; le chemin natif n’appelle pas le legacy.

Sur le corpus local de **39 288 objets Oracle /67 420 capacités**, le nouveau parseur reconnaît **15 852 capacités natives** et **2 870 cartes non vides entières**. Depuis9D : **+715 capacités / +344 cartes hors Token et Vanguard**, plus huit objets auxiliaires. Les 3 144 objets sans texte restent séparés. Les 15 132 anciennes IR inchangées et les cinq corrections historiques explicitement approuvées rendent compte des 15 137 anciennes capacités, sans perte de couverture ni changement non expliqué.

Les cinq anciennes corrections et Spider Food réparent la portée d’un keyword sur une liste : Artifact OR Enchantment OR(Creature AND Flying). Les champs de coût, source et effets restent identiques. Morningtide répare une ancienne mauvaise interprétation legacy : les cimetières sont exilés, le sort termine normalement au cimetière. Les rapports bruts et tuples avant/après sont conservés; aucune normalisation générale de JSON ne transforme un écart en réussite.

| Vérification | Résultat | Portée |
|---|---:|---|
| Fixtures parser natif | 252/252 | Carte exacte, frères, near-misses, bindings et scopes |
| Tests privés parser ciblés | 95/95 | Automates affectés; tous les tests lib ne sont pas revendiqués |
| Contrats exécutables | 1312/1312 | 240 permanents +715 règles nouvelles +352 vecteurs de cartes +5 corrections |
| Runtime permanent | 454/454;2964 assertions | Coûts, choix, résolution, annulation, états et ciblage |
| Compatibilité Full étendue | 1165 cartes;1144 parités natives | Zéro perte et aucun changement non manifesté |
| Panel historique Full515 | 296 parités natives | Dérivé sans refaire les audits;37 pins conservés |
| Préfixe historique | 145/145 | IR, contrats et runtime vérifiés; cache courant réutilisé |

Les totaux globaux mesurent le parsing complet et le contrat primitif. Ils n’impliquent pas un scénario de partie indépendant pour chacune des nouvelles cartes. Sakura-Tribe Elder et Deadly Dispute sont les deux cartes complètes du lot100 avec leur vrai Oracle et leurs preuves dynamiques. Airship Crash reste partielle : sa capacité0 est corrigée, Cycling reste non couvert.

| Mesure finale | Temps | Description |
|---|---:|---|
| Compilation des tests Selector | 32,91s | Build partagé pour les tests du sous-automate |
| Compilation runner parser après Scope | 31,25s | Un build partagé, puis les JSON changent sans recompilation |
| Exécution parser permanent | 0,408s | 252 cas de données, sans Cargo |
| Compilation release finale | environ170s | 81 artefacts, aucun artefact moteur dans le graphe parser |
| Tests runtime permanent | 1.689s | 454 scénarios; compilation moteur antérieure63,813s |
| Tests privés parser | 0.555s | 95 tests sur le même binaire, sans relink |
| Analyse du lot100 | 0,031s | Texte Oracle réel et entier; processus séparé dans le JSON |
| Analyse globale39288 | 7,618s | Analyse/export JSON; processus complet consigné séparément |
| Préfixe145 avec caches | 22.688s | 145 cache-hits de preuve, zéro réseau; aucune nouvelle exécution des395preuves déjà valides |

Plusieurs boucles d’implémentation, revue et compilation ont précédé ce dernier gel. Les journaux les conservent; cette table de validation n’est pas une durée totale de développement. Les agents travaillent en parallèle : leurs durées ne doivent pas être additionnées. La comparaison9D/debug et55A/release n’est pas un benchmark à profils égaux.

Le moteur distingue maintenant une production mana pure d’un programme avec effets annexes pour le paiement automatique. Pristine Talisman doit être activée explicitement pour gagner son point de vie avant de payer depuis la réserve; ses sept cas couvrent cette correction. Les activations qui déplacent une carte vers/depuis une bibliothèque utilisent la pile conformément à [CR605.1a et son bulletin officiel](https://magic.wizards.com/en/news/announcements/the-hobbit-update-bulletin). Les vrais textes de Chromatic Sphere/Eggs restent explicitement non couverts tant que leur reminder n’est pas lu.

Les dix sets audités sont à **173/2 279 cartes entières**, dont trois sans texte, et **1 308/4 652 capacités natives** : [tableau détaillé](last-10-sets-native-coverage-55A4D476-2026-11-13.md).

Le prochain lot est officiellement rempli : **98 cartes retenues + Culling Ritual et Akroma’s Will**, quotas34/33/33,100cartes/207capacités dont32natives, aucune entière au départ. Son entrée complète est hashée `8890E5C8FAFC4433E35B5E12A10070E836967D4BD3831350E89E06A509B2EFE1`. Les deux correctifs suivants sont préparés en staging : sacrifice d’un objet choisi (Phyrexian Altar/Zuran Orb/Skirk Prospector), puis défausse en coût additionnel (Big Score/Cathartic Reunion/Magmatic Insight). Les projections restent des hypothèses jusqu’à intégration et replay.

Le checkpoint est accepté localement, le but de migration globale reste actif. Aucun commit, reset ou nettoyage de travail partagé non lié.
