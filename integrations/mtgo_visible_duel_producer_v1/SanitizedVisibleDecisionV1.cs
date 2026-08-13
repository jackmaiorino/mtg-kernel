using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;

namespace MtgKernel.Mtgo.VisibleDuelProducer.V1
{
    public static partial class VisibleDuelProducerV1
    {
        private sealed class ReferenceIdentityComparerV1 : IEqualityComparer<object>
        {
            internal static readonly ReferenceIdentityComparerV1 Instance =
                new ReferenceIdentityComparerV1();

            public new bool Equals(object? left, object? right)
            {
                return ReferenceEquals(left, right);
            }

            public int GetHashCode(object value)
            {
                return RuntimeHelpers.GetHashCode(value);
            }
        }

        [DataContract]
        private sealed class VisibleObjectRefV1
        {
            [DataMember(Name = "visible_ordinal", Order = 1)]
            public uint VisibleOrdinal { get; set; }
        }

        [DataContract]
        private sealed class VisibleNamedCardV1
        {
            [DataMember(Name = "object_ref", Order = 1)]
            public VisibleObjectRefV1 ObjectRef { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "card_name", Order = 2)]
            public string CardName { get; set; } = string.Empty;
        }

        [DataContract]
        private sealed class VisibleExileCardV1
        {
            [DataMember(Name = "object_ref", Order = 1)]
            public VisibleObjectRefV1 ObjectRef { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "zone_owner", Order = 2)]
            public string ZoneOwner { get; set; } = string.Empty;

            [DataMember(Name = "visible_card_name", Order = 3)]
            public string? VisibleCardName { get; set; }
        }

        [DataContract]
        private sealed class VisibleCounterStateV1
        {
            [DataMember(Name = "plus_one_plus_one", Order = 1)]
            public short PlusOnePlusOne { get; set; }

            [DataMember(Name = "minus_one_minus_one", Order = 2)]
            public short MinusOneMinusOne { get; set; }

            [DataMember(Name = "minus_zero_minus_one", Order = 3)]
            public short MinusZeroMinusOne { get; set; }

            [DataMember(Name = "stun", Order = 4)]
            public short Stun { get; set; }

            [DataMember(Name = "lore", Order = 5)]
            public short Lore { get; set; }
        }

        [DataContract]
        private sealed class VisibleBattlefieldCardV1
        {
            [DataMember(Name = "object_ref", Order = 1)]
            public VisibleObjectRefV1 ObjectRef { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "card_name", Order = 2)]
            public string CardName { get; set; } = string.Empty;

            [DataMember(Name = "tapped", Order = 3)]
            public bool Tapped { get; set; }

            [DataMember(Name = "marked_damage", Order = 4)]
            public ushort MarkedDamage { get; set; }

            [DataMember(Name = "counters", Order = 5)]
            public VisibleCounterStateV1 Counters { get; set; } = new VisibleCounterStateV1();

            [DataMember(Name = "is_token", Order = 6)]
            public bool IsToken { get; set; }

            [DataMember(Name = "visible_effective_power", Order = 7)]
            public int? VisibleEffectivePower { get; set; }

            [DataMember(Name = "visible_effective_toughness", Order = 8)]
            public int? VisibleEffectiveToughness { get; set; }
        }

        [DataContract]
        private sealed class VisibleRelationV1
        {
            [DataMember(Name = "relation_kind", Order = 1)]
            public string RelationKind { get; set; } = "attached_to";

            [DataMember(Name = "object", Order = 2)]
            public VisibleObjectRefV1 Object { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "attached_to", Order = 3)]
            public VisibleObjectRefV1 AttachedTo { get; set; } = new VisibleObjectRefV1();
        }

        [DataContract]
        private sealed class VisibleCombatStateV1
        {
            [DataMember(Name = "attackers_declared", Order = 1)]
            public bool AttackersDeclared { get; set; }

            [DataMember(Name = "blockers_declared", Order = 2)]
            public bool BlockersDeclared { get; set; }

            [DataMember(Name = "ordered_attackers", Order = 3)]
            public List<VisibleObjectRefV1> OrderedAttackers { get; set; } =
                new List<VisibleObjectRefV1>();

            [DataMember(Name = "blocker_assignments", Order = 4)]
            public List<object> BlockerAssignments { get; set; } = new List<object>();
        }

        [DataContract]
        private sealed class VisibleStateV1
        {
            [DataMember(Name = "acting_player", Order = 1)]
            public string ActingPlayer { get; set; } = "seated_player";

            [DataMember(Name = "turn", Order = 2)]
            public uint Turn { get; set; }

            [DataMember(Name = "phase", Order = 3)]
            public string Phase { get; set; } = string.Empty;

            [DataMember(Name = "active_player", Order = 4)]
            public string ActivePlayer { get; set; } = string.Empty;

            [DataMember(Name = "priority_player", Order = 5)]
            public string PriorityPlayer { get; set; } = string.Empty;

            [DataMember(Name = "initiative", Order = 6)]
            public string? Initiative { get; set; }

            [DataMember(Name = "life_totals", Order = 7)]
            public int[] LifeTotals { get; set; } = Array.Empty<int>();

            [DataMember(Name = "mana_pools", Order = 8)]
            public int[][] ManaPools { get; set; } = Array.Empty<int[]>();

            [DataMember(Name = "hand_counts", Order = 9)]
            public int[] HandCounts { get; set; } = Array.Empty<int>();

            [DataMember(Name = "library_counts", Order = 10)]
            public int[] LibraryCounts { get; set; } = Array.Empty<int>();

            [DataMember(Name = "battlefield", Order = 11)]
            public List<VisibleBattlefieldCardV1>[] Battlefield { get; set; } =
                Array.Empty<List<VisibleBattlefieldCardV1>>();

            [DataMember(Name = "graveyards", Order = 12)]
            public List<VisibleNamedCardV1>[] Graveyards { get; set; } =
                Array.Empty<List<VisibleNamedCardV1>>();

            [DataMember(Name = "exile", Order = 13)]
            public List<VisibleExileCardV1> Exile { get; set; } = new List<VisibleExileCardV1>();

            [DataMember(Name = "stack", Order = 14)]
            public List<object> Stack { get; set; } = new List<object>();

            [DataMember(Name = "combat", Order = 15)]
            public VisibleCombatStateV1 Combat { get; set; } = new VisibleCombatStateV1();

            [DataMember(Name = "visible_object_relations", Order = 16)]
            public List<VisibleRelationV1> VisibleObjectRelations { get; set; } =
                new List<VisibleRelationV1>();

            [DataMember(Name = "own_hand", Order = 17)]
            public List<VisibleNamedCardV1> OwnHand { get; set; } = new List<VisibleNamedCardV1>();

            [DataMember(Name = "known_library_cards", Order = 18)]
            public List<object>[] KnownLibraryCards { get; set; } = Array.Empty<List<object>>();

            [DataMember(Name = "known_hand_cards", Order = 19)]
            public List<VisibleNamedCardV1>[] KnownHandCards { get; set; } =
                Array.Empty<List<VisibleNamedCardV1>>();
        }

        [DataContract]
        private sealed class VisibleDecisionV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "ordered_legal_actions", Order = 2)]
            public List<Dictionary<string, object?>> OrderedLegalActions { get; set; } =
                new List<Dictionary<string, object?>>();
        }

        [DataContract]
        private sealed class VisibleDecisionResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } = "visible_decision";

            [DataMember(Name = "decision", Order = 2)]
            public VisibleDecisionV1 Decision { get; set; } = new VisibleDecisionV1();
        }

        private sealed class PlayerSnapshotV1
        {
            internal object Player = new object();
            internal bool Local;
            internal bool Active;
            internal bool Priority;
            internal int Life;
            internal int HandCount;
            internal int LibraryCount;
            internal int[] ManaPool = new int[6];
            internal List<object> Battlefield = new List<object>();
            internal List<object> Hand = new List<object>();
            internal List<object> Graveyard = new List<object>();
            internal List<object> Exile = new List<object>();
            internal List<object> Revealed = new List<object>();
            internal List<object> Shields = new List<object>();
        }

        private static bool TryBuildFirstSanitizedVisibleDecisionV1(
            object viewModel,
            out byte[] result)
        {
            return TryBuildSanitizedVisibleDecisionAndActionBindingsV1(
                viewModel,
                out result,
                out _);
        }

        internal static bool TryBuildSanitizedVisibleDecisionAndActionBindingsV1(
            object viewModel,
            out byte[] result,
            out List<object> boundClientActions)
        {
            result = Array.Empty<byte>();
            boundClientActions = new List<object>();
            try
            {
                if (!TryBuildFirstSanitizedVisibleDecisionCoreV1(
                        viewModel,
                        out VisibleDecisionResultV1 payload,
                        out boundClientActions))
                {
                    return false;
                }
                var settings = new DataContractJsonSerializerSettings
                {
                    UseSimpleDictionaryFormat = true,
                    KnownTypes = new[]
                    {
                        typeof(Dictionary<string, object?>),
                        typeof(VisibleObjectRefV1)
                    },
                    EmitTypeInformation = EmitTypeInformation.Never
                };
                var serializer = new DataContractJsonSerializer(
                    typeof(VisibleDecisionResultV1),
                    settings);
                using (var stream = new MemoryStream())
                {
                    serializer.WriteObject(stream, payload);
                    if (stream.Length <= 0 || stream.Length > MaximumOutputBytes - OutputPayloadOffset)
                    {
                        return false;
                    }
                    result = stream.ToArray();
                }
                return true;
            }
            catch
            {
                result = Array.Empty<byte>();
                boundClientActions = new List<object>();
                return false;
            }
        }

        private static bool TryBuildFirstSanitizedVisibleDecisionCoreV1(
            object viewModel,
            out VisibleDecisionResultV1 result,
            out List<object> boundClientActions)
        {
            result = new VisibleDecisionResultV1();
            boundClientActions = new List<object>();
            if (!TryRequireSupportedDuelVariantV1(viewModel) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CurrentPhase",
                    out object? phaseValue) ||
                !TryMapVisiblePhaseV1(phaseValue, out string phase) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "GameTurnText",
                    out object? turnTextValue) ||
                !TryParseVisibleTurnV1(turnTextValue, out uint turn) || turn != 1 ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Players",
                    out object? playersValue) ||
                !TryBoundedCollectionV1(playersValue, 2, out List<object> players) ||
                players.Count != 2)
            {
                return false;
            }

            var snapshots = new List<PlayerSnapshotV1>();
            foreach (object player in players)
            {
                if (!TryBuildPlayerSnapshotV1(player, out PlayerSnapshotV1 snapshot))
                {
                    return false;
                }
                snapshots.Add(snapshot);
            }
            if (snapshots.Count(player => player.Local) != 1 ||
                snapshots.Count(player => player.Active) != 1 ||
                snapshots.Count(player => player.Priority) != 1)
            {
                return false;
            }
            PlayerSnapshotV1 seated = snapshots.Single(player => player.Local);
            PlayerSnapshotV1 opponent = snapshots.Single(player => !player.Local);
            if (!TryMapVisibleInitiativeHolderV1(
                    seated,
                    opponent,
                    out string? visibleInitiative) ||
                visibleInitiative != null)
            {
                // The source mapping is structurally exercised, but a non-null
                // holder remains closed until an exact live UI corpus qualifies
                // the Shields-zone player association for this client version.
                return false;
            }
            // The first emitted slice deliberately excludes temporary revealed
            // zones. Until every revealed-zone presentation can be assigned to
            // the exact public state field, omitting one would be incomplete.
            // The first live-success candidate is deliberately restricted to
            // the untouched 60-card, seven-card-hand opening state. Combined
            // with empty public zones, zero mana, and base life, this proves
            // that no game action has occurred and Initiative is absent.
            // Later states abstain until a visible Initiative source is
            // reconstructed.
            if (!seated.Priority || phase != "main1" ||
                snapshots.Any(player => player.Life != 20 ||
                    player.HandCount != 7 || player.LibraryCount != 53 ||
                    player.ManaPool.Any(mana => mana != 0) ||
                    player.Battlefield.Count != 0 ||
                    player.Graveyard.Count != 0 || player.Exile.Count != 0) ||
                seated.Revealed.Count != 0 || opponent.Revealed.Count != 0 ||
                (opponent.Hand.Count != 0 && opponent.Hand.Count != opponent.HandCount) ||
                !TryRequireNoUnrepresentedVisibleModalSurfaceV1(viewModel) ||
                !TryRequireNoVisiblePromptChoiceV1(viewModel))
            {
                return false;
            }

            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "StackZone",
                    out object? stackZone) ||
                stackZone == null ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Count",
                    out object? stackCountValue) ||
                !(stackCountValue is int stackCount) || stackCount != 0)
            {
                return false;
            }

            var objectRefs = new Dictionary<object, VisibleObjectRefV1>(
                ReferenceIdentityComparerV1.Instance);
            uint nextOrdinal = 0;
            Func<object, VisibleObjectRefV1> register = card =>
            {
                if (!objectRefs.TryGetValue(card, out VisibleObjectRefV1? visibleRef))
                {
                    visibleRef = new VisibleObjectRefV1 { VisibleOrdinal = nextOrdinal++ };
                    objectRefs.Add(card, visibleRef);
                }
                return visibleRef;
            };

            foreach (object card in seated.Battlefield) register(card);
            foreach (object card in opponent.Battlefield) register(card);
            foreach (object card in seated.Graveyard) register(card);
            foreach (object card in opponent.Graveyard) register(card);
            foreach (object card in seated.Exile) register(card);
            foreach (object card in opponent.Exile) register(card);
            foreach (object card in seated.Hand) register(card);
            foreach (object card in opponent.Hand) register(card);

            if (!TryMapBattlefieldV1(seated.Battlefield, objectRefs, out List<VisibleBattlefieldCardV1> seatedBattlefield, out List<VisibleRelationV1> relations) ||
                !TryMapBattlefieldV1(opponent.Battlefield, objectRefs, out List<VisibleBattlefieldCardV1> opponentBattlefield, out List<VisibleRelationV1> opponentRelations) ||
                !TryMapNamedCardsV1(seated.Graveyard, objectRefs, out List<VisibleNamedCardV1> seatedGraveyard) ||
                !TryMapNamedCardsV1(opponent.Graveyard, objectRefs, out List<VisibleNamedCardV1> opponentGraveyard) ||
                !TryMapExileV1(seated.Exile, "seated_player", objectRefs, out List<VisibleExileCardV1> seatedExile) ||
                !TryMapExileV1(opponent.Exile, "opponent", objectRefs, out List<VisibleExileCardV1> opponentExile) ||
                !TryMapNamedCardsV1(seated.Hand, objectRefs, out List<VisibleNamedCardV1> ownHand) ||
                !TryMapNamedCardsV1(opponent.Hand, objectRefs, out List<VisibleNamedCardV1> opponentKnownHand))
            {
                return false;
            }
            relations.AddRange(opponentRelations);
            seatedExile.AddRange(opponentExile);
            if (seatedBattlefield.Any(card => card.ObjectRef == null) ||
                opponentBattlefield.Any(card => card.ObjectRef == null))
            {
                return false;
            }

            var visibleCardsForActions = new List<object>();
            visibleCardsForActions.AddRange(seated.Battlefield);
            visibleCardsForActions.AddRange(seated.Hand);
            visibleCardsForActions.AddRange(seated.Graveyard);
            visibleCardsForActions.AddRange(seated.Exile);
            if (!TryMapBasicVisibleActionsV1(
                    visibleCardsForActions,
                    seated.Hand,
                    objectRefs,
                    out List<Dictionary<string, object?>> actions,
                    out List<object> actionBindings))
            {
                return false;
            }
            if (!TryMapVisiblePriorityPassControlV1(
                    viewModel,
                    out Dictionary<string, object?> pass,
                    out object passAction))
            {
                return false;
            }
            actions.Add(pass);
            actionBindings.Add(passAction);
            if (actions.Count > 64)
            {
                return false;
            }

            var state = new VisibleStateV1
            {
                Turn = turn,
                Phase = phase,
                ActivePlayer = seated.Active ? "seated_player" : "opponent",
                PriorityPlayer = "seated_player",
                Initiative = visibleInitiative,
                LifeTotals = new[] { seated.Life, opponent.Life },
                ManaPools = new[] { seated.ManaPool, opponent.ManaPool },
                HandCounts = new[] { seated.HandCount, opponent.HandCount },
                LibraryCounts = new[] { seated.LibraryCount, opponent.LibraryCount },
                Battlefield = new[] { seatedBattlefield, opponentBattlefield },
                Graveyards = new[] { seatedGraveyard, opponentGraveyard },
                Exile = seatedExile,
                Stack = new List<object>(),
                Combat = new VisibleCombatStateV1(),
                VisibleObjectRelations = relations,
                OwnHand = ownHand,
                KnownLibraryCards = new[] { new List<object>(), new List<object>() },
                KnownHandCards = new[]
                {
                    new List<VisibleNamedCardV1>(),
                    opponentKnownHand
                }
            };
            result.Decision = new VisibleDecisionV1
            {
                CurrentState = state,
                OrderedLegalActions = actions
            };
            boundClientActions = actionBindings;
            return true;
        }

        private static bool TryRequireSupportedDuelVariantV1(object viewModel)
        {
            foreach (string propertyName in new[] { "IsCommander", "IsPlanechase" })
            {
                if (!TryReadExactPropertyV1(
                        viewModel,
                        "DuelScene",
                        DuelViewModelType,
                        propertyName,
                        out object? value) ||
                    !(value is bool enabled) || enabled)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryRequireNoUnrepresentedVisibleModalSurfaceV1(
            object viewModel)
        {
            foreach (string propertyName in new[]
            {
                "IsPileZoneActive",
                "IsWishingFromSideboard",
                "LocalTriggersPanelEnabled",
                "OpponentTriggersPanelEnabled",
                "StormCounterVisible",
                "ThreePilePanelEnabled",
                "TwoPilePanelEnabled"
            })
            {
                if (!TryReadExactPropertyV1(
                        viewModel,
                        "DuelScene",
                        DuelViewModelType,
                        propertyName,
                        out object? value) ||
                    !(value is bool visible) || visible)
                {
                    return false;
                }
            }

            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CardSelection",
                    out object? cardSelection) ||
                cardSelection == null ||
                !TryReadExactPropertyV1(
                    cardSelection,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.CardSelectionViewModel",
                    "Visible",
                    out object? selectionVisible) ||
                !(selectionVisible is bool selectionIsVisible) || selectionIsVisible ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CardSelectorDialog",
                    out object? cardSelectorDialog) ||
                cardSelectorDialog == null ||
                !TryReadExactPropertyV1(
                    cardSelectorDialog,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.CardSelectorDialogViewModel",
                    "Visible",
                    out object? dialogVisible) ||
                !(dialogVisible is bool dialogIsVisible) || dialogIsVisible ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CardSelectors",
                    out object? cardSelectors) ||
                cardSelectors == null ||
                !TryReadExactPropertyV1(
                    cardSelectors,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.CardSelectorManager",
                    "Collection",
                    out object? selectorCollection) ||
                !TryBoundedCollectionV1(selectorCollection, 64, out List<object> selectors) ||
                selectors.Count != 0 ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "TemporaryZones",
                    out object? temporaryZonesValue) ||
                !TryBoundedCollectionV1(
                    temporaryZonesValue,
                    MaximumVisibleCollectionItems,
                    out List<object> temporaryZones))
            {
                return false;
            }
            foreach (object temporaryZone in temporaryZones)
            {
                if (!TryReadExactPropertyV1(
                        temporaryZone,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.TemporaryZoneViewModel",
                        "IsVisible",
                        out object? visibleValue) ||
                    !(visibleValue is bool visible) || visible)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryBuildPlayerSnapshotV1(
            object player,
            out PlayerSnapshotV1 snapshot)
        {
            snapshot = new PlayerSnapshotV1 { Player = player };
            const string playerType = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
            if (!TryReadExactPropertyV1(player, "DuelScene", playerType, "LocalPlayer", out object? localValue) || !(localValue is bool local) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "Active", out object? activeValue) || !(activeValue is bool active) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "MatActive", out object? priorityValue) || !(priorityValue is bool priority) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "Health", out object? lifeValue) || !(lifeValue is int life) || life < -1000000 || life > 1000000 ||
                !TryRequireNoUnrepresentedVisiblePlayerCountersV1(player) ||
                !TryRequireNoVisibleCompanionPanelV1(player) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "HandTotal", out object? handCountValue) || !(handCountValue is int handCount) || handCount < 0 || handCount > 10000 ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "DeckTotal", out object? deckCountValue) || !(deckCountValue is int deckCount) || deckCount < 0 || deckCount > 10000 ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "ManaPoolItems", out object? manaItemsValue) ||
                !TryBoundedCollectionV1(manaItemsValue, 16, out List<object> manaItems) ||
                !TryMapManaPoolV1(manaItems, out int[] manaPool) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "BattlefieldCards", out object? battlefieldValue) ||
                !TryBoundedCollectionV1(battlefieldValue, 512, out List<object> battlefield) ||
                !TryReadZoneCardsForSnapshotV1(player, "HandZone", local, out List<object> hand) ||
                !TryReadZoneCardsForSnapshotV1(player, "GraveyardZone", true, out List<object> graveyard) ||
                !TryReadZoneCardsForSnapshotV1(player, "ExileZone", true, out List<object> exile) ||
                !TryReadZoneCardsForSnapshotV1(player, "RevealedZone", false, out List<object> revealed) ||
                !TryReadZoneCardsForSnapshotV1(player, "ShieldsZone", false, out List<object> shields) ||
                (local && hand.Count != handCount))
            {
                return false;
            }
            snapshot.Local = local;
            snapshot.Active = active;
            snapshot.Priority = priority;
            snapshot.Life = life;
            snapshot.HandCount = handCount;
            snapshot.LibraryCount = deckCount;
            snapshot.ManaPool = manaPool;
            snapshot.Battlefield = battlefield;
            snapshot.Hand = hand;
            snapshot.Graveyard = graveyard;
            snapshot.Exile = exile;
            snapshot.Revealed = revealed;
            snapshot.Shields = shields;
            return true;
        }

        private static bool TryRequireNoVisibleCompanionPanelV1(object player)
        {
            const string playerType = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            return TryReadExactPropertyV1(
                    player,
                    "DuelScene",
                    playerType,
                    "CompanionZone",
                    out object? zone) &&
                zone != null &&
                TryReadExactPropertyV1(
                    zone,
                    "DuelScene",
                    zoneType,
                    "IsVisible",
                    out object? visibleValue) &&
                visibleValue is bool visible && !visible;
        }

        private static bool TryRequireNoUnrepresentedVisiblePlayerCountersV1(
            object player)
        {
            const string playerType = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
            foreach (string propertyName in new[]
            {
                "HasEnergyCounters",
                "HasExperienceCounters",
                "HasPoisonCounters",
                "HasRadCounters"
            })
            {
                if (!TryReadExactPropertyV1(
                        player,
                        "DuelScene",
                        playerType,
                        propertyName,
                        out object? value) ||
                    !(value is bool present) || present)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryMapVisibleInitiativeHolderV1(
            PlayerSnapshotV1 seated,
            PlayerSnapshotV1 opponent,
            out string? holder)
        {
            holder = null;
            if (!TryCountVisibleInitiativeEmblemsV1(
                    seated.Shields,
                    out int seatedCount) ||
                !TryCountVisibleInitiativeEmblemsV1(
                    opponent.Shields,
                    out int opponentCount) ||
                seatedCount != seated.Shields.Count ||
                opponentCount != opponent.Shields.Count ||
                seatedCount > 1 || opponentCount > 1 ||
                seatedCount + opponentCount > 1)
            {
                return false;
            }
            if (seatedCount == 1)
            {
                holder = "seated_player";
            }
            else if (opponentCount == 1)
            {
                holder = "opponent";
            }
            return true;
        }

        private static bool TryCountVisibleInitiativeEmblemsV1(
            List<object> shields,
            out int count)
        {
            count = 0;
            const string cardType = "Shiny.Card.ViewModels.CardViewModel";
            foreach (object card in shields)
            {
                if (!TryReadExactPropertyV1(
                        card,
                        "Card",
                        cardType,
                        "CardFrameID",
                        out object? frameValue) ||
                    frameValue == null ||
                    frameValue.GetType().FullName != "Shiny.Card.Enums.FrameStyle")
                {
                    return false;
                }
                string? frameName = Enum.GetName(frameValue.GetType(), frameValue);
                if (frameName == null)
                {
                    return false;
                }
                if (string.Equals(
                        frameName,
                        "CLBInitiativeEmblem",
                        StringComparison.Ordinal))
                {
                    count++;
                }
            }
            return true;
        }

        private static bool TryRequireNoVisiblePromptChoiceV1(object viewModel)
        {
            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "PromptBox",
                    out object? promptBox) ||
                promptBox == null ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
                    "IsPromptBoxActive",
                    out object? activeValue) ||
                !(activeValue is bool active) || active ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.PromptBoxViewModel",
                    "StandardButtons",
                    out object? buttonsValue) ||
                !TryBoundedCollectionV1(buttonsValue, 64, out List<object> buttons))
            {
                return false;
            }
            foreach (object button in buttons)
            {
                if (!TryReadExactPropertyV1(
                        button,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.OptionButton",
                        "Visible",
                        out object? visibleValue) || !(visibleValue is bool visible) ||
                    !TryReadExactPropertyV1(
                        button,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.OptionButton",
                        "Enabled",
                        out object? enabledValue) || !(enabledValue is bool enabled) ||
                    (visible && enabled))
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryReadZoneCardsForSnapshotV1(
            object player,
            string zoneProperty,
            bool alwaysVisible,
            out List<object> cards)
        {
            cards = new List<object>();
            const string playerType = "Shiny.Play.Duel.ViewModel.PlayerViewModel";
            const string zoneType = "Shiny.Play.Duel.ViewModel.ZoneViewModel";
            if (!TryReadExactPropertyV1(player, "DuelScene", playerType, zoneProperty, out object? zone) ||
                zone == null ||
                !TryReadExactPropertyV1(zone, "DuelScene", zoneType, "IsVisible", out object? visibleValue) ||
                !(visibleValue is bool visible))
            {
                return false;
            }
            if (!alwaysVisible && !visible)
            {
                return true;
            }
            if (!TryReadExactPropertyV1(zone, "DuelScene", zoneType, "Count", out object? countValue) ||
                !(countValue is int count) || count < 0 || count > MaximumVisibleCollectionItems ||
                !TryReadExactPropertyV1(zone, "DuelScene", zoneType, "Cards", out object? cardsValue) ||
                !TryBoundedCollectionV1(cardsValue, MaximumVisibleCollectionItems, out cards) ||
                cards.Count != count)
            {
                cards = new List<object>();
                return false;
            }
            return true;
        }

        private static bool TryMapVisiblePhaseV1(object? value, out string phase)
        {
            phase = string.Empty;
            if (value == null || value.GetType().FullName != "WotC.MtGO.Client.Model.Play.GamePhase")
            {
                return false;
            }
            switch (Enum.GetName(value.GetType(), value))
            {
                case "Untap": phase = "untap"; break;
                case "Upkeep": phase = "upkeep"; break;
                case "Draw": phase = "draw"; break;
                case "PreCombatMain": phase = "main1"; break;
                case "BeginCombat": phase = "begin_combat"; break;
                case "DeclareAttackers": phase = "declare_attackers"; break;
                case "DeclareBlockers": phase = "declare_blockers"; break;
                case "CombatDamage": phase = "combat_damage"; break;
                case "EndOfCombat": phase = "end_combat"; break;
                case "PostCombatMain": phase = "main2"; break;
                case "EndOfTurn": phase = "end"; break;
                case "Cleanup": phase = "cleanup"; break;
                default: return false;
            }
            return true;
        }

        private static bool TryParseVisibleTurnV1(object? value, out uint turn)
        {
            turn = 0;
            const string prefix = "Turn ";
            if (!(value is string text) ||
                !IsBoundedVisibleStringV1(text, 128, false) ||
                !text.StartsWith(prefix, StringComparison.Ordinal))
            {
                return false;
            }

            int digitStart = prefix.Length;
            int cursor = digitStart;
            while (cursor < text.Length &&
                text[cursor] >= '0' && text[cursor] <= '9')
            {
                cursor++;
            }
            int digitCount = cursor - digitStart;
            if (digitCount == 0 || digitCount > 10 ||
                (digitCount > 1 && text[digitStart] == '0'))
            {
                return false;
            }
            if (cursor < text.Length &&
                (cursor + 2 >= text.Length ||
                    text[cursor] != ':' || text[cursor + 1] != ' ' ||
                    !text.Substring(cursor + 2).Any(
                        character => !char.IsWhiteSpace(character))))
            {
                return false;
            }

            return uint.TryParse(
                    text.Substring(digitStart, digitCount),
                    System.Globalization.NumberStyles.None,
                    System.Globalization.CultureInfo.InvariantCulture,
                    out turn) &&
                turn >= 1 && turn <= 1000000;
        }

        private static bool TryMapManaPoolV1(List<object> items, out int[] manaPool)
        {
            manaPool = new int[6];
            foreach (object item in items)
            {
                if (!TryReadExactPrivateVisibleActionPropertyV1(
                        item,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel",
                        "Color",
                        out object? colorValue) ||
                    colorValue == null ||
                    colorValue.GetType().FullName != "WotC.MtGO.Client.Model.MagicColors" ||
                    !TryReadExactPropertyV1(
                        item,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.ManaPoolItemViewModel",
                        "Count",
                        out object? countValue) ||
                    !(countValue is int count) || count < 0 || count > 100)
                {
                    return false;
                }
                string? colorName = Enum.GetName(colorValue.GetType(), colorValue);
                int index;
                switch (colorName)
                {
                    case "White": index = 0; break;
                    case "Blue": index = 1; break;
                    case "Black": index = 2; break;
                    case "Red": index = 3; break;
                    case "Green": index = 4; break;
                    case "Colorless": index = 5; break;
                    default: return false;
                }
                manaPool[index] = checked(manaPool[index] + count);
                if (manaPool[index] > 100)
                {
                    return false;
                }
            }
            return true;
        }

        private static bool TryMapBattlefieldV1(
            List<object> cards,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleBattlefieldCardV1> mapped,
            out List<VisibleRelationV1> relations)
        {
            mapped = new List<VisibleBattlefieldCardV1>();
            relations = new List<VisibleRelationV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsTapped", out object? tappedValue) || !(tappedValue is bool tapped) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsAttacking", out object? attackingValue) || !(attackingValue is bool attacking) || attacking ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsBlocking", out object? blockingValue) || !(blockingValue is bool blocking) || blocking ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "CurrentDamage", out object? damageValue) || !(damageValue is int damage) || damage < 0 || damage > ushort.MaxValue ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Power", out object? powerValue) || (powerValue != null && !(powerValue is int)) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Toughness", out object? toughnessValue) || (toughnessValue != null && !(toughnessValue is int)) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "IsToken", out object? tokenValue) || !(tokenValue is bool token) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "VisibleCounters", out object? countersValue) ||
                    !TryBoundedCollectionV1(countersValue, 128, out List<object> counters) ||
                    !TryMapCountersV1(counters, out VisibleCounterStateV1 counterState) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "CardAttachedTo", out object? attachedTo))
                {
                    return false;
                }
                if (attachedTo != null)
                {
                    if (!refs.TryGetValue(attachedTo, out VisibleObjectRefV1? attachedRef))
                    {
                        return false;
                    }
                    relations.Add(new VisibleRelationV1
                    {
                        Object = refs[card],
                        AttachedTo = attachedRef
                    });
                }
                mapped.Add(new VisibleBattlefieldCardV1
                {
                    ObjectRef = refs[card],
                    CardName = name,
                    Tapped = tapped,
                    MarkedDamage = checked((ushort)damage),
                    Counters = counterState,
                    IsToken = token,
                    VisibleEffectivePower = (int?)powerValue,
                    VisibleEffectiveToughness = (int?)toughnessValue
                });
            }
            return true;
        }

        private static bool TryMapCountersV1(
            List<object> counters,
            out VisibleCounterStateV1 state)
        {
            state = new VisibleCounterStateV1();
            foreach (object counter in counters)
            {
                if (!TryReadExactPropertyV1(counter, "DuelScene", "Shiny.Play.Duel.ViewModel.CardCounterViewModel", "Quantity", out object? quantityValue) ||
                    !(quantityValue is int quantity) || quantity < 0 || quantity > short.MaxValue ||
                    !TryReadExactPropertyV1(counter, "DuelScene", "Shiny.Play.Duel.ViewModel.CardCounterViewModel", "Type", out object? typeValue) ||
                    typeValue == null || typeValue.GetType().FullName != "WotC.MtGO.Client.Model.Play.Counter")
                {
                    return false;
                }
                short value = checked((short)quantity);
                switch (Enum.GetName(typeValue.GetType(), typeValue))
                {
                    case "PlusOnePlusOne": state.PlusOnePlusOne = value; break;
                    case "MinusOneMinusOne": state.MinusOneMinusOne = value; break;
                    case "MinusZeroMinusOne": state.MinusZeroMinusOne = value; break;
                    case "Stun": state.Stun = value; break;
                    case "Lore": state.Lore = value; break;
                    default: return false;
                }
            }
            return true;
        }

        private static bool TryMapNamedCardsV1(
            IEnumerable<object> cards,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleNamedCardV1> mapped)
        {
            mapped = new List<VisibleNamedCardV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true))
                {
                    return false;
                }
                mapped.Add(new VisibleNamedCardV1 { ObjectRef = refs[card], CardName = name });
            }
            return true;
        }

        private static bool TryMapExileV1(
            IEnumerable<object> cards,
            string zoneOwner,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleExileCardV1> mapped)
        {
            mapped = new List<VisibleExileCardV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown))
                {
                    return false;
                }
                string? name = null;
                // MTGO can retain the private card name on either player's
                // face-down exiled object. The plotted card is visible, but
                // that retained name is not. Export no name unless the card
                // itself is face up.
                if (!faceDown)
                {
                    if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string visibleName) || !IsBoundedVisibleStringV1(visibleName, 256, true))
                    {
                        return false;
                    }
                    name = visibleName;
                }
                mapped.Add(new VisibleExileCardV1
                {
                    ObjectRef = refs[card],
                    ZoneOwner = zoneOwner,
                    VisibleCardName = name
                });
            }
            return true;
        }

        private static bool TryMapBasicVisibleActionsV1(
            List<object> visibleCards,
            List<object> handCards,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<Dictionary<string, object?>> actions,
            out List<object> boundClientActions)
        {
            // Card actions come only from the visible-source-bound client
            // collection. Priority Pass is joined separately from the one
            // visible and enabled default OK or Done control.
            actions = new List<Dictionary<string, object?>>();
            boundClientActions = new List<object>();
            var hand = new HashSet<object>(handCards, ReferenceIdentityComparerV1.Instance);
            foreach (object card in visibleCards.Distinct(ReferenceIdentityComparerV1.Instance))
            {
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? source) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionValue) ||
                    !TryBoundedCollectionV1(actionValue, 256, out List<object> cardActions))
                {
                    return false;
                }
                foreach (object action in cardActions)
                {
                    if (!TryMapBasicCardActionV1(
                            action,
                            hand.Contains(card),
                            source,
                            actions.Count,
                            out Dictionary<string, object?> mapped))
                    {
                        return false;
                    }
                    actions.Add(mapped);
                    boundClientActions.Add(action);
                    if (actions.Count > 1024)
                    {
                        return false;
                    }
                }
            }
            return actions.Count > 0 && actions.Count < 64 &&
                actions.Select(CanonicalBasicActionKeyV1).Distinct(StringComparer.Ordinal).Count() ==
                    actions.Count;
        }

        private static bool TryMapVisiblePriorityPassControlV1(
            object viewModel,
            out Dictionary<string, object?> pass,
            out object boundClientAction)
        {
            pass = new Dictionary<string, object?>();
            boundClientAction = new object();
            const string promptType = "Shiny.Play.Duel.ViewModel.PromptBoxViewModel";
            if (!TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "PromptBox",
                    out object? promptBox) || promptBox == null ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    promptBox,
                    "DuelScene",
                    promptType,
                    "OkPromptButton",
                    out object? okButton) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    promptBox,
                    "DuelScene",
                    promptType,
                    "DoneButton",
                    out object? doneButton) ||
                !TryReadVisiblePriorityControlV1(okButton, out bool okEligible, out object? okAction) ||
                !TryReadVisiblePriorityControlV1(doneButton, out bool doneEligible, out object? doneAction) ||
                (okEligible ? 1 : 0) + (doneEligible ? 1 : 0) != 1)
            {
                return false;
            }
            pass["action_kind"] = "pass";
            pass["actor"] = "seated_player";
            boundClientAction = okEligible ? okAction! : doneAction!;
            return true;
        }

        private static bool TryReadVisiblePriorityControlV1(
            object? control,
            out bool eligible,
            out object? boundClientAction)
        {
            eligible = false;
            boundClientAction = null;
            if (control == null)
            {
                return true;
            }
            const string buttonType = "Shiny.Play.Duel.ViewModel.OptionButton";
            if (!TryReadExactPropertyV1(
                    control,
                    "DuelScene",
                    buttonType,
                    "Visible",
                    out object? visibleValue) || !(visibleValue is bool visible) ||
                !TryReadExactPropertyV1(
                    control,
                    "DuelScene",
                    buttonType,
                    "Enabled",
                    out object? enabledValue) || !(enabledValue is bool enabled) ||
                !TryReadExactPropertyV1(
                    control,
                    "DuelScene",
                    buttonType,
                    "Name",
                    out object? nameValue) ||
                !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true))
            {
                return false;
            }
            if (!visible || !enabled)
            {
                return true;
            }
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    control,
                    "DuelScene",
                    buttonType,
                    "Action",
                    out object? action) || action == null ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    "WotC.MtGO.Client.Model.Reference",
                    "WotC.MtGO.Client.Model.Play.IGameAction",
                    "IsDefault",
                    out object? defaultValue) || !(defaultValue is bool isDefault) || !isDefault)
            {
                return false;
            }
            eligible = true;
            boundClientAction = action;
            return true;
        }

        private static bool TryMapBasicCardActionV1(
            object action,
            bool sourceInHand,
            VisibleObjectRefV1 source,
            int actionIndex,
            out Dictionary<string, object?> mapped)
        {
            mapped = new Dictionary<string, object?>();
            if (action.GetType().FullName == "Shiny.Play.Duel.GroupCardAction")
            {
                return false;
            }
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string gameAction = "WotC.MtGO.Client.Model.Play.IGameAction";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            if (!TryReadExactPrivateVisibleActionPropertyV1(action, assembly, gameAction, "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 512, true) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, gameAction, "ActionType", out object? actionTypeValue) || actionTypeValue == null || actionTypeValue.GetType().FullName != "WotC.MtGO.Client.Model.Play.ActionType" || Enum.GetName(actionTypeValue.GetType(), actionTypeValue) != "CardAction" ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "CanBePerformedLocally", out object? localValue) || !(localValue is bool local) || !local ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsManaAbility", out object? manaValue) || !(manaValue is bool mana) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsActivatedAbility", out object? activatedValue) || !(activatedValue is bool activated) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsCastAction", out object? castValue) || !(castValue is bool cast) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ModeOptions", out object? modesValue) ||
                (modesValue != null && (!TryBoundedCollectionV1(modesValue, 64, out List<object> modes) || modes.Count != 0)) ||
                !TryRequireBasicVisibleCardActionMenuShapeV1(action))
            {
                return false;
            }
            mapped["actor"] = "seated_player";
            mapped["source"] = source;
            if (mana)
            {
                mapped["action_kind"] = "activate_mana_ability";
                mapped["mana_choice"] = null;
            }
            else if (activated)
            {
                mapped["action_kind"] = "activate_ability";
                mapped["visible_choice_ordinal"] = checked((uint)actionIndex);
            }
            else if (cast)
            {
                mapped["action_kind"] = "cast_spell";
            }
            else if (sourceInHand &&
                (string.Equals(name, "Play", StringComparison.OrdinalIgnoreCase) ||
                 string.Equals(name, "Play Land", StringComparison.OrdinalIgnoreCase)))
            {
                mapped["action_kind"] = "play_land";
            }
            else
            {
                return false;
            }
            return true;
        }

        private static string CanonicalBasicActionKeyV1(
            Dictionary<string, object?> action)
        {
            string source = action.TryGetValue("source", out object? sourceValue) &&
                sourceValue is VisibleObjectRefV1 sourceRef
                ? sourceRef.VisibleOrdinal.ToString(System.Globalization.CultureInfo.InvariantCulture)
                : string.Empty;
            string ordinal = action.TryGetValue("visible_choice_ordinal", out object? ordinalValue)
                ? Convert.ToString(ordinalValue, System.Globalization.CultureInfo.InvariantCulture) ?? string.Empty
                : string.Empty;
            return Convert.ToString(action["action_kind"], System.Globalization.CultureInfo.InvariantCulture) +
                "|" + source + "|" + ordinal;
        }
    }
}
