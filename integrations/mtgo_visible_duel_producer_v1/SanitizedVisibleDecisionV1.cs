using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
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
            public List<VisibleBlockerAssignmentV1> BlockerAssignments { get; set; } =
                new List<VisibleBlockerAssignmentV1>();
        }

        [DataContract]
        private sealed class VisibleBlockerAssignmentV1
        {
            [DataMember(Name = "attacker", Order = 1)]
            public VisibleObjectRefV1 Attacker { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "ordered_blockers", Order = 2)]
            public List<VisibleObjectRefV1> OrderedBlockers { get; set; } =
                new List<VisibleObjectRefV1>();
        }

        [DataContract]
        private sealed class VisibleStackItemV1
        {
            [DataMember(Name = "visible_stack_position", Order = 1)]
            public uint VisibleStackPosition { get; set; }

            [DataMember(Name = "source_object_ref", Order = 2)]
            public VisibleObjectRefV1 SourceObjectRef { get; set; } =
                new VisibleObjectRefV1();

            [DataMember(Name = "visible_source_name", Order = 3)]
            public string VisibleSourceName { get; set; } = string.Empty;

            [DataMember(Name = "controller", Order = 4)]
            public string Controller { get; set; } = "seated_player";

            [DataMember(Name = "visible_targets", Order = 5)]
            public List<object> VisibleTargets { get; set; } = new List<object>();

            [DataMember(Name = "item_kind", Order = 6)]
            public string ItemKind { get; set; } = "spell";
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
            public List<VisibleStackItemV1> Stack { get; set; } =
                new List<VisibleStackItemV1>();

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

        [DataContract]
        private sealed class VisibleAttackerCandidateV1
        {
            [DataMember(Name = "attacker", Order = 1)]
            public VisibleObjectRefV1 Attacker { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "currently_attacking", Order = 2)]
            public bool CurrentlyAttacking { get; set; }

            [DataMember(Name = "attack_opponent_action_visible", Order = 3)]
            public bool AttackOpponentActionVisible { get; set; }

            [DataMember(Name = "dont_attack_action_visible", Order = 4)]
            public bool DontAttackActionVisible { get; set; }
        }

        [DataContract]
        private sealed class VisibleAttackerSelectionV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "ordered_candidates", Order = 2)]
            public List<VisibleAttackerCandidateV1> OrderedCandidates { get; set; } =
                new List<VisibleAttackerCandidateV1>();

            [DataMember(Name = "unique_visible_enabled_done_control", Order = 3)]
            public bool UniqueVisibleEnabledDoneControl { get; set; }
        }

        [DataContract]
        private sealed class VisibleAttackerSelectionResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } = "visible_attacker_selection";

            [DataMember(Name = "selection", Order = 2)]
            public VisibleAttackerSelectionV1 Selection { get; set; } =
                new VisibleAttackerSelectionV1();
        }

        [DataContract]
        private sealed class VisibleSingleAttackerBlockerCandidateV1
        {
            [DataMember(Name = "blocker", Order = 1)]
            public VisibleObjectRefV1 Blocker { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "currently_blocking", Order = 2)]
            public bool CurrentlyBlocking { get; set; }

            [DataMember(Name = "block_action_visible", Order = 3)]
            public bool BlockActionVisible { get; set; }
        }

        [DataContract]
        private sealed class VisibleSingleAttackerBlockerSelectionV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "attacker", Order = 2)]
            public VisibleObjectRefV1 Attacker { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "ordered_candidates", Order = 3)]
            public List<VisibleSingleAttackerBlockerCandidateV1> OrderedCandidates { get; set; } =
                new List<VisibleSingleAttackerBlockerCandidateV1>();

            [DataMember(Name = "unique_visible_enabled_done_control", Order = 4)]
            public bool UniqueVisibleEnabledDoneControl { get; set; }
        }

        [DataContract]
        private sealed class VisibleSingleAttackerBlockerSelectionResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } =
                "visible_single_attacker_blocker_selection";

            [DataMember(Name = "selection", Order = 2)]
            public VisibleSingleAttackerBlockerSelectionV1 Selection { get; set; } =
                new VisibleSingleAttackerBlockerSelectionV1();
        }

        [DataContract]
        private sealed class VisibleSingleAttackerBlockerExecutionStateResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } =
                "visible_single_attacker_blocker_execution_state";

            [DataMember(Name = "selection", Order = 2)]
            public VisibleSingleAttackerBlockerSelectionV1 Selection { get; set; } =
                new VisibleSingleAttackerBlockerSelectionV1();
        }

        [DataContract]
        private sealed class VisibleMultiAttackerBlockerSelectionV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "ordered_available_blockers", Order = 2)]
            public List<VisibleObjectRefV1> OrderedAvailableBlockers { get; set; } =
                new List<VisibleObjectRefV1>();

            [DataMember(Name = "unique_visible_enabled_done_control", Order = 3)]
            public bool UniqueVisibleEnabledDoneControl { get; set; }
        }

        [DataContract]
        private sealed class VisibleMultiAttackerBlockerSelectionResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } =
                "visible_multi_attacker_blocker_selection";

            [DataMember(Name = "selection", Order = 2)]
            public VisibleMultiAttackerBlockerSelectionV1 Selection { get; set; } =
                new VisibleMultiAttackerBlockerSelectionV1();
        }

        [DataContract]
        private sealed class VisibleBlockerTargetSelectionV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "blocker", Order = 2)]
            public VisibleObjectRefV1 Blocker { get; set; } = new VisibleObjectRefV1();

            [DataMember(Name = "ordered_visible_targetable_attackers", Order = 3)]
            public List<VisibleObjectRefV1> OrderedVisibleTargetableAttackers { get; set; } =
                new List<VisibleObjectRefV1>();
        }

        [DataContract]
        private sealed class VisibleBlockerTargetSelectionResultV1
        {
            [DataMember(Name = "result_kind", Order = 1)]
            public string ResultKind { get; set; } = "visible_blocker_target_selection";

            [DataMember(Name = "selection", Order = 2)]
            public VisibleBlockerTargetSelectionV1 Selection { get; set; } =
                new VisibleBlockerTargetSelectionV1();
        }

        [DataContract]
        private sealed class VisibleAttackerExecutionUniverseV1
        {
            [DataMember(Name = "current_state", Order = 1)]
            public VisibleStateV1 CurrentState { get; set; } = new VisibleStateV1();

            [DataMember(Name = "ordered_candidate_attackers", Order = 2)]
            public List<VisibleObjectRefV1> OrderedCandidateAttackers { get; set; } =
                new List<VisibleObjectRefV1>();
        }

        private sealed class PlayerSnapshotV1
        {
            internal object Player = new object();
            internal string VisibleName = string.Empty;
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

        private static bool TryBuildSanitizedVisibleDecisionV1(
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
                if (TryBuildSanitizedVisibleBlockerTargetSelectionV1(
                        viewModel,
                        out result))
                {
                    // The target modal is a separately typed visible model
                    // decision. No client target or action binding leaves it.
                    return true;
                }
                if (TryBuildSanitizedVisibleAttackerSelectionV1(viewModel, out result))
                {
                    // The legacy selected-index dispatcher cannot consume a
                    // multi-step combat plan, so no client bindings leave
                    // this branch.
                    return true;
                }
                if (TryBuildSanitizedVisibleSingleAttackerBlockerSelectionV1(
                        viewModel,
                        out result))
                {
                    // Client action bindings never leave this branch. The
                    // separate sealed plan dispatcher must rebuild it.
                    return true;
                }
                if (TryBuildSanitizedVisibleSingleAttackerBlockerExecutionStateV1(
                        viewModel,
                        out result))
                {
                    // This is a fresh complete rendered postcondition for a
                    // monotonic single-attacker blocker plan. It carries no
                    // client action or target binding.
                    return true;
                }
                if (TryBuildSanitizedVisibleMultiAttackerBlockerSelectionV1(
                        viewModel,
                        out result))
                {
                    // Multi-attacker blocking is staged through a later
                    // visible target modal. This observation carries no
                    // executable action binding.
                    return true;
                }
                if (!TryBuildSanitizedVisibleDecisionCoreV1(
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

        internal static bool TryBuildSanitizedVisibleAttackerSelectionV1(
            object viewModel,
            out byte[] result)
        {
            return TryBuildSanitizedVisibleAttackerSelectionAndBindingsV1(
                viewModel,
                out result,
                out _,
                out _,
                out _,
                out _);
        }

        internal static bool TryBuildSanitizedVisibleSingleAttackerBlockerSelectionV1(
            object viewModel,
            out byte[] result)
        {
            result = Array.Empty<byte>();
            try
            {
                if (!TryBuildSanitizedVisibleSingleAttackerBlockerSelectionCoreV1(
                        viewModel,
                        out VisibleSingleAttackerBlockerSelectionResultV1 payload))
                {
                    return false;
                }
                var settings = new DataContractJsonSerializerSettings
                {
                    UseSimpleDictionaryFormat = true,
                    KnownTypes = new[] { typeof(VisibleObjectRefV1) },
                    EmitTypeInformation = EmitTypeInformation.Never
                };
                var serializer = new DataContractJsonSerializer(
                    typeof(VisibleSingleAttackerBlockerSelectionResultV1),
                    settings);
                using (var stream = new MemoryStream())
                {
                    serializer.WriteObject(stream, payload);
                    if (stream.Length <= 0 ||
                        stream.Length > MaximumOutputBytes - OutputPayloadOffset)
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
                return false;
            }
        }

        internal static bool TryBuildSanitizedVisibleSingleAttackerBlockerStateAndBindingsV1(
            object viewModel,
            out byte[] result,
            out List<SealedVisibleSingleBlockerBindingV1> bindings,
            out List<SealedVisibleBlockerAssignmentV1> visibleAssignments,
            out object? doneAction,
            out uint turn,
            out string visibleUniverseSha256)
        {
            result = Array.Empty<byte>();
            bindings = new List<SealedVisibleSingleBlockerBindingV1>();
            visibleAssignments = new List<SealedVisibleBlockerAssignmentV1>();
            doneAction = null;
            turn = 0;
            visibleUniverseSha256 = string.Empty;
            try
            {
                if (TryBuildSanitizedVisibleSingleAttackerBlockerSelectionCoreV1(
                        viewModel,
                        out VisibleSingleAttackerBlockerSelectionResultV1 initial,
                        out List<SealedVisibleBlockerBindingV1> initialBindings,
                        out doneAction) &&
                    doneAction != null &&
                    TryComputeVisibleBlockerUniverseSha256V1(
                        initial.Selection.CurrentState,
                        out visibleUniverseSha256) &&
                    TrySerializeSanitizedVisibleCombatPayloadV1(initial, out result))
                {
                    bindings = initialBindings.Select(binding =>
                        new SealedVisibleSingleBlockerBindingV1
                        {
                            BlockerCard = binding.BlockerCard,
                            BlockerVisibleOrdinal = binding.BlockerVisibleOrdinal,
                            CurrentlyBlocking = false,
                            BlockAction = binding.BlockAction
                        }).ToList();
                    turn = initial.Selection.CurrentState.Turn;
                    return true;
                }

                if (!TryBuildVisibleDeclareBlockersSnapshotV1(
                        viewModel,
                        false,
                        out VisibleStateV1 state,
                        out List<object> seatedBattlefield,
                        out List<object> opponentBattlefield,
                        out Dictionary<object, VisibleObjectRefV1> objectRefs,
                        out List<VisibleObjectRefV1> visibleAttackers) ||
                    visibleAttackers.Count != 1 ||
                    !TryMapVisibleBlockerAssignmentsV1(
                        seatedBattlefield,
                        opponentBattlefield,
                        objectRefs,
                        visibleAttackers,
                        out List<VisibleBlockerAssignmentV1> assignments,
                        out visibleAssignments) ||
                    assignments.Count != 1 ||
                    !TryMapVisibleSingleAttackerBlockerExecutionCandidatesV1(
                        seatedBattlefield,
                        objectRefs,
                        visibleAssignments,
                        out List<VisibleSingleAttackerBlockerCandidateV1> candidates,
                        out bindings) ||
                    !TryRequireVisibleAttackerDoneControlV1(viewModel, out doneAction) ||
                    doneAction == null)
                {
                    return false;
                }
                state.Combat.AttackersDeclared = true;
                state.Combat.OrderedAttackers = visibleAttackers;
                state.Combat.BlockerAssignments = assignments;
                var current = new VisibleSingleAttackerBlockerExecutionStateResultV1
                {
                    Selection = new VisibleSingleAttackerBlockerSelectionV1
                    {
                        CurrentState = state,
                        Attacker = visibleAttackers.Single(),
                        OrderedCandidates = candidates,
                        UniqueVisibleEnabledDoneControl = true
                    }
                };
                if (!TryComputeVisibleBlockerUniverseSha256V1(
                        state,
                        out visibleUniverseSha256) ||
                    !TrySerializeSanitizedVisibleCombatPayloadV1(current, out result))
                {
                    return false;
                }
                turn = state.Turn;
                return true;
            }
            catch
            {
                result = Array.Empty<byte>();
                bindings = new List<SealedVisibleSingleBlockerBindingV1>();
                visibleAssignments = new List<SealedVisibleBlockerAssignmentV1>();
                doneAction = null;
                turn = 0;
                visibleUniverseSha256 = string.Empty;
                return false;
            }
        }

        internal static bool TryBuildSanitizedVisibleSingleAttackerBlockerExecutionStateV1(
            object viewModel,
            out byte[] result)
        {
            result = Array.Empty<byte>();
            if (!TryBuildSanitizedVisibleSingleAttackerBlockerStateAndBindingsV1(
                    viewModel,
                    out byte[] current,
                    out _,
                    out List<SealedVisibleBlockerAssignmentV1> assignments,
                    out _,
                    out _,
                    out _) ||
                assignments.Count != 1 || current.Length == 0 ||
                !IsSanitizedVisibleSingleAttackerBlockerExecutionStateResultV1(current))
            {
                return false;
            }
            result = current;
            return true;
        }

        internal static bool TryBuildSanitizedVisibleMultiAttackerBlockerSelectionV1(
            object viewModel,
            out byte[] result)
        {
            return TrySerializeSanitizedVisibleCombatSelectionV1<
                VisibleMultiAttackerBlockerSelectionResultV1>(
                viewModel,
                TryBuildSanitizedVisibleMultiAttackerBlockerSelectionCoreV1,
                out result);
        }

        internal static bool TryBuildSanitizedVisibleMultiAttackerBlockerSelectionAndBindingsV1(
            object viewModel,
            out byte[] result,
            out List<SealedVisibleBlockerBindingV1> bindings,
            out List<SealedVisibleBlockerAssignmentV1> visibleAssignments,
            out object? doneAction,
            out uint turn,
            out string visibleUniverseSha256)
        {
            result = Array.Empty<byte>();
            bindings = new List<SealedVisibleBlockerBindingV1>();
            visibleAssignments = new List<SealedVisibleBlockerAssignmentV1>();
            doneAction = null;
            turn = 0;
            visibleUniverseSha256 = string.Empty;
            try
            {
                if (!TryBuildSanitizedVisibleMultiAttackerBlockerSelectionCoreV1(
                        viewModel,
                        out VisibleMultiAttackerBlockerSelectionResultV1 payload,
                        out bindings,
                        out visibleAssignments,
                        out doneAction) ||
                    doneAction == null ||
                    !TryComputeVisibleBlockerUniverseSha256V1(
                        payload.Selection.CurrentState,
                        out visibleUniverseSha256) ||
                    !TrySerializeSanitizedVisibleCombatPayloadV1(payload, out result))
                {
                    return false;
                }
                turn = payload.Selection.CurrentState.Turn;
                return true;
            }
            catch
            {
                result = Array.Empty<byte>();
                bindings = new List<SealedVisibleBlockerBindingV1>();
                visibleAssignments = new List<SealedVisibleBlockerAssignmentV1>();
                doneAction = null;
                turn = 0;
                visibleUniverseSha256 = string.Empty;
                return false;
            }
        }

        internal static bool TryBuildSanitizedVisibleBlockerTargetSelectionV1(
            object viewModel,
            out byte[] result)
        {
            return TrySerializeSanitizedVisibleCombatSelectionV1<
                VisibleBlockerTargetSelectionResultV1>(
                viewModel,
                TryBuildSanitizedVisibleBlockerTargetSelectionCoreV1,
                out result);
        }

        internal static bool TryBuildSanitizedVisibleBlockerTargetSelectionAndBindingsV1(
            object viewModel,
            out byte[] result,
            out object? blockerCard,
            out uint blockerVisibleOrdinal,
            out List<SealedVisibleBlockerTargetBindingV1> targetBindings,
            out uint turn,
            out string visibleUniverseSha256)
        {
            result = Array.Empty<byte>();
            blockerCard = null;
            blockerVisibleOrdinal = 0;
            targetBindings = new List<SealedVisibleBlockerTargetBindingV1>();
            turn = 0;
            visibleUniverseSha256 = string.Empty;
            try
            {
                if (!TryBuildSanitizedVisibleBlockerTargetSelectionCoreV1(
                        viewModel,
                        out VisibleBlockerTargetSelectionResultV1 payload,
                        out blockerCard,
                        out targetBindings) ||
                    blockerCard == null ||
                    !TryComputeVisibleBlockerUniverseSha256V1(
                        payload.Selection.CurrentState,
                        out visibleUniverseSha256) ||
                    !TrySerializeSanitizedVisibleCombatPayloadV1(payload, out result))
                {
                    return false;
                }
                blockerVisibleOrdinal = payload.Selection.Blocker.VisibleOrdinal;
                turn = payload.Selection.CurrentState.Turn;
                return true;
            }
            catch
            {
                result = Array.Empty<byte>();
                blockerCard = null;
                blockerVisibleOrdinal = 0;
                targetBindings = new List<SealedVisibleBlockerTargetBindingV1>();
                turn = 0;
                visibleUniverseSha256 = string.Empty;
                return false;
            }
        }

        private delegate bool TryBuildVisibleCombatPayloadV1<TPayload>(
            object viewModel,
            out TPayload payload);

        private static bool TrySerializeSanitizedVisibleCombatSelectionV1<TPayload>(
            object viewModel,
            TryBuildVisibleCombatPayloadV1<TPayload> builder,
            out byte[] result)
        {
            result = Array.Empty<byte>();
            try
            {
                if (!builder(viewModel, out TPayload payload))
                {
                    return false;
                }
                return TrySerializeSanitizedVisibleCombatPayloadV1(payload, out result);
            }
            catch
            {
                result = Array.Empty<byte>();
                return false;
            }
        }

        private static bool TrySerializeSanitizedVisibleCombatPayloadV1<TPayload>(
            TPayload payload,
            out byte[] result)
        {
            result = Array.Empty<byte>();
            try
            {
                var serializer = new DataContractJsonSerializer(
                    typeof(TPayload),
                    new DataContractJsonSerializerSettings
                    {
                        UseSimpleDictionaryFormat = true,
                        KnownTypes = new[]
                        {
                            typeof(VisibleObjectRefV1),
                            typeof(Dictionary<string, object?>)
                        },
                        EmitTypeInformation = EmitTypeInformation.Never
                    });
                using (var stream = new MemoryStream())
                {
                    serializer.WriteObject(stream, payload);
                    if (stream.Length <= 0 ||
                        stream.Length > MaximumOutputBytes - OutputPayloadOffset)
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
                return false;
            }
        }

        internal static bool TryBuildSanitizedVisibleAttackerSelectionAndBindingsV1(
            object viewModel,
            out byte[] result,
            out List<SealedVisibleAttackerBindingV1> bindings,
            out object? doneAction,
            out uint turn,
            out string visibleUniverseSha256)
        {
            result = Array.Empty<byte>();
            bindings = new List<SealedVisibleAttackerBindingV1>();
            doneAction = null;
            turn = 0;
            visibleUniverseSha256 = string.Empty;
            try
            {
                if (!TryBuildSanitizedVisibleAttackerSelectionCoreV1(
                        viewModel,
                        out VisibleAttackerSelectionResultV1 payload,
                        out bindings,
                        out doneAction,
                        out turn))
                {
                    return false;
                }
                if (!TryComputeVisibleAttackerUniverseSha256V1(
                        payload.Selection,
                        out visibleUniverseSha256))
                {
                    return false;
                }
                var settings = new DataContractJsonSerializerSettings
                {
                    UseSimpleDictionaryFormat = true,
                    KnownTypes = new[] { typeof(VisibleObjectRefV1) },
                    EmitTypeInformation = EmitTypeInformation.Never
                };
                var serializer = new DataContractJsonSerializer(
                    typeof(VisibleAttackerSelectionResultV1),
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
                bindings = new List<SealedVisibleAttackerBindingV1>();
                doneAction = null;
                turn = 0;
                visibleUniverseSha256 = string.Empty;
                return false;
            }
        }

        private static bool TryComputeVisibleAttackerUniverseSha256V1(
            VisibleAttackerSelectionV1 selection,
            out string visibleUniverseSha256)
        {
            visibleUniverseSha256 = string.Empty;
            VisibleCombatStateV1 originalCombat = selection.CurrentState.Combat;
            try
            {
                selection.CurrentState.Combat = new VisibleCombatStateV1();
                var universe = new VisibleAttackerExecutionUniverseV1
                {
                    CurrentState = selection.CurrentState,
                    OrderedCandidateAttackers = selection.OrderedCandidates
                        .Select(candidate => candidate.Attacker)
                        .ToList()
                };
                var serializer = new DataContractJsonSerializer(
                    typeof(VisibleAttackerExecutionUniverseV1),
                    new DataContractJsonSerializerSettings
                    {
                        UseSimpleDictionaryFormat = true,
                        KnownTypes = new[] { typeof(VisibleObjectRefV1) },
                        EmitTypeInformation = EmitTypeInformation.Never
                    });
                using (var stream = new MemoryStream())
                {
                    serializer.WriteObject(stream, universe);
                    visibleUniverseSha256 = LowerSha256V1(stream.ToArray());
                }
                return IsLowerSha256V1(visibleUniverseSha256);
            }
            finally
            {
                selection.CurrentState.Combat = originalCombat;
            }
        }

        private static bool TryComputeVisibleBlockerUniverseSha256V1(
            VisibleStateV1 state,
            out string visibleUniverseSha256)
        {
            visibleUniverseSha256 = string.Empty;
            List<VisibleBlockerAssignmentV1> originalAssignments =
                state.Combat.BlockerAssignments;
            try
            {
                state.Combat.BlockerAssignments = new List<VisibleBlockerAssignmentV1>();
                var serializer = new DataContractJsonSerializer(
                    typeof(VisibleStateV1),
                    new DataContractJsonSerializerSettings
                    {
                        UseSimpleDictionaryFormat = true,
                        KnownTypes = new[] { typeof(VisibleObjectRefV1) },
                        EmitTypeInformation = EmitTypeInformation.Never
                    });
                using (var stream = new MemoryStream())
                {
                    serializer.WriteObject(stream, state);
                    visibleUniverseSha256 = LowerSha256V1(stream.ToArray());
                }
                return IsLowerSha256V1(visibleUniverseSha256);
            }
            finally
            {
                state.Combat.BlockerAssignments = originalAssignments;
            }
        }

        private static bool TryBuildSanitizedVisibleDecisionCoreV1(
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
                !TryParseVisibleTurnV1(turnTextValue, out uint turn) ||
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
            // This emitted slice deliberately excludes temporary revealed
            // zones. Until every revealed-zone presentation can be assigned to
            // the exact public state field, omitting one would be incomplete.
            // V1.19 admits ordinary noncombat priority decisions during
            // upkeep, draw, either main phase, and the end step with an empty
            // stack and no visible modal. Life, mana, hand and library counts,
            // battlefield, graveyard, and exile are mapped from their rendered
            // presentation values. One narrow seated-player, target-free,
            // non-copy spell stack slice is represented. Combat, other stack
            // items, revealed windows, and Initiative still abstain.
            if (!seated.Priority ||
                !IsSupportedNoncombatPriorityPhaseV1(phase) ||
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
                !(stackCountValue is int stackCount) || stackCount < 0 ||
                stackCount > MaximumVisibleCollectionItems ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Cards",
                    out object? stackCardsValue) ||
                !TryBoundedCollectionV1(
                    stackCardsValue,
                    MaximumVisibleCollectionItems,
                    out List<object> stackCards) ||
                stackCards.Count != stackCount)
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
            foreach (object card in stackCards) register(card);
            foreach (object card in seated.Hand) register(card);
            foreach (object card in opponent.Hand) register(card);

            if (!TryMapBattlefieldV1(seated.Battlefield, objectRefs, out List<VisibleBattlefieldCardV1> seatedBattlefield, out List<VisibleRelationV1> relations) ||
                !TryMapBattlefieldV1(opponent.Battlefield, objectRefs, out List<VisibleBattlefieldCardV1> opponentBattlefield, out List<VisibleRelationV1> opponentRelations) ||
                !TryMapNamedCardsV1(seated.Graveyard, objectRefs, out List<VisibleNamedCardV1> seatedGraveyard) ||
                !TryMapNamedCardsV1(opponent.Graveyard, objectRefs, out List<VisibleNamedCardV1> opponentGraveyard) ||
                !TryMapExileV1(seated.Exile, "seated_player", objectRefs, out List<VisibleExileCardV1> seatedExile) ||
                !TryMapExileV1(opponent.Exile, "opponent", objectRefs, out List<VisibleExileCardV1> opponentExile) ||
                !TryMapNarrowVisibleStackV1(stackCards, objectRefs, out List<VisibleStackItemV1> visibleStack) ||
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
                Stack = visibleStack,
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

        private static bool TryBuildSanitizedVisibleAttackerSelectionCoreV1(
            object viewModel,
            out VisibleAttackerSelectionResultV1 result,
            out List<SealedVisibleAttackerBindingV1> bindings,
            out object? doneAction,
            out uint observedTurn)
        {
            result = new VisibleAttackerSelectionResultV1();
            bindings = new List<SealedVisibleAttackerBindingV1>();
            doneAction = null;
            observedTurn = 0;
            if (!TryRequireSupportedDuelVariantV1(viewModel) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CurrentPhase",
                    out object? phaseValue) ||
                !TryMapVisiblePhaseV1(phaseValue, out string phase) ||
                !string.Equals(phase, "declare_attackers", StringComparison.Ordinal) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "GameTurnText",
                    out object? turnTextValue) ||
                !TryParseVisibleTurnV1(turnTextValue, out uint turn) ||
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
            if (!seated.Active || !seated.Priority ||
                seated.Revealed.Count != 0 || opponent.Revealed.Count != 0 ||
                (opponent.Hand.Count != 0 && opponent.Hand.Count != opponent.HandCount) ||
                !TryMapVisibleInitiativeHolderV1(
                    seated,
                    opponent,
                    out string? visibleInitiative) ||
                visibleInitiative != null ||
                !TryRequireNoUnrepresentedVisibleModalSurfaceV1(viewModel) ||
                !TryRequireVisibleAttackerDoneControlV1(viewModel, out doneAction))
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
                !(stackCountValue is int stackCount) || stackCount != 0 ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Cards",
                    out object? stackCardsValue) ||
                !TryBoundedCollectionV1(stackCardsValue, 0, out List<object> stackCards) ||
                stackCards.Count != 0)
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

            if (!TryMapBattlefieldForAttackerSelectionV1(
                    seated.Battlefield,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> seatedBattlefield,
                    out List<VisibleRelationV1> relations,
                    out List<VisibleObjectRefV1> currentlyAttacking) ||
                !TryMapBattlefieldV1(
                    opponent.Battlefield,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> opponentBattlefield,
                    out List<VisibleRelationV1> opponentRelations) ||
                !TryMapNamedCardsV1(
                    seated.Graveyard,
                    objectRefs,
                    out List<VisibleNamedCardV1> seatedGraveyard) ||
                !TryMapNamedCardsV1(
                    opponent.Graveyard,
                    objectRefs,
                    out List<VisibleNamedCardV1> opponentGraveyard) ||
                !TryMapExileV1(
                    seated.Exile,
                    "seated_player",
                    objectRefs,
                    out List<VisibleExileCardV1> seatedExile) ||
                !TryMapExileV1(
                    opponent.Exile,
                    "opponent",
                    objectRefs,
                    out List<VisibleExileCardV1> opponentExile) ||
                !TryMapNamedCardsV1(
                    seated.Hand,
                    objectRefs,
                    out List<VisibleNamedCardV1> ownHand) ||
                !TryMapNamedCardsV1(
                    opponent.Hand,
                    objectRefs,
                    out List<VisibleNamedCardV1> opponentKnownHand) ||
                !TryMapVisibleAttackerCandidatesV1(
                    seated.Battlefield,
                    opponent.VisibleName,
                    objectRefs,
                    out List<VisibleAttackerCandidateV1> candidates,
                    out bindings))
            {
                return false;
            }
            relations.AddRange(opponentRelations);
            seatedExile.AddRange(opponentExile);
            if (!currentlyAttacking.SequenceEqual(
                    candidates.Where(candidate => candidate.CurrentlyAttacking)
                        .Select(candidate => candidate.Attacker)))
            {
                return false;
            }

            var state = new VisibleStateV1
            {
                Turn = turn,
                Phase = phase,
                ActivePlayer = "seated_player",
                PriorityPlayer = "seated_player",
                Initiative = null,
                LifeTotals = new[] { seated.Life, opponent.Life },
                ManaPools = new[] { seated.ManaPool, opponent.ManaPool },
                HandCounts = new[] { seated.HandCount, opponent.HandCount },
                LibraryCounts = new[] { seated.LibraryCount, opponent.LibraryCount },
                Battlefield = new[] { seatedBattlefield, opponentBattlefield },
                Graveyards = new[] { seatedGraveyard, opponentGraveyard },
                Exile = seatedExile,
                Stack = new List<VisibleStackItemV1>(),
                Combat = new VisibleCombatStateV1
                {
                    OrderedAttackers = currentlyAttacking
                },
                VisibleObjectRelations = relations,
                OwnHand = ownHand,
                KnownLibraryCards = new[] { new List<object>(), new List<object>() },
                KnownHandCards = new[]
                {
                    new List<VisibleNamedCardV1>(),
                    opponentKnownHand
                }
            };
            result.Selection = new VisibleAttackerSelectionV1
            {
                CurrentState = state,
                OrderedCandidates = candidates,
                UniqueVisibleEnabledDoneControl = true
            };
            observedTurn = turn;
            return true;
        }

        private static bool TryBuildSanitizedVisibleSingleAttackerBlockerSelectionCoreV1(
            object viewModel,
            out VisibleSingleAttackerBlockerSelectionResultV1 result)
        {
            return TryBuildSanitizedVisibleSingleAttackerBlockerSelectionCoreV1(
                viewModel,
                out result,
                out _,
                out _);
        }

        private static bool TryBuildSanitizedVisibleSingleAttackerBlockerSelectionCoreV1(
            object viewModel,
            out VisibleSingleAttackerBlockerSelectionResultV1 result,
            out List<SealedVisibleBlockerBindingV1> bindings,
            out object? exactDoneAction)
        {
            result = new VisibleSingleAttackerBlockerSelectionResultV1();
            bindings = new List<SealedVisibleBlockerBindingV1>();
            exactDoneAction = null;
            if (!TryRequireSupportedDuelVariantV1(viewModel) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CurrentPhase",
                    out object? phaseValue) ||
                !TryMapVisiblePhaseV1(phaseValue, out string phase) ||
                !string.Equals(phase, "declare_blockers", StringComparison.Ordinal) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "GameTurnText",
                    out object? turnTextValue) ||
                !TryParseVisibleTurnV1(turnTextValue, out uint turn) ||
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
            if (seated.Active || !opponent.Active || !seated.Priority ||
                seated.Revealed.Count != 0 || opponent.Revealed.Count != 0 ||
                (opponent.Hand.Count != 0 && opponent.Hand.Count != opponent.HandCount) ||
                !TryMapVisibleInitiativeHolderV1(
                    seated,
                    opponent,
                    out string? visibleInitiative) ||
                visibleInitiative != null ||
                !TryRequireNoUnrepresentedVisibleModalSurfaceV1(viewModel) ||
                !TryRequireVisibleAttackerDoneControlV1(viewModel, out object? doneAction) ||
                doneAction == null ||
                !TryReadExactPropertyV1(
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
                !(stackCountValue is int stackCount) || stackCount != 0 ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Cards",
                    out object? stackCardsValue) ||
                !TryBoundedCollectionV1(stackCardsValue, 0, out List<object> stackCards) ||
                stackCards.Count != 0)
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

            if (!TryMapBattlefieldForSingleAttackerBlockerSelectionV1(
                    seated.Battlefield,
                    false,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> seatedBattlefield,
                    out List<VisibleRelationV1> relations,
                    out List<VisibleObjectRefV1> visibleAttackers) ||
                visibleAttackers.Count != 0 ||
                !TryMapBattlefieldForSingleAttackerBlockerSelectionV1(
                    opponent.Battlefield,
                    true,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> opponentBattlefield,
                    out List<VisibleRelationV1> opponentRelations,
                    out visibleAttackers) ||
                visibleAttackers.Count != 1 ||
                !TryMapNamedCardsV1(
                    seated.Graveyard,
                    objectRefs,
                    out List<VisibleNamedCardV1> seatedGraveyard) ||
                !TryMapNamedCardsV1(
                    opponent.Graveyard,
                    objectRefs,
                    out List<VisibleNamedCardV1> opponentGraveyard) ||
                !TryMapExileV1(
                    seated.Exile,
                    "seated_player",
                    objectRefs,
                    out List<VisibleExileCardV1> seatedExile) ||
                !TryMapExileV1(
                    opponent.Exile,
                    "opponent",
                    objectRefs,
                    out List<VisibleExileCardV1> opponentExile) ||
                !TryMapNamedCardsV1(
                    seated.Hand,
                    objectRefs,
                    out List<VisibleNamedCardV1> ownHand) ||
                !TryMapNamedCardsV1(
                    opponent.Hand,
                    objectRefs,
                    out List<VisibleNamedCardV1> opponentKnownHand) ||
                !TryMapVisibleSingleAttackerBlockerCandidatesV1(
                    seated.Battlefield,
                    objectRefs,
                    out List<VisibleSingleAttackerBlockerCandidateV1> candidates,
                    out bindings))
            {
                return false;
            }
            relations.AddRange(opponentRelations);
            seatedExile.AddRange(opponentExile);
            VisibleObjectRefV1 attacker = visibleAttackers.Single();
            var state = new VisibleStateV1
            {
                Turn = turn,
                Phase = phase,
                ActivePlayer = "opponent",
                PriorityPlayer = "seated_player",
                Initiative = null,
                LifeTotals = new[] { seated.Life, opponent.Life },
                ManaPools = new[] { seated.ManaPool, opponent.ManaPool },
                HandCounts = new[] { seated.HandCount, opponent.HandCount },
                LibraryCounts = new[] { seated.LibraryCount, opponent.LibraryCount },
                Battlefield = new[] { seatedBattlefield, opponentBattlefield },
                Graveyards = new[] { seatedGraveyard, opponentGraveyard },
                Exile = seatedExile,
                Stack = new List<VisibleStackItemV1>(),
                Combat = new VisibleCombatStateV1
                {
                    AttackersDeclared = true,
                    OrderedAttackers = new List<VisibleObjectRefV1> { attacker }
                },
                VisibleObjectRelations = relations,
                OwnHand = ownHand,
                KnownLibraryCards = new[] { new List<object>(), new List<object>() },
                KnownHandCards = new[]
                {
                    new List<VisibleNamedCardV1>(),
                    opponentKnownHand
                }
            };
            result.Selection = new VisibleSingleAttackerBlockerSelectionV1
            {
                CurrentState = state,
                Attacker = attacker,
                OrderedCandidates = candidates,
                UniqueVisibleEnabledDoneControl = true
            };
            exactDoneAction = doneAction;
            return true;
        }

        private static bool TryBuildSanitizedVisibleMultiAttackerBlockerSelectionCoreV1(
            object viewModel,
            out VisibleMultiAttackerBlockerSelectionResultV1 result)
        {
            return TryBuildSanitizedVisibleMultiAttackerBlockerSelectionCoreV1(
                viewModel,
                out result,
                out _,
                out _,
                out _);
        }

        private static bool TryBuildSanitizedVisibleMultiAttackerBlockerSelectionCoreV1(
            object viewModel,
            out VisibleMultiAttackerBlockerSelectionResultV1 result,
            out List<SealedVisibleBlockerBindingV1> bindings,
            out List<SealedVisibleBlockerAssignmentV1> visibleAssignments,
            out object? doneAction)
        {
            result = new VisibleMultiAttackerBlockerSelectionResultV1();
            bindings = new List<SealedVisibleBlockerBindingV1>();
            visibleAssignments = new List<SealedVisibleBlockerAssignmentV1>();
            doneAction = null;
            if (!TryBuildVisibleDeclareBlockersSnapshotV1(
                    viewModel,
                    false,
                    out VisibleStateV1 state,
                    out List<object> seatedBattlefield,
                    out List<object> opponentBattlefield,
                    out Dictionary<object, VisibleObjectRefV1> objectRefs,
                    out List<VisibleObjectRefV1> visibleAttackers) ||
                visibleAttackers.Count < 2 ||
                !TryMapVisibleBlockerAssignmentsV1(
                    seatedBattlefield,
                    opponentBattlefield,
                    objectRefs,
                    visibleAttackers,
                    out List<VisibleBlockerAssignmentV1> assignments,
                    out visibleAssignments) ||
                !TryMapVisibleMultiAttackerBlockerCandidatesV1(
                    seatedBattlefield,
                    objectRefs,
                    out List<VisibleObjectRefV1> availableBlockers,
                    out bindings) ||
                !TryRequireVisibleAttackerDoneControlV1(viewModel, out doneAction) ||
                doneAction == null)
            {
                return false;
            }
            state.Combat.AttackersDeclared = true;
            state.Combat.OrderedAttackers = visibleAttackers;
            state.Combat.BlockerAssignments = assignments;
            result.Selection = new VisibleMultiAttackerBlockerSelectionV1
            {
                CurrentState = state,
                OrderedAvailableBlockers = availableBlockers,
                UniqueVisibleEnabledDoneControl = true
            };
            return true;
        }

        private static bool TryBuildSanitizedVisibleBlockerTargetSelectionCoreV1(
            object viewModel,
            out VisibleBlockerTargetSelectionResultV1 result)
        {
            return TryBuildSanitizedVisibleBlockerTargetSelectionCoreV1(
                viewModel,
                out result,
                out _,
                out _);
        }

        private static bool TryBuildSanitizedVisibleBlockerTargetSelectionCoreV1(
            object viewModel,
            out VisibleBlockerTargetSelectionResultV1 result,
            out object? blockerCard,
            out List<SealedVisibleBlockerTargetBindingV1> targetBindings)
        {
            result = new VisibleBlockerTargetSelectionResultV1();
            blockerCard = null;
            targetBindings = new List<SealedVisibleBlockerTargetBindingV1>();
            if (!TryBuildVisibleDeclareBlockersSnapshotV1(
                    viewModel,
                    true,
                    out VisibleStateV1 state,
                    out List<object> seatedBattlefield,
                    out List<object> opponentBattlefield,
                    out Dictionary<object, VisibleObjectRefV1> objectRefs,
                    out List<VisibleObjectRefV1> visibleAttackers) ||
                visibleAttackers.Count < 2 ||
                !TryMapVisibleBlockerAssignmentsV1(
                    seatedBattlefield,
                    opponentBattlefield,
                    objectRefs,
                    visibleAttackers,
                    out List<VisibleBlockerAssignmentV1> assignments,
                    out _))
            {
                return false;
            }

            var targetingBlockers = new List<object>();
            foreach (object card in seatedBattlefield)
            {
                if (!TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "IsTargeting",
                        out object? targetingValue) ||
                    !(targetingValue is bool targeting))
                {
                    return false;
                }
                if (targeting)
                {
                    targetingBlockers.Add(card);
                }
            }
            if (targetingBlockers.Count != 1 ||
                !TryRequireVisibleTargetSelectionModeV1(viewModel) ||
                !objectRefs.TryGetValue(
                    targetingBlockers.Single(),
                    out VisibleObjectRefV1? blocker))
            {
                return false;
            }
            blockerCard = targetingBlockers.Single();

            var targetableAttackers = new List<VisibleObjectRefV1>();
            for (int index = 0; index < opponentBattlefield.Count; index++)
            {
                object card = opponentBattlefield[index];
                if (!TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyAttacking",
                        out object? attackingValue) ||
                    !(attackingValue is bool attacking) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "IsTargetable",
                        out object? targetableValue) ||
                    !(targetableValue is bool targetable) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "IsTargeting",
                        out object? targetingValue) ||
                    !(targetingValue is bool targeting) || targeting ||
                    !objectRefs.TryGetValue(card, out VisibleObjectRefV1? attacker))
                {
                    return false;
                }
                if (targetable && !attacking)
                {
                    return false;
                }
                if (attacking && targetable)
                {
                    targetableAttackers.Add(attacker);
                    targetBindings.Add(new SealedVisibleBlockerTargetBindingV1
                    {
                        AttackerCard = card,
                        AttackerVisibleOrdinal = attacker.VisibleOrdinal
                    });
                }
            }
            foreach (object card in seatedBattlefield)
            {
                if (!TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "IsTargetable",
                        out object? targetableValue) ||
                    !(targetableValue is bool targetable) || targetable)
                {
                    return false;
                }
            }
            if (targetableAttackers.Count == 0)
            {
                return false;
            }

            state.Combat.AttackersDeclared = true;
            state.Combat.OrderedAttackers = visibleAttackers;
            state.Combat.BlockerAssignments = assignments;
            result.Selection = new VisibleBlockerTargetSelectionV1
            {
                CurrentState = state,
                Blocker = blocker,
                OrderedVisibleTargetableAttackers = targetableAttackers
            };
            return true;
        }

        private static bool TryBuildVisibleDeclareBlockersSnapshotV1(
            object viewModel,
            bool targetModalExpected,
            out VisibleStateV1 state,
            out List<object> seatedBattlefieldCards,
            out List<object> opponentBattlefieldCards,
            out Dictionary<object, VisibleObjectRefV1> objectRefs,
            out List<VisibleObjectRefV1> visibleAttackers)
        {
            state = new VisibleStateV1();
            seatedBattlefieldCards = new List<object>();
            opponentBattlefieldCards = new List<object>();
            objectRefs = new Dictionary<object, VisibleObjectRefV1>(
                ReferenceIdentityComparerV1.Instance);
            visibleAttackers = new List<VisibleObjectRefV1>();
            if (!TryRequireSupportedDuelVariantV1(viewModel) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "CurrentPhase",
                    out object? phaseValue) ||
                !TryMapVisiblePhaseV1(phaseValue, out string phase) ||
                !string.Equals(phase, "declare_blockers", StringComparison.Ordinal) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "GameTurnText",
                    out object? turnTextValue) ||
                !TryParseVisibleTurnV1(turnTextValue, out uint turn) ||
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
            if (seated.Active || !opponent.Active || !seated.Priority ||
                seated.Revealed.Count != 0 || opponent.Revealed.Count != 0 ||
                (opponent.Hand.Count != 0 && opponent.Hand.Count != opponent.HandCount) ||
                !TryMapVisibleInitiativeHolderV1(seated, opponent, out string? initiative) ||
                initiative != null ||
                !TryRequireNoUnrepresentedVisibleModalSurfaceV1(viewModel) ||
                (!targetModalExpected &&
                    (!TryRequireVisibleAttackerDoneControlV1(viewModel, out object? doneAction) ||
                     doneAction == null)) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "StackZone",
                    out object? stackZone) || stackZone == null ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Count",
                    out object? stackCountValue) ||
                !(stackCountValue is int stackCount) || stackCount != 0 ||
                !TryReadExactPropertyV1(
                    stackZone,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.ZoneViewModel",
                    "Cards",
                    out object? stackCardsValue) ||
                !TryBoundedCollectionV1(stackCardsValue, 0, out List<object> stackCards) ||
                stackCards.Count != 0)
            {
                return false;
            }

            foreach (PlayerSnapshotV1 player in snapshots)
            {
                if (!TryReadExactPropertyV1(
                        player.Player,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.PlayerViewModel",
                        "IsTargetable",
                        out object? playerTargetableValue) ||
                    !(playerTargetableValue is bool playerTargetable) ||
                    playerTargetable)
                {
                    return false;
                }
            }

            uint nextOrdinal = 0;
            Dictionary<object, VisibleObjectRefV1> localObjectRefs = objectRefs;
            Func<object, VisibleObjectRefV1> register = card =>
            {
                if (!localObjectRefs.TryGetValue(card, out VisibleObjectRefV1 visibleRef))
                {
                    visibleRef = new VisibleObjectRefV1 { VisibleOrdinal = nextOrdinal++ };
                    localObjectRefs.Add(card, visibleRef);
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

            if (!TryMapBattlefieldForVisibleBlockerSelectionV1(
                    seated.Battlefield,
                    false,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> seatedBattlefield,
                    out List<VisibleRelationV1> relations,
                    out List<VisibleObjectRefV1> seatedAttackers) ||
                seatedAttackers.Count != 0 ||
                !TryMapBattlefieldForVisibleBlockerSelectionV1(
                    opponent.Battlefield,
                    true,
                    objectRefs,
                    out List<VisibleBattlefieldCardV1> opponentBattlefield,
                    out List<VisibleRelationV1> opponentRelations,
                    out visibleAttackers) ||
                visibleAttackers.Count == 0 || visibleAttackers.Count > 64 ||
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
            state = new VisibleStateV1
            {
                Turn = turn,
                Phase = phase,
                ActivePlayer = "opponent",
                PriorityPlayer = "seated_player",
                Initiative = null,
                LifeTotals = new[] { seated.Life, opponent.Life },
                ManaPools = new[] { seated.ManaPool, opponent.ManaPool },
                HandCounts = new[] { seated.HandCount, opponent.HandCount },
                LibraryCounts = new[] { seated.LibraryCount, opponent.LibraryCount },
                Battlefield = new[] { seatedBattlefield, opponentBattlefield },
                Graveyards = new[] { seatedGraveyard, opponentGraveyard },
                Exile = seatedExile,
                Stack = new List<VisibleStackItemV1>(),
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
            seatedBattlefieldCards = seated.Battlefield;
            opponentBattlefieldCards = opponent.Battlefield;
            return true;
        }

        private static bool TryRequireVisibleTargetSelectionModeV1(object viewModel)
        {
            return TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "InteractionState",
                    out object? interactionState) && interactionState != null &&
                TryReadExactPropertyV1(
                    interactionState,
                    "DuelScene",
                    "Shiny.Play.Duel.Utility.InteractionState",
                    "Mode",
                    out object? modeValue) && modeValue != null &&
                modeValue.GetType().FullName == "Shiny.Play.Duel.Utility.InteractMode" &&
                string.Equals(
                    Enum.GetName(modeValue.GetType(), modeValue),
                    "SelectTargets",
                    StringComparison.Ordinal);
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
            if (!TryReadExactPropertyV1(player, "DuelScene", playerType, "Name", out object? nameValue) || !(nameValue is string visibleName) || !IsBoundedVisibleStringV1(visibleName, 256, true) ||
                !TryReadExactPropertyV1(player, "DuelScene", playerType, "LocalPlayer", out object? localValue) || !(localValue is bool local) ||
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
            snapshot.VisibleName = visibleName;
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
            const string promptType = "Shiny.Play.Duel.ViewModel.PromptBoxViewModel";
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
                    promptType,
                    "IsPromptBoxActive",
                    out object? activeValue) ||
                !(activeValue is bool active) || !active ||
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
                !TryReadVisiblePriorityControlV1(
                    okButton,
                    out bool okEligible,
                    out _) ||
                !TryReadVisiblePriorityControlV1(
                    doneButton,
                    out bool doneEligible,
                    out _) ||
                (okEligible ? 1 : 0) + (doneEligible ? 1 : 0) != 1 ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    promptType,
                    "ManaButtons",
                    out object? manaButtonsValue) ||
                !TryBoundedCollectionV1(
                    manaButtonsValue,
                    16,
                    out List<object> manaButtons) ||
                manaButtons.Count != 0 ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    promptType,
                    "NumberEntry",
                    out object? numberEntry) ||
                numberEntry == null ||
                !TryReadExactPropertyV1(
                    numberEntry,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.NumberEntryData",
                    "Enabled",
                    out object? numberEntryEnabledValue) ||
                !(numberEntryEnabledValue is bool numberEntryEnabled) ||
                numberEntryEnabled ||
                !TryReadExactPropertyV1(
                    promptBox,
                    "DuelScene",
                    promptType,
                    "StandardButtons",
                    out object? buttonsValue) ||
                !TryBoundedCollectionV1(buttonsValue, 64, out List<object> buttons))
            {
                return false;
            }
            object selectedPriorityControl = okEligible ? okButton! : doneButton!;
            int visibleEnabledButtonCount = 0;
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
                    (visible && enabled &&
                        !ReferenceEquals(button, selectedPriorityControl)))
                {
                    return false;
                }
                if (visible && enabled)
                {
                    visibleEnabledButtonCount++;
                }
            }
            return visibleEnabledButtonCount == 1;
        }

        private static bool TryRequireVisibleAttackerDoneControlV1(
            object viewModel,
            out object? selectedDoneAction)
        {
            selectedDoneAction = null;
            const string promptType = "Shiny.Play.Duel.ViewModel.PromptBoxViewModel";
            const string buttonType = "Shiny.Play.Duel.ViewModel.OptionButton";
            if (!TryReadExactPropertyV1(viewModel, "DuelScene", DuelViewModelType, "PromptBox", out object? promptBox) || promptBox == null ||
                !TryReadExactPropertyV1(promptBox, "DuelScene", promptType, "IsPromptBoxActive", out object? activeValue) || !(activeValue is bool active) || !active ||
                !TryReadExactPrivateVisibleActionPropertyV1(promptBox, "DuelScene", promptType, "OkPromptButton", out object? okButton) || okButton != null ||
                !TryReadExactPrivateVisibleActionPropertyV1(promptBox, "DuelScene", promptType, "DoneButton", out object? doneButton) || doneButton == null ||
                !TryReadExactPropertyV1(doneButton, "DuelScene", buttonType, "Name", out object? doneNameValue) || !(doneNameValue is string doneName) || !string.Equals(doneName, "Done", StringComparison.Ordinal) ||
                !TryReadExactPropertyV1(doneButton, "DuelScene", buttonType, "Visible", out object? doneVisibleValue) || !(doneVisibleValue is bool doneVisible) || !doneVisible ||
                !TryReadExactPropertyV1(doneButton, "DuelScene", buttonType, "Enabled", out object? doneEnabledValue) || !(doneEnabledValue is bool doneEnabled) || !doneEnabled ||
                !TryReadExactPrivateVisibleActionPropertyV1(doneButton, "DuelScene", buttonType, "Action", out object? doneAction) || doneAction == null ||
                !TryValidatePrivateVisibleActionV1(doneAction, false) ||
                !TryReadExactPrivateVisibleActionPropertyV1(doneAction, "WotC.MtGO.Client.Model.Reference", "WotC.MtGO.Client.Model.Play.IGameAction", "Name", out object? doneActionNameValue) ||
                !(doneActionNameValue is string doneActionName) || !string.Equals(doneActionName, "Done", StringComparison.Ordinal) ||
                !TryReadExactPropertyV1(promptBox, "DuelScene", promptType, "ManaButtons", out object? manaButtonsValue) ||
                !TryBoundedCollectionV1(manaButtonsValue, 16, out List<object> manaButtons) || manaButtons.Count != 0 ||
                !TryReadExactPropertyV1(promptBox, "DuelScene", promptType, "NumberEntry", out object? numberEntry) || numberEntry == null ||
                !TryReadExactPropertyV1(numberEntry, "DuelScene", "Shiny.Play.Duel.ViewModel.NumberEntryData", "Enabled", out object? numberEntryEnabledValue) || !(numberEntryEnabledValue is bool numberEntryEnabled) || numberEntryEnabled ||
                !TryReadExactPropertyV1(promptBox, "DuelScene", promptType, "StandardButtons", out object? buttonsValue) ||
                !TryBoundedCollectionV1(buttonsValue, 64, out List<object> buttons))
            {
                return false;
            }
            int visibleEnabledCount = 0;
            foreach (object button in buttons)
            {
                if (!TryReadExactPropertyV1(button, "DuelScene", buttonType, "Visible", out object? visibleValue) || !(visibleValue is bool visible) ||
                    !TryReadExactPropertyV1(button, "DuelScene", buttonType, "Enabled", out object? enabledValue) || !(enabledValue is bool enabled))
                {
                    return false;
                }
                if (visible && enabled)
                {
                    visibleEnabledCount++;
                    if (!ReferenceEquals(button, doneButton))
                    {
                        return false;
                    }
                }
            }
            if (visibleEnabledCount != 1)
            {
                return false;
            }
            selectedDoneAction = doneAction;
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

        private static bool IsSupportedNoncombatPriorityPhaseV1(string phase)
        {
            return phase == "upkeep" ||
                phase == "draw" ||
                phase == "main1" ||
                phase == "main2" ||
                phase == "end";
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

        private static bool TryMapBattlefieldForAttackerSelectionV1(
            List<object> cards,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleBattlefieldCardV1> mapped,
            out List<VisibleRelationV1> relations,
            out List<VisibleObjectRefV1> currentlyAttacking)
        {
            mapped = new List<VisibleBattlefieldCardV1>();
            relations = new List<VisibleRelationV1>();
            currentlyAttacking = new List<VisibleObjectRefV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsTapped", out object? tappedValue) || !(tappedValue is bool tapped) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsAttacking", out object? attackingValue) || !(attackingValue is bool attacking) ||
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
                VisibleObjectRefV1 objectRef = refs[card];
                if (attacking)
                {
                    currentlyAttacking.Add(objectRef);
                }
                mapped.Add(new VisibleBattlefieldCardV1
                {
                    ObjectRef = objectRef,
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

        private static bool TryMapBattlefieldForSingleAttackerBlockerSelectionV1(
            List<object> cards,
            bool collectAttackers,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleBattlefieldCardV1> mapped,
            out List<VisibleRelationV1> relations,
            out List<VisibleObjectRefV1> visibleAttackers)
        {
            mapped = new List<VisibleBattlefieldCardV1>();
            relations = new List<VisibleRelationV1>();
            visibleAttackers = new List<VisibleObjectRefV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsTapped", out object? tappedValue) || !(tappedValue is bool tapped) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "VisuallyAttacking", out object? attackingValue) || !(attackingValue is bool attacking) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "VisuallyBlocking", out object? blockingValue) || !(blockingValue is bool blocking) || blocking ||
                    (!collectAttackers && attacking) ||
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
                VisibleObjectRefV1 objectRef = refs[card];
                if (collectAttackers && attacking)
                {
                    visibleAttackers.Add(objectRef);
                }
                mapped.Add(new VisibleBattlefieldCardV1
                {
                    ObjectRef = objectRef,
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

        private static bool TryMapBattlefieldForVisibleBlockerSelectionV1(
            List<object> cards,
            bool collectAttackers,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleBattlefieldCardV1> mapped,
            out List<VisibleRelationV1> relations,
            out List<VisibleObjectRefV1> visibleAttackers)
        {
            mapped = new List<VisibleBattlefieldCardV1>();
            relations = new List<VisibleRelationV1>();
            visibleAttackers = new List<VisibleObjectRefV1>();
            foreach (object card in cards)
            {
                if (!TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) || !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) || !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsTapped", out object? tappedValue) || !(tappedValue is bool tapped) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "VisuallyAttacking", out object? attackingValue) || !(attackingValue is bool attacking) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "VisuallyBlocking", out object? blockingValue) || !(blockingValue is bool) ||
                    (!collectAttackers && attacking) ||
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
                VisibleObjectRefV1 objectRef = refs[card];
                if (collectAttackers && attacking)
                {
                    visibleAttackers.Add(objectRef);
                }
                mapped.Add(new VisibleBattlefieldCardV1
                {
                    ObjectRef = objectRef,
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

        private static bool TryMapVisibleBlockerAssignmentsV1(
            List<object> seatedBattlefield,
            List<object> opponentBattlefield,
            Dictionary<object, VisibleObjectRefV1> refs,
            List<VisibleObjectRefV1> visibleAttackers,
            out List<VisibleBlockerAssignmentV1> assignments,
            out List<SealedVisibleBlockerAssignmentV1> sealedAssignments)
        {
            assignments = new List<VisibleBlockerAssignmentV1>();
            sealedAssignments = new List<SealedVisibleBlockerAssignmentV1>();
            var attackerByBackingCard = new Dictionary<object, VisibleObjectRefV1>(
                ReferenceIdentityComparerV1.Instance);
            for (int index = 0; index < opponentBattlefield.Count; index++)
            {
                object card = opponentBattlefield[index];
                if (!TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyAttacking",
                        out object? attackingValue) ||
                    !(attackingValue is bool attacking) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "GameCard",
                        out object? gameCard))
                {
                    return false;
                }
                if (attacking)
                {
                    if (gameCard == null || attackerByBackingCard.ContainsKey(gameCard))
                    {
                        return false;
                    }
                    attackerByBackingCard.Add(gameCard, refs[card]);
                }
            }
            if (attackerByBackingCard.Count != visibleAttackers.Count)
            {
                return false;
            }

            var orderedByAttacker = visibleAttackers.ToDictionary(
                attacker => attacker.VisibleOrdinal,
                _ => new List<Tuple<int, VisibleObjectRefV1>>());
            foreach (object card in seatedBattlefield)
            {
                if (!TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyBlocking",
                        out object? blockingValue) ||
                    !(blockingValue is bool blocking) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisualBlockingOrders",
                        out object? ordersValue) ||
                    !TryBoundedCollectionV1(ordersValue, 64, out List<object> orders) ||
                    blocking != (orders.Count > 0))
                {
                    return false;
                }
                // Multiple blocking destinations for one permanent require a
                // separate layout qualification. Normal blocking admits one.
                if (orders.Count > 1)
                {
                    return false;
                }
                foreach (object order in orders)
                {
                    if (!TryReadExactPrivateVisibleCombatFieldV1(
                            order,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.OrderedCombatParticipant",
                            "Order",
                            out object? orderValue) ||
                        !(orderValue is int position) || position <= 0 || position > 64 ||
                        !TryReadExactPrivateVisibleCombatFieldV1(
                            order,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.OrderedCombatParticipant",
                            "Target",
                            out object? target) || target == null ||
                        !attackerByBackingCard.TryGetValue(
                            target,
                            out VisibleObjectRefV1? attacker))
                    {
                        return false;
                    }
                    orderedByAttacker[attacker.VisibleOrdinal].Add(
                        Tuple.Create(position, refs[card]));
                }
            }

            foreach (VisibleObjectRefV1 attacker in visibleAttackers)
            {
                List<Tuple<int, VisibleObjectRefV1>> blockers =
                    orderedByAttacker[attacker.VisibleOrdinal];
                if (blockers.Count == 0)
                {
                    continue;
                }
                blockers.Sort((left, right) => left.Item1.CompareTo(right.Item1));
                for (int index = 0; index < blockers.Count; index++)
                {
                    if (blockers[index].Item1 != index + 1)
                    {
                        return false;
                    }
                }
                assignments.Add(new VisibleBlockerAssignmentV1
                {
                    Attacker = attacker,
                    OrderedBlockers = blockers.Select(blocker => blocker.Item2).ToList()
                });
                sealedAssignments.Add(new SealedVisibleBlockerAssignmentV1
                {
                    AttackerVisibleOrdinal = attacker.VisibleOrdinal,
                    OrderedBlockerVisibleOrdinals = blockers
                        .Select(blocker => blocker.Item2.VisibleOrdinal)
                        .ToList()
                });
            }
            return true;
        }

        private static bool TryMapVisibleMultiAttackerBlockerCandidatesV1(
            List<object> seatedBattlefield,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleObjectRefV1> candidates,
            out List<SealedVisibleBlockerBindingV1> bindings)
        {
            candidates = new List<VisibleObjectRefV1>();
            bindings = new List<SealedVisibleBlockerBindingV1>();
            foreach (object card in seatedBattlefield)
            {
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? blocker) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyBlocking",
                        out object? blockingValue) ||
                    !(blockingValue is bool currentlyBlocking) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "HasNoBlockingAction",
                        out object? noBlockingValue) ||
                    !(noBlockingValue is bool hasNoBlockingAction) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionsValue) ||
                    !TryBoundedCollectionV1(actionsValue, 64, out List<object> actions))
                {
                    return false;
                }
                int blockActionCount = 0;
                object? blockAction = null;
                foreach (object action in actions)
                {
                    if (!TryReadExactPrivateVisibleActionPropertyV1(
                            action,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.IGameAction",
                            "Name",
                            out object? nameValue) ||
                        !(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 512, true))
                    {
                        return false;
                    }
                    if (!string.Equals(name, "Block", StringComparison.Ordinal))
                    {
                        continue;
                    }
                    if (!TryRequireSimpleVisibleMultiAttackerBlockerActionV1(action))
                    {
                        return false;
                    }
                    blockActionCount++;
                    blockAction = action;
                }
                if (hasNoBlockingAction != (blockActionCount == 0) || blockActionCount > 1)
                {
                    return false;
                }
                if (blockActionCount == 1 && !currentlyBlocking)
                {
                    candidates.Add(blocker);
                    bindings.Add(new SealedVisibleBlockerBindingV1
                    {
                        BlockerCard = card,
                        BlockerVisibleOrdinal = blocker.VisibleOrdinal,
                        BlockAction = blockAction!
                    });
                }
            }
            return candidates.Count <= 64 && candidates.Count == bindings.Count;
        }

        private static bool TryRequireSimpleVisibleMultiAttackerBlockerActionV1(object action)
        {
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string gameAction = "WotC.MtGO.Client.Model.Play.IGameAction";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            if (action.GetType().FullName == "Shiny.Play.Duel.GroupCardAction" ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, gameAction, "ActionType", out object? actionTypeValue) ||
                actionTypeValue == null || actionTypeValue.GetType().FullName != "WotC.MtGO.Client.Model.Play.ActionType" ||
                !string.Equals(Enum.GetName(actionTypeValue.GetType(), actionTypeValue), "CardAction", StringComparison.Ordinal) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "CanBePerformedLocally", out object? localValue) || !(localValue is bool local) || !local ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsManaAbility", out object? manaValue) || !(manaValue is bool mana) || mana ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsActivatedAbility", out object? activatedValue) || !(activatedValue is bool activated) || activated ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsCastAction", out object? castValue) || !(castValue is bool cast) || cast ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ModeOptions", out object? modesValue) ||
                (modesValue != null && (!TryBoundedCollectionV1(modesValue, 0, out List<object> modes) || modes.Count != 0)) ||
                !TryRequireBasicVisibleCardActionMenuShapeV1(action) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "Targets", out object? targetsValue) ||
                !TryBoundedCollectionV1(targetsValue, 1, out List<object> targets) || targets.Count != 1)
            {
                return false;
            }
            Assembly[] referenceAssemblies = AppDomain.CurrentDomain.GetAssemblies()
                .Where(candidate => string.Equals(
                    candidate.GetName().Name,
                    assembly,
                    StringComparison.Ordinal))
                .ToArray();
            Type? targetSetType = referenceAssemblies.Length == 1
                ? referenceAssemblies[0].GetType(
                    "WotC.MtGO.Client.Model.Play.ITargetSet",
                    false,
                    false)
                : null;
            if (targetSetType == null || targets[0] == null ||
                !targetSetType.IsInstanceOfType(targets[0]))
            {
                return false;
            }
            // The target-set values are deliberately not read or exported.
            // Outward choices come separately from IsTargetable presentation.
            return TryRequireNoUnrepresentedVisibleCardActionModalExceptTargetsV1(action);
        }

        private static bool TryMapVisibleSingleAttackerBlockerCandidatesV1(
            List<object> seatedBattlefield,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleSingleAttackerBlockerCandidateV1> candidates,
            out List<SealedVisibleBlockerBindingV1> bindings)
        {
            candidates = new List<VisibleSingleAttackerBlockerCandidateV1>();
            bindings = new List<SealedVisibleBlockerBindingV1>();
            foreach (object card in seatedBattlefield)
            {
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? blocker) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyBlocking",
                        out object? blockingValue) ||
                    !(blockingValue is bool currentlyBlocking) || currentlyBlocking ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "HasNoBlockingAction",
                        out object? noBlockingValue) ||
                    !(noBlockingValue is bool hasNoBlockingAction) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionsValue) ||
                    !TryBoundedCollectionV1(actionsValue, 64, out List<object> actions))
                {
                    return false;
                }
                var blockActions = new List<object>();
                foreach (object action in actions)
                {
                    if (!TryReadExactPrivateVisibleActionPropertyV1(
                            action,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.IGameAction",
                            "Name",
                            out object? nameValue) ||
                        !(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 512, true))
                    {
                        return false;
                    }
                    if (!string.Equals(name, "Block", StringComparison.Ordinal))
                    {
                        continue;
                    }
                    if (!TryRequireSimpleVisibleBlockerActionV1(action))
                    {
                        return false;
                    }
                    blockActions.Add(action);
                }
                if (hasNoBlockingAction != (blockActions.Count == 0))
                {
                    return false;
                }
                if (blockActions.Count == 0)
                {
                    continue;
                }
                if (blockActions.Count != 1)
                {
                    return false;
                }
                candidates.Add(new VisibleSingleAttackerBlockerCandidateV1
                {
                    Blocker = blocker,
                    CurrentlyBlocking = false,
                    BlockActionVisible = true
                });
                bindings.Add(new SealedVisibleBlockerBindingV1
                {
                    BlockerCard = card,
                    BlockerVisibleOrdinal = blocker.VisibleOrdinal,
                    BlockAction = blockActions.Single()
                });
            }
            return candidates.Count <= 64 && candidates.Count == bindings.Count;
        }

        private static bool TryMapVisibleSingleAttackerBlockerExecutionCandidatesV1(
            List<object> seatedBattlefield,
            Dictionary<object, VisibleObjectRefV1> refs,
            List<SealedVisibleBlockerAssignmentV1> assignments,
            out List<VisibleSingleAttackerBlockerCandidateV1> candidates,
            out List<SealedVisibleSingleBlockerBindingV1> bindings)
        {
            candidates = new List<VisibleSingleAttackerBlockerCandidateV1>();
            bindings = new List<SealedVisibleSingleBlockerBindingV1>();
            HashSet<uint> assigned = assignments
                .SelectMany(assignment => assignment.OrderedBlockerVisibleOrdinals)
                .ToHashSet();
            foreach (object card in seatedBattlefield)
            {
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? blocker) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "VisuallyBlocking",
                        out object? blockingValue) ||
                    !(blockingValue is bool currentlyBlocking) ||
                    currentlyBlocking != assigned.Contains(blocker.VisibleOrdinal) ||
                    !TryReadExactPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "HasNoBlockingAction",
                        out object? noBlockingValue) ||
                    !(noBlockingValue is bool hasNoBlockingAction) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionsValue) ||
                    !TryBoundedCollectionV1(actionsValue, 64, out List<object> actions))
                {
                    return false;
                }
                var blockActions = new List<object>();
                foreach (object action in actions)
                {
                    if (!TryReadExactPrivateVisibleActionPropertyV1(
                            action,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.IGameAction",
                            "Name",
                            out object? nameValue) ||
                        !(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 512, true))
                    {
                        return false;
                    }
                    if (!string.Equals(name, "Block", StringComparison.Ordinal))
                    {
                        continue;
                    }
                    if (!TryRequireSimpleVisibleBlockerActionV1(action))
                    {
                        return false;
                    }
                    blockActions.Add(action);
                }
                if (blockActions.Count > 1 ||
                    hasNoBlockingAction != (blockActions.Count == 0))
                {
                    return false;
                }
                if (!currentlyBlocking && blockActions.Count == 0)
                {
                    continue;
                }
                candidates.Add(new VisibleSingleAttackerBlockerCandidateV1
                {
                    Blocker = blocker,
                    CurrentlyBlocking = currentlyBlocking,
                    BlockActionVisible = blockActions.Count == 1
                });
                bindings.Add(new SealedVisibleSingleBlockerBindingV1
                {
                    BlockerCard = card,
                    BlockerVisibleOrdinal = blocker.VisibleOrdinal,
                    CurrentlyBlocking = currentlyBlocking,
                    BlockAction = blockActions.SingleOrDefault()
                });
            }
            return candidates.Count <= 64 && candidates.Count == bindings.Count;
        }

        private static bool TryRequireSimpleVisibleBlockerActionV1(object action)
        {
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string gameAction = "WotC.MtGO.Client.Model.Play.IGameAction";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            return action.GetType().FullName != "Shiny.Play.Duel.GroupCardAction" &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, gameAction, "ActionType", out object? actionTypeValue) &&
                actionTypeValue != null &&
                actionTypeValue.GetType().FullName == "WotC.MtGO.Client.Model.Play.ActionType" &&
                string.Equals(Enum.GetName(actionTypeValue.GetType(), actionTypeValue), "CardAction", StringComparison.Ordinal) &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "CanBePerformedLocally", out object? localValue) && localValue is bool local && local &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsManaAbility", out object? manaValue) && manaValue is bool mana && !mana &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsActivatedAbility", out object? activatedValue) && activatedValue is bool activated && !activated &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsCastAction", out object? castValue) && castValue is bool cast && !cast &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ModeOptions", out object? modesValue) &&
                (modesValue == null || TryBoundedCollectionV1(modesValue, 0, out List<object> modes) && modes.Count == 0) &&
                TryRequireBasicVisibleCardActionMenuShapeV1(action) &&
                TryRequireNoUnrepresentedVisibleCardActionModalV1(action);
        }

        private static bool TryMapVisibleAttackerCandidatesV1(
            List<object> seatedBattlefield,
            string visibleOpponentName,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleAttackerCandidateV1> candidates,
            out List<SealedVisibleAttackerBindingV1> bindings)
        {
            candidates = new List<VisibleAttackerCandidateV1>();
            bindings = new List<SealedVisibleAttackerBindingV1>();
            if (!IsBoundedVisibleStringV1(visibleOpponentName, 256, true))
            {
                return false;
            }
            string exactAttackOpponentName = "Attack " + visibleOpponentName;
            foreach (object card in seatedBattlefield)
            {
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? attacker) ||
                    !TryReadExactPropertyV1(
                        card,
                        "Card",
                        "Shiny.Card.ViewModels.CardViewModel",
                        "IsAttacking",
                        out object? attackingValue) ||
                    !(attackingValue is bool currentlyAttacking) ||
                    !TryReadExactPrivateVisibleActionPropertyV1(
                        card,
                        "DuelScene",
                        "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                        "Actions",
                        out object? actionsValue) ||
                    !TryBoundedCollectionV1(actionsValue, 64, out List<object> actions))
                {
                    return false;
                }
                var attackActions = new List<object>();
                var dontAttackActions = new List<object>();
                foreach (object action in actions)
                {
                    if (!TryReadExactPrivateVisibleActionPropertyV1(
                            action,
                            "WotC.MtGO.Client.Model.Reference",
                            "WotC.MtGO.Client.Model.Play.IGameAction",
                            "Name",
                            out object? nameValue) ||
                        !(nameValue is string name) ||
                        !IsBoundedVisibleStringV1(name, 512, true))
                    {
                        return false;
                    }
                    bool attackNamed = name.StartsWith("Attack ", StringComparison.Ordinal);
                    bool dontAttackNamed = string.Equals(name, "Don't attack", StringComparison.Ordinal);
                    if (!attackNamed && !dontAttackNamed)
                    {
                        continue;
                    }
                    if (!TryRequireSimpleVisibleAttackerToggleActionV1(action, name))
                    {
                        return false;
                    }
                    if (attackNamed)
                    {
                        if (!string.Equals(name, exactAttackOpponentName, StringComparison.Ordinal))
                        {
                            return false;
                        }
                        attackActions.Add(action);
                    }
                    else
                    {
                        dontAttackActions.Add(action);
                    }
                }
                if (attackActions.Count == 0 && dontAttackActions.Count == 0)
                {
                    continue;
                }
                if ((currentlyAttacking &&
                        (attackActions.Count != 0 || dontAttackActions.Count != 1)) ||
                    (!currentlyAttacking &&
                        (attackActions.Count != 1 || dontAttackActions.Count != 0)))
                {
                    return false;
                }
                candidates.Add(new VisibleAttackerCandidateV1
                {
                    Attacker = attacker,
                    CurrentlyAttacking = currentlyAttacking,
                    AttackOpponentActionVisible = attackActions.Count == 1,
                    DontAttackActionVisible = dontAttackActions.Count == 1
                });
                bindings.Add(new SealedVisibleAttackerBindingV1
                {
                    Card = card,
                    VisibleOrdinal = attacker.VisibleOrdinal,
                    CurrentlyAttacking = currentlyAttacking,
                    ToggleAction = currentlyAttacking
                        ? dontAttackActions.Single()
                        : attackActions.Single()
                });
            }
            return candidates.Count <= 64 && bindings.Count == candidates.Count;
        }

        private static bool TryRequireSimpleVisibleAttackerToggleActionV1(
            object action,
            string visibleName)
        {
            const string assembly = "WotC.MtGO.Client.Model.Reference";
            const string gameAction = "WotC.MtGO.Client.Model.Play.IGameAction";
            const string cardAction = "WotC.MtGO.Client.Model.Play.ICardAction";
            if (visibleName.EndsWith("and exert", StringComparison.OrdinalIgnoreCase) ||
                action.GetType().FullName == "Shiny.Play.Duel.GroupCardAction" ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, gameAction, "ActionType", out object? actionTypeValue) ||
                actionTypeValue == null ||
                actionTypeValue.GetType().FullName != "WotC.MtGO.Client.Model.Play.ActionType" ||
                !string.Equals(Enum.GetName(actionTypeValue.GetType(), actionTypeValue), "CardAction", StringComparison.Ordinal) ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "CanBePerformedLocally", out object? localValue) || !(localValue is bool local) || !local ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsManaAbility", out object? manaValue) || !(manaValue is bool mana) || mana ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsActivatedAbility", out object? activatedValue) || !(activatedValue is bool activated) || activated ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsCastAction", out object? castValue) || !(castValue is bool cast) || cast ||
                !TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ModeOptions", out object? modesValue) ||
                (modesValue != null && (!TryBoundedCollectionV1(modesValue, 64, out List<object> modes) || modes.Count != 0)) ||
                !TryRequireNoUnrepresentedVisibleCardActionModalV1(action))
            {
                return false;
            }
            bool isAttack = visibleName.StartsWith("Attack ", StringComparison.Ordinal);
            return TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "AltMenuAction", out object? altValue) && altValue is bool alt && !alt &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "GroupName", out object? groupValue) && (groupValue == null || groupValue is string group && group.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ActionChoices", out object? choicesValue) && (choicesValue == null || choicesValue is string choices && choices.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "ModeChoiceMapping", out object? mappingValue) && (mappingValue == null || mappingValue is string mapping && mapping.Length == 0) &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "IsSubmenuItem", out object? submenuValue) && submenuValue is bool submenu && !submenu &&
                TryReadExactPrivateVisibleActionPropertyV1(action, assembly, cardAction, "AttackVictimId", out object? victimValue) && victimValue is int victim &&
                (isAttack ? victim >= 0 : victim == -1);
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

        private static bool TryMapNarrowVisibleStackV1(
            List<object> cards,
            Dictionary<object, VisibleObjectRefV1> refs,
            out List<VisibleStackItemV1> mapped)
        {
            mapped = new List<VisibleStackItemV1>();
            for (int index = 0; index < cards.Count; index++)
            {
                object card = cards[index];
                if (!refs.TryGetValue(card, out VisibleObjectRefV1? source) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsFaceDown", out object? faceDownValue) ||
                    !(faceDownValue is bool faceDown) || faceDown ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "IsClone", out object? cloneValue) ||
                    !(cloneValue is bool clone) || clone ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "CardFrameID", out object? frameValue) ||
                    frameValue == null || frameValue.GetType().FullName != "Shiny.Card.Enums.FrameStyle" ||
                    Enum.GetName(frameValue.GetType(), frameValue) == null ||
                    string.Equals(Enum.GetName(frameValue.GetType(), frameValue), "AbilityOrEffect", StringComparison.Ordinal) ||
                    !TryReadExactPropertyV1(card, "Card", "Shiny.Card.ViewModels.CardViewModel", "Name", out object? nameValue) ||
                    !(nameValue is string name) || !IsBoundedVisibleStringV1(name, 256, true) ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "IsAbilityOnTheStack", out object? abilityValue) ||
                    !(abilityValue is bool ability) || ability ||
                    !TryReadExactPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "IsController", out object? controllerValue) ||
                    !(controllerValue is bool controller) || !controller ||
                    !TryReadExactPrivateVisibleActionPropertyV1(card, "DuelScene", "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel", "Associations", out object? associationsValue) ||
                    !TryBoundedCollectionV1(associationsValue, 128, out List<object> associations) ||
                    associations.Count != 0)
                {
                    return false;
                }
                mapped.Add(new VisibleStackItemV1
                {
                    VisibleStackPosition = checked((uint)index),
                    SourceObjectRef = source,
                    VisibleSourceName = name
                });
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
            // visible and enabled default OK control.
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
            return actions.Count < 64 &&
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
            if (!string.Equals(name, "OK", StringComparison.OrdinalIgnoreCase))
            {
                return false;
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
                    "ActionType",
                    out object? actionTypeValue) ||
                actionTypeValue == null ||
                actionTypeValue.GetType().FullName !=
                    "WotC.MtGO.Client.Model.Play.ActionType" ||
                !string.Equals(
                    Enum.GetName(actionTypeValue.GetType(), actionTypeValue),
                    "ChooseOption",
                    StringComparison.Ordinal) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    action,
                    "WotC.MtGO.Client.Model.Reference",
                    "WotC.MtGO.Client.Model.Play.IGameAction",
                    "ActionFlags",
                    out object? actionFlagsValue) ||
                !(actionFlagsValue is uint actionFlags) || actionFlags != 1u ||
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
                !TryRequireBasicVisibleCardActionMenuShapeV1(action) ||
                !TryRequireNoUnrepresentedVisibleCardActionModalV1(action))
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
