# Pauper meta gap report (2026-09-09)

Source: mtgtop8 Pauper metagame, Last 2 Weeks window, 779 decks; 120 decklists sampled (8 most recent per archetype) via the MTGO export endpoint. Weight = meta share x average mainboard copies x fraction of decks (sideboard at half weight). Registry = data/cards_v1.json at 75406ffe (162 definitions).

## Archetype coverage

| archetype | share | decks sampled | mainboard copies covered | missing distinct mainboard cards (avg >= 1) |
|---|---|---|---|---|
| Burn | 12.0% | 8 | 93% | 1 |
| MonoBlueAggro | 12.0% | 8 | 90% | 2 |
| Affinity | 10.0% | 8 | 84% | 4 |
| Urzatron | 9.0% | 8 | 13% | 16 |
| RedDeckWins | 6.0% | 8 | 94% | 2 |
| Elves | 5.0% | 8 | 90% | 2 |
| Jund | 5.0% | 8 | 94% | 1 |
| DimirControl | 4.0% | 8 | 70% | 5 |
| Ephemerate | 4.0% | 8 | 30% | 16 |
| GruulAggro | 4.0% | 8 | 42% | 11 |
| Spy | 4.0% | 8 | 100% | 0 |
| WhiteWeenie | 4.0% | 8 | 10% | 11 |
| Garden | 3.0% | 8 | 57% | 9 |
| Terror | 3.0% | 8 | 95% | 1 |
| Gates | 2.0% | 8 | 74% | 5 |

## Missing cards by weight (top 120)

| rank | card | weight | archetypes (avg main copies) | xmage java lines | features |
|---|---|---|---|---|---|
| 1 | Plains | 64.8 | Affinity 0.12, Ephemerate 0.5, Gates 1.75, WhiteWeenie 15.5 | absent in pinned XMage |  |
| 2 | Giant's Boulder | 41.2 | Affinity 1.38, Urzatron 4.0 | absent in pinned XMage |  |
| 3 | Kessig Flamebreather | 36.8 | Burn 3.5 | 43 | trigger static target |
| 4 | Urza's Tower | 36.0 | Urzatron 4.0 | 37 |  |
| 5 | Urza's Power Plant | 36.0 | Urzatron 4.0 | 37 |  |
| 6 | Urza's Mine | 36.0 | Urzatron 4.0 | 37 |  |
| 7 | Expedition Map | 36.0 | Urzatron 4.0 | 45 | activated target cost |
| 8 | Utrom Monitor | 31.2 | Affinity 3.12 | absent in pinned XMage |  |
| 9 | Bramble Wurm | 27.6 | Urzatron 3.5 | 57 | trigger activated cost |
| 10 | Delver of Secrets | 27.0 | MonoBlueAggro 3.0 | 90 | trigger |
| 11 | Barrels of Blasting Jelly | 25.6 | Urzatron 3.25 | 48 | activated target cost |
| 12 | Crop Rotation | 23.6 | Urzatron 2.62 | 40 | static target cost |
| 13 | Ancient Stirrings | 20.2 | Urzatron 3.0 | 39 |  |
| 14 | Snuff Out | 18.8 | DimirControl 3.88, Garden 0.88, Jund 0.5 | 51 | static target condition cost |
| 15 | Unfathomable Truths | 17.7 | Urzatron 2.25 | 37 | token |
| 16 | Snow-Covered Island | 16.1 | Ephemerate 5.12, Terror 2.0 | 36 |  |
| 17 | Contaminated Aquifer | 16.0 | DimirControl 4.0 | 40 |  |
| 18 | Mulldrifter | 14.4 | Ephemerate 3.5, Urzatron 0.38 | 44 | trigger |
| 19 | Bojuka Bog | 14.0 | DimirControl 0.62, Garden 1.88, Jund 0.12, Urzatron 0.88, WhiteWeenie 0 | 41 | trigger target |
| 20 | Eldrazi Repurposer | 13.5 | GruulAggro 3.38 | 51 | trigger token |
| 21 | Bonder's Ornament | 13.4 | Ephemerate 0.12, Garden 0.25, GruulAggro 0.88, Urzatron 1.75 | 79 | activated cost |
| 22 | Malevolent Rumble | 13.2 | Gates 2.5, GruulAggro 2.0, Urzatron 1.5 | 37 | static token |
| 23 | Maelstrom Colossus | 12.7 | Urzatron 2.25 | 36 |  |
| 24 | Ephemerate | 12.6 | Ephemerate 3.12, Urzatron 0.12 | 36 | target |
| 25 | Arbor Elf | 12.4 | Elves 0.25, GruulAggro 3.5 | 46 | activated target cost |
| 26 | Khalni Garden | 12.4 | Garden 4.0, GruulAggro 0.25, WhiteWeenie 0.5 | 39 | trigger token |
| 27 | Utopia Sprawl | 12.2 | GruulAggro 3.5 | 98 | trigger target |
| 28 | Thraben Inspector | 12.2 | WhiteWeenie 3.5 | 38 | trigger |
| 29 | Archaeomancer | 11.0 | Ephemerate 2.75 | 43 | trigger static target |
| 30 | Perilous Landscape | 10.9 | Ephemerate 3.12 | 64 | activated target cost cycling |
| 31 | Novice Inspector | 10.9 | WhiteWeenie 3.12 | 38 | trigger |
| 32 | Conduit Pylons | 10.8 | Urzatron 1.38 | 47 | trigger cost |
| 33 | Augur of Bolas | 10.5 | DimirControl 1.5, Ephemerate 2.75 | 43 | trigger static |
| 34 | Leonardo, Big Brother | 10.1 | WhiteWeenie 2.88 | absent in pinned XMage |  |
| 35 | Wild Growth | 9.6 | GruulAggro 2.75 | 52 | trigger target |
| 36 | Skred | 9.5 | Ephemerate 3.0, GruulAggro 0.5 | 73 | target |
| 37 | Kor Skyfisher | 9.5 | Affinity 0.38, WhiteWeenie 3.0 | 46 | trigger |
| 38 | Boarding Party | 9.0 | GruulAggro 3.0 | 41 |  |
| 39 | Pinnacle Kill-Ship | 9.0 | Urzatron 2.0 | 51 | trigger target |
| 40 | Battle Screech | 9.0 | WhiteWeenie 3.0 | 48 | target cost token flashback |
| 41 | Raffine's Informant | 9.0 | WhiteWeenie 3.0 | 38 | trigger |
| 42 | Lunarch Veteran | 8.6 | WhiteWeenie 2.88 | 62 | trigger static |
| 43 | Smash to Smithereens | 7.5 | Burn 0, RedDeckWins 0 | 71 | static target |
| 44 | Boulderbranch Golem | 7.5 | Elves 0.25, Urzatron 1.62 | 44 | trigger |
| 45 | Campfire | 7.3 | Affinity 0, Burn 0, DimirControl 0, Ephemerate 0, Garden 2.25 | 90 | activated cost |
| 46 | Tithing Blade | 7.1 | Garden 2.38 | 59 | trigger static |
| 47 | Sewer-veillance Cam | 6.7 | Affinity 0.62, DimirControl 0.38, MonoBlueAggro 0.75 | absent in pinned XMage |  |
| 48 | Glacial Floodplain | 6.1 | Ephemerate 1.75 | 43 |  |
| 49 | Glint Hawk | 5.6 | Affinity 1.5 | 89 | trigger static target |
| 50 | Volatile Fjord | 5.6 | Ephemerate 1.88 | 43 |  |
| 51 | Defile | 5.6 | Garden 2.5 | 45 | target |
| 52 | Guardians' Pledge | 5.2 | WhiteWeenie 1.75 | 42 |  |
| 53 | Crypt Rats | 5.1 | Garden 2.25 | 59 | activated cost |
| 54 | Snow-Covered Mountain | 4.7 | Ephemerate 1.38, GruulAggro 0.5, Jund 0.12 | 36 |  |
| 55 | Ancient Den | 4.7 | Affinity 1.25 | 31 |  |
| 56 | Annoyed Altisaur | 4.7 | GruulAggro 1.88 | 44 |  |
| 57 | Snow-Covered Plains | 4.5 | Ephemerate 1.5 | 36 |  |
| 58 | Abandon Attachments | 4.1 | DimirControl 1.62 | 35 | cost |
| 59 | Jewel Thief | 4.0 | GruulAggro 2.0 | 47 | trigger token |
| 60 | Mwonvuli Acid-Moss | 4.0 | GruulAggro 2.0 | 45 | target |
| 61 | Deglamer | 3.9 | GruulAggro 0 | 33 | static target |
| 62 | Call Damage Control | 3.7 | Gates 0.5, GruulAggro 0.38, Urzatron 0.12 | absent in pinned XMage |  |
| 63 | Idyllic Grange | 3.4 | WhiteWeenie 1.38 | 64 | trigger target condition counter |
| 64 | Ride's End | 3.4 | Ephemerate 1.12 | 54 | static target condition cost |
| 65 | Inventor's Axe | 3.4 | RedDeckWins 1.5 | 53 | trigger static target cost counter |
| 66 | Gurmag Angler | 3.1 | DimirControl 1.25 | 37 |  |
| 67 | Gixian Infiltrator | 3.1 | Jund 1.25 | 42 | trigger static counter |
| 68 | Witch's Cottage | 3.1 | Garden 1.38 | 66 | trigger static target condition |
| 69 | Drown in Sorrow | 3.1 | DimirControl 0, Garden 0 | 35 |  |
| 70 | Bender's Waterskin | 3.0 | Ephemerate 1.5 | 35 | static |
| 71 | Haunted Mire | 3.0 | Garden 1.0 | 40 |  |
| 72 | Brinebarrow Intruder | 3.0 | MonoBlueAggro 1.0 | 46 | trigger target |
| 73 | Ancient Grudge | 2.9 | Gates 0, GruulAggro 0.25, Jund 0, Urzatron 0 | 37 | target cost flashback |
| 74 | Jaspera Sentinel | 2.8 | Elves 1.5 | 46 | static target cost |
| 75 | Cryoshatter | 2.6 | MonoBlueAggro 0.75 | 53 | trigger static target |
| 76 | Melded Moxite | 2.6 | Burn 0.75, Gates 0.62 | 48 | trigger activated cost token |
| 77 | Spider-Man, Web-Slinger | 2.5 | WhiteWeenie 1.0 | 40 |  |
| 78 | Suffocating Fumes | 2.4 | DimirControl 0.62, Garden 0.5 | 39 | static cost cycling |
| 79 | Birchlore Rangers | 2.3 | Elves 1.25 | 95 | target cost |
| 80 | Manor Gate | 2.3 | Gates 1.88 | 48 | cost |
| 81 | Artful Dodge | 2.3 | MonoBlueAggro 0.38, Terror 0.62 | 39 | target cost flashback |
| 82 | Rooftop Percher | 2.3 | Elves 0, Urzatron 0.62 | 51 | trigger target |
| 83 | Thermokarst | 2.2 | GruulAggro 1.5 | 69 | target |
| 84 | Arms of Hadar | 2.2 | DimirControl 0 | 43 | target |
| 85 | Union of the Third Path | 2.2 | Ephemerate 0.88 | 34 |  |
| 86 | Standard Bearer | 2.0 | WhiteWeenie 0 | 39 | static target |
| 87 | Nylea's Disciple | 2.0 | Elves 0, GruulAggro 0, Spy 0 | 41 | trigger |
| 88 | Suplex | 1.9 | Ephemerate 0.62, Gates 0, GruulAggro 0, RedDeckWins 0 | 41 | target |
| 89 | Razortide Bridge | 1.9 | Affinity 0.5 | 40 |  |
| 90 | Prophetic Prism | 1.8 | Affinity 0.12, Urzatron 0.75 | 41 | trigger cost |
| 91 | Thorn of the Black Rose | 1.8 | DimirControl 0.88 | 44 | trigger |
| 92 | Ice Tunnel | 1.8 | DimirControl 0.88 | 43 |  |
| 93 | Ramosian Rally | 1.8 | WhiteWeenie 0.88 | 51 | target condition cost |
| 94 | Martyr of Sands | 1.8 | WhiteWeenie 0 | 62 | activated target cost |
| 95 | Cliffgate | 1.5 | Gates 1.5 | 48 | cost |
| 96 | Ram Through | 1.5 | GruulAggro 1.0 | 97 | static target |
| 97 | Gingerbrute | 1.5 | RedDeckWins 1.0 | 60 | activated cost |
| 98 | Raze | 1.3 | Burn 0, RedDeckWins 0 | 39 | static target cost |
| 99 | Plunder the Trollshaws | 1.3 | DimirControl 0.38, MonoBlueAggro 0.12, Urzatron 0.5 | absent in pinned XMage |  |
| 100 | Accursed Marauder | 1.3 | Garden 0.62, Spy 0 | 47 | trigger token |
| 101 | Haunted Fengraf | 1.3 | Urzatron 0.38 | 45 | static activated cost |
| 102 | Earth Rift | 1.3 | Urzatron 0 | 39 | target cost flashback |
| 103 | Behold the Multiverse | 1.2 | Ephemerate 0.62 | 36 |  |
| 104 | Golgari Rot Farm | 1.1 | Garden 0.75 | 45 | trigger cost |
| 105 | Structural Distortion | 1.1 | GruulAggro 0.75 | 44 | target |
| 106 | Elite Interceptor | 1.1 | WhiteWeenie 0.75 | absent in pinned XMage |  |
| 107 | Tormod's Crypt | 1.1 | Burn 0, RedDeckWins 0, Urzatron 0 | 39 | activated target cost |
| 108 | God-Pharaoh's Faithful | 1.0 | Ephemerate 1.0 | 48 | trigger |
| 109 | Sunscape Familiar | 1.0 | Ephemerate 1.0 | 54 | static cost |
| 110 | Azorius Chancery | 1.0 | Ephemerate 1.0 | 45 | trigger cost |
| 111 | Fiery Cannonade | 1.0 | Ephemerate 0, Gates 0, GruulAggro 0, Urzatron 0 | 40 |  |
| 112 | Kenku Artificer | 0.9 | Affinity 0.38 | 65 | trigger static target token counter |
| 113 | Torch the Tower | 0.9 | Ephemerate 0.12, Jund 0.62 | 54 | target condition |
| 114 | Salt Road Packbeast | 0.9 | Elves 0.5, WhiteWeenie 0.5 | 47 | trigger static cost affinity |
| 115 | Holy Light | 0.9 | WhiteWeenie 0 | 43 |  |
| 116 | Destroy Evil | 0.9 | Ephemerate 0, Gates 0, WhiteWeenie 0 | 51 | target |
| 117 | Fang Dragon | 0.8 | Spy 0 | 45 | static |
| 118 | Nutrient Block | 0.8 | Garden 1.0 | 42 | trigger |
| 119 | Fungal Infection | 0.8 | Garden 0.5 | 37 | target token |
| 120 | Spinning Darkness | 0.8 | Garden 0.5 | 109 | static target cost |
