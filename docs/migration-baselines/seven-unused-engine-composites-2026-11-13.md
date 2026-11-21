# Sept opérations moteur obsolètes retirées — 13 novembre 2026

Codes retirés : searchLibrary, searchLibraryRelativeToSacrificedPermanent, chooseCreatureForEachPlayerThenSacrificeNonsharing, revealUntilAndPutOntoBattlefield, revealUntilAndPutIntoHand, destroyUpToOneForEachOpponent, putAbilitySourceOntoBattlefieldTappedAndAttacking.

L'inventaire courant recoupe 16 656 capacités natives, les producteurs parser, les références internes, les artifacts générés de tous les crates, les données et les fixtures permanentes. Aucun producteur ni consommateur positif de ces sept opérations n'est observé. La fermeture CST retire exactement leurs quatorze bras dans effect_supported/execute_effect ; les 37 fonctions partagées appelées par ces bras restent référencées et sont conservées. Aucun retrait des hooks compteurs/tokens des agents ni du backend, des horloges ou du training.

631 lignes de logique supprimées. Rustfmt est appliqué au seul module changé avec edition2024, skip_children, CRLF Windows et aucun réordre de modules/imports. Le fichier net perd679lignes, dont48de formatage. La comparaison AST de formatage normalise explicitement les accolades d'une expression unique de bras match et les token_tree vides ; aucune normalisation de chaînes de l'IR. Les snapshots avant, après suppression et après formatage restent distincts.

Les sept probes de forme valident l'ancienne frontière. Le premier passage reste6/7 : une fixture avait utilisé self là où un targetRef déclaré est requis. Elle est corrigée avec chosenTarget et une déclaration, sans changer les attentes vrai-avant/faux-après. Le second7/7, les deux rapports bruts et la boucle de diagnostic sont conservés. Ces probes synthétiques ne sont pas des cartes migrées.

Preuves fraîches : runtime530/530,3 615assertions ; contrats917/917, zéro invalide ; ancien corpus16 029formes exécutables conservées, quatre anciens refus inchangés ; gate145/145 avec395preuves réexécutées et zéro fetch ; architecture sans violation. Le parser n'a pas changé, fingerprint B667 identique ; ses320cas permanents du checkpoint parent restent une preuve héritée, pas un nouveau rejeu revendiqué ici.

Temps runtime : compilation/liaison 45.431s, exécution 2.514s. Contrats917 : 3.483s,130groupes de conservation : 0.412s. Les temps de la boucle et du gate sont séparés dans leurs JSON. Aucun benchmark de partie complète.

Le cumul de reconnaissance du parser reste16 656capacités et3 297objets non vides entiers, dont+328depuisC355 ; cela ne certifie pas toute la sémantique primitive. Amass, Explore, certains autres aliases historiques et les quatre anciens refus restent de la dette explicitement conservée. La décomposition Amass/Explore et les quantités sont en staging, aucune couverture issue d'un paquet non intégré n'est ajoutée.

L'archive distante reste archive/legacy-oracle-2026-11-13. Aucun commit ni modification de l'index initial. Le round17 exact reste actif, sans nouveau refill.
