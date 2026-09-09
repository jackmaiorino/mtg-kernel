# Pauper meta gap report (2026-09-09)

Source: mtgtop8 Pauper metagame, Last 2 Weeks window, 779 decks; 120 decklists sampled (8 most recent per archetype) via the MTGO export endpoint, preserved under pauper_meta_decklists_2026-09-09/. Shares in pauper_meta_shares_2026-09-09.json; the sampled archetypes total 87 percent of the field and the rest is unmeasured. Weight = meta share x unconditional average copies across the archetype's sampled decks (sideboard at half weight). Registry = data/cards_v1.json at 75406ffe (162 definitions). Regenerate with python/tools/pauper_meta_gap_v1.py.

## Archetype coverage (sampled lists)

| archetype | share | decks | mainboard copies covered | missing mainboard names (avg >= 1) | sideboard copies covered | missing sideboard names (avg >= 1) |
|---|---|---|---|---|---|---|
| Burn | 12% | 8 | 93% | 1 | 85% | 0 |
| MonoBlueAggro | 12% | 8 | 90% | 2 | 98% | 0 |
| Affinity | 10% | 8 | 84% | 4 | 97% | 0 |
| Urzatron | 9% | 8 | 13% | 16 | 56% | 1 |
| RedDeckWins | 6% | 8 | 94% | 2 | 69% | 2 |
| Elves | 5% | 8 | 90% | 2 | 79% | 0 |
| Jund | 5% | 8 | 94% | 1 | 91% | 0 |
| DimirControl | 4% | 8 | 70% | 5 | 79% | 1 |
| Ephemerate | 4% | 8 | 30% | 16 | 81% | 0 |
| GruulAggro | 4% | 8 | 42% | 11 | 63% | 1 |
| Spy | 4% | 8 | 100% | 0 | 65% | 2 |
| WhiteWeenie | 4% | 8 | 10% | 11 | 36% | 2 |
| Garden | 3% | 8 | 57% | 9 | 70% | 2 |
| Terror | 3% | 8 | 95% | 1 | 93% | 0 |
| Gates | 2% | 8 | 74% | 5 | 72% | 0 |

## Missing cards by exposure weight (top 120)

| rank | card | weight | archetypes (avg main copies) | xmage java lines | features |
|---|---|---|---|---|---|
| 1 | Plains | 68.8 | Affinity 0.12, Ephemerate 0.5, Gates 1.75, WhiteWeenie 15.5 | 28 |  |
| 2 | Giant's Boulder | 49.8 | Affinity 1.38, Urzatron 4.0 | absent in pinned XMage |  |
| 3 | Kessig Flamebreather | 42.0 | Burn 3.5 | 43 | trigger static target |
| 4 | Delver of Secrets | 36.0 | MonoBlueAggro 3.0 | 90 | trigger |
| 5 | Urza's Tower | 36.0 | Urzatron 4.0 | 37 |  |
| 6 | Urza's Power Plant | 36.0 | Urzatron 4.0 | 37 |  |
| 7 | Urza's Mine | 36.0 | Urzatron 4.0 | 37 |  |
| 8 | Expedition Map | 36.0 | Urzatron 4.0 | 45 | activated target cost |
| 9 | Bramble Wurm | 31.5 | Urzatron 3.5 | 57 | trigger activated cost |
| 10 | Utrom Monitor | 31.2 | Affinity 3.12 | absent in pinned XMage |  |
| 11 | Barrels of Blasting Jelly | 29.2 | Urzatron 3.25 | 48 | activated target cost |
| 12 | Ancient Stirrings | 27.0 | Urzatron 3.0 | 39 |  |
| 13 | Snow-Covered Island | 26.5 | Ephemerate 5.12, Terror 2.0 | 36 |  |
| 14 | Malevolent Rumble | 26.5 | Gates 2.5, GruulAggro 2.0, Urzatron 1.5 | 37 | static token |
| 15 | Crop Rotation | 23.6 | Urzatron 2.62 | 40 | static target cost |
| 16 | Snuff Out | 20.8 | DimirControl 3.88, Garden 0.88, Jund 0.5 | 51 | static target condition cost |
| 17 | Bonder's Ornament | 20.5 | Ephemerate 0.12, Garden 0.25, GruulAggro 0.88, Urzatron 1.75 | 79 | activated cost |
| 18 | Unfathomable Truths | 20.2 | Urzatron 2.25 | 37 | token |
| 19 | Maelstrom Colossus | 20.2 | Urzatron 2.25 | 36 |  |
| 20 | Pinnacle Kill-Ship | 18.0 | Urzatron 2.0 | 51 | trigger target |
| 21 | Bojuka Bog | 17.6 | DimirControl 0.62, Garden 1.88, Jund 0.12, Urzatron 0.88, WhiteWeenie 0 | 41 | trigger target |
| 22 | Mulldrifter | 17.4 | Ephemerate 3.5, Urzatron 0.38 | 44 | trigger |
| 23 | Augur of Bolas | 17.0 | DimirControl 1.5, Ephemerate 2.75 | 43 | trigger static |
| 24 | Sewer-veillance Cam | 16.8 | Affinity 0.62, DimirControl 0.38, MonoBlueAggro 0.75 | absent in pinned XMage |  |
| 25 | Contaminated Aquifer | 16.0 | DimirControl 4.0 | 40 |  |
| 26 | Boulderbranch Golem | 15.9 | Elves 0.25, Urzatron 1.62 | 44 | trigger |
| 27 | Kor Skyfisher | 15.8 | Affinity 0.38, WhiteWeenie 3.0 | 46 | trigger |
| 28 | Arbor Elf | 15.2 | Elves 0.25, GruulAggro 3.5 | 46 | activated target cost |
| 29 | Glint Hawk | 15.0 | Affinity 1.5 | 89 | trigger static target |
| 30 | Khalni Garden | 15.0 | Garden 4.0, GruulAggro 0.25, WhiteWeenie 0.5 | 39 | trigger token |
| 31 | Skred | 14.0 | Ephemerate 3.0, GruulAggro 0.5 | 73 | target |
| 32 | Utopia Sprawl | 14.0 | GruulAggro 3.5 | 98 | trigger target |
| 33 | Thraben Inspector | 14.0 | WhiteWeenie 3.5 | 38 | trigger |
| 34 | Ephemerate | 13.6 | Ephemerate 3.12, Urzatron 0.12 | 36 | target |
| 35 | Eldrazi Repurposer | 13.5 | GruulAggro 3.38 | 51 | trigger token |
| 36 | Ancient Den | 12.5 | Affinity 1.25 | 31 |  |
| 37 | Perilous Landscape | 12.5 | Ephemerate 3.12 | 64 | activated target cost cycling |
| 38 | Novice Inspector | 12.5 | WhiteWeenie 3.12 | 38 | trigger |
| 39 | Conduit Pylons | 12.4 | Urzatron 1.38 | 47 | trigger cost |
| 40 | Boarding Party | 12.0 | GruulAggro 3.0 | 41 |  |
| 41 | Brinebarrow Intruder | 12.0 | MonoBlueAggro 1.0 | 46 | trigger target |
| 42 | Battle Screech | 12.0 | WhiteWeenie 3.0 | 48 | target cost token flashback |
| 43 | Raffine's Informant | 12.0 | WhiteWeenie 3.0 | 38 | trigger |
| 44 | Smash to Smithereens | 11.6 | Burn 0, RedDeckWins 0 | 71 | static target |
| 45 | Lunarch Veteran | 11.5 | WhiteWeenie 2.88 | 62 | trigger static |
| 46 | Leonardo, Big Brother | 11.5 | WhiteWeenie 2.88 | absent in pinned XMage |  |
| 47 | Archaeomancer | 11.0 | Ephemerate 2.75 | 43 | trigger static target |
| 48 | Wild Growth | 11.0 | GruulAggro 2.75 | 52 | trigger target |
| 49 | Campfire | 10.9 | Affinity 0, Burn 0, DimirControl 0, Ephemerate 0, Garden 2.25 | 90 | activated cost |
| 50 | Cryoshatter | 10.5 | MonoBlueAggro 0.75 | 53 | trigger static target |
| 51 | Melded Moxite | 10.2 | Burn 0.75, Gates 0.62 | 48 | trigger activated cost token |
| 52 | Inventor's Axe | 9.0 | RedDeckWins 1.5 | 53 | trigger static target cost counter |
| 53 | Call Damage Control | 8.7 | Gates 0.5, GruulAggro 0.38, Urzatron 0.12 | absent in pinned XMage |  |
| 54 | Rooftop Percher | 8.2 | Elves 0, Urzatron 0.62 | 51 | trigger target |
| 55 | Snow-Covered Mountain | 8.1 | Ephemerate 1.38, GruulAggro 0.5, Jund 0.12 | 36 |  |
| 56 | Prophetic Prism | 8.0 | Affinity 0.12, Urzatron 0.75 | 41 | trigger cost |
| 57 | Jewel Thief | 8.0 | GruulAggro 2.0 | 47 | trigger token |
| 58 | Mwonvuli Acid-Moss | 8.0 | GruulAggro 2.0 | 45 | target |
| 59 | Plunder the Trollshaws | 7.5 | DimirControl 0.38, MonoBlueAggro 0.12, Urzatron 0.5 | absent in pinned XMage |  |
| 60 | Jaspera Sentinel | 7.5 | Elves 1.5 | 46 | static target cost |
| 61 | Volatile Fjord | 7.5 | Ephemerate 1.88 | 43 |  |
| 62 | Defile | 7.5 | Garden 2.5 | 45 | target |
| 63 | Annoyed Altisaur | 7.5 | GruulAggro 1.88 | 44 |  |
| 64 | Tithing Blade | 7.1 | Garden 2.38 | 59 | trigger static |
| 65 | Glacial Floodplain | 7.0 | Ephemerate 1.75 | 43 |  |
| 66 | Guardians' Pledge | 7.0 | WhiteWeenie 1.75 | 42 |  |
| 67 | Crypt Rats | 6.8 | Garden 2.25 | 59 | activated cost |
| 68 | Abandon Attachments | 6.5 | DimirControl 1.62 | 35 | cost |
| 69 | Ancient Grudge | 6.4 | Gates 0, GruulAggro 0.25, Jund 0, Urzatron 0 | 37 | target cost flashback |
| 70 | Tormod's Crypt | 6.4 | Burn 0, RedDeckWins 0, Urzatron 0 | 39 | activated target cost |
| 71 | Artful Dodge | 6.4 | MonoBlueAggro 0.38, Terror 0.62 | 39 | target cost flashback |
| 72 | Birchlore Rangers | 6.2 | Elves 1.25 | 95 | target cost |
| 73 | Gixian Infiltrator | 6.2 | Jund 1.25 | 42 | trigger static counter |
| 74 | Bender's Waterskin | 6.0 | Ephemerate 1.5 | 35 | static |
| 75 | Snow-Covered Plains | 6.0 | Ephemerate 1.5 | 36 |  |
| 76 | Thermokarst | 6.0 | GruulAggro 1.5 | 69 | target |
| 77 | Gingerbrute | 6.0 | RedDeckWins 1.0 | 60 | activated cost |
| 78 | Idyllic Grange | 5.5 | WhiteWeenie 1.38 | 64 | trigger target condition counter |
| 79 | Razortide Bridge | 5.0 | Affinity 0.5 | 40 |  |
| 80 | Gurmag Angler | 5.0 | DimirControl 1.25 | 37 |  |
| 81 | Suplex | 4.9 | Ephemerate 0.62, Gates 0, GruulAggro 0, RedDeckWins 0 | 41 | target |
| 82 | Nylea's Disciple | 4.8 | Elves 0, GruulAggro 0, Spy 0 | 41 | trigger |
| 83 | Raze | 4.5 | Burn 0, RedDeckWins 0 | 39 | static target cost |
| 84 | Suffocating Fumes | 4.5 | DimirControl 0.62, Garden 0.5 | 39 | static cost cycling |
| 85 | Salt Road Packbeast | 4.5 | Elves 0.5, WhiteWeenie 0.5 | 47 | trigger static cost affinity |
| 86 | Ride's End | 4.5 | Ephemerate 1.12 | 54 | static target condition cost |
| 87 | Deglamer | 4.5 | GruulAggro 0 | 33 | static target |
| 88 | Visionary's Dance | 4.5 | Urzatron 0.5 | absent in pinned XMage |  |
| 89 | God-Pharaoh's Faithful | 4.2 | Ephemerate 1.0 | 48 | trigger |
| 90 | Fiery Cannonade | 4.1 | Ephemerate 0, Gates 0, GruulAggro 0, Urzatron 0 | 40 |  |
| 91 | Witch's Cottage | 4.1 | Garden 1.38 | 66 | trigger static target condition |
| 92 | Sunscape Familiar | 4.0 | Ephemerate 1.0 | 54 | static cost |
| 93 | Azorius Chancery | 4.0 | Ephemerate 1.0 | 45 | trigger cost |
| 94 | Ram Through | 4.0 | GruulAggro 1.0 | 97 | static target |
| 95 | Spider-Man, Web-Slinger | 4.0 | WhiteWeenie 1.0 | 40 |  |
| 96 | Torch the Tower | 3.9 | Ephemerate 0.12, Jund 0.62 | 54 | target condition |
| 97 | Kenku Artificer | 3.8 | Affinity 0.38 | 65 | trigger static target token counter |
| 98 | Metallic Rebuke | 3.8 | Affinity 0.38 | 38 | target cost counter |
| 99 | Thorn of the Black Rose | 3.8 | DimirControl 0.88 | 44 | trigger |
| 100 | Manor Gate | 3.8 | Gates 1.88 | 48 | cost |
| 101 | Ice Tunnel | 3.5 | DimirControl 0.88 | 43 |  |
| 102 | Drown in Sorrow | 3.5 | DimirControl 0, Garden 0 | 35 |  |
| 103 | Union of the Third Path | 3.5 | Ephemerate 0.88 | 34 |  |
| 104 | Ramosian Rally | 3.5 | WhiteWeenie 0.88 | 51 | target condition cost |
| 105 | Martyr of Sands | 3.5 | WhiteWeenie 0 | 62 | activated target cost |
| 106 | Mystical Teachings | 3.4 | Urzatron 0.38 | 50 | target cost flashback |
| 107 | Haunted Fengraf | 3.4 | Urzatron 0.38 | 45 | static activated cost |
| 108 | Warped Tusker | 3.4 | Urzatron 0.38 | 58 | trigger cost token cycling |
| 109 | Earth Rift | 3.4 | Urzatron 0 | 39 | target cost flashback |
| 110 | Ghostly Flicker | 3.2 | Ephemerate 0.25, Urzatron 0.25 | 42 | target |
| 111 | Standard Bearer | 3.2 | WhiteWeenie 0 | 39 | static target |
| 112 | Haunted Mire | 3.0 | Garden 1.0 | 40 |  |
| 113 | Nutrient Block | 3.0 | Garden 1.0 | 42 | trigger |
| 114 | Cliffgate | 3.0 | Gates 1.5 | 48 | cost |
| 115 | Structural Distortion | 3.0 | GruulAggro 0.75 | 44 | target |
| 116 | Wrenn's Resolve | 3.0 | RedDeckWins 0.5 | 31 |  |
| 117 | Reckless Lackey | 3.0 | RedDeckWins 0.5 | 56 | activated cost token |
| 118 | Tectonic Hazard | 3.0 | RedDeckWins 0.25 | 36 | static target |
| 119 | Elite Interceptor | 3.0 | WhiteWeenie 0.75 | absent in pinned XMage |  |
| 120 | Accursed Marauder | 2.9 | Garden 0.62, Spy 0 | 47 | trigger token |
