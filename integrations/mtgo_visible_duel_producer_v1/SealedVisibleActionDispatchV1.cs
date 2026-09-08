using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO.MemoryMappedFiles;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using System.Windows.Media;

namespace MtgKernel.Mtgo.VisibleDuelProducer.V1
{
    public static partial class VisibleDuelProducerV1
    {
        private const int OrdinaryDispatchCommandSchemaV1 = 2;
        private const int AttackerDispatchCommandSchemaV1 = 3;
        private const int BlockerDispatchCommandSchemaV1 = 4;
        private const int SingleBlockerDispatchCommandSchemaV1 = 5;
        private const int MaximumDispatchCommandBytesV1 = 384;
        private const string DispatchCommandPrefixV1 = "execute_visible_action_v1|";
        private const string AttackerDispatchCommandPrefixV1 =
            "execute_visible_attacker_step_v1|";
        private const string BlockerDispatchCommandPrefixV1 =
            "execute_visible_blocker_step_v1|";
        private const string SingleBlockerDispatchCommandPrefixV1 =
            "execute_visible_single_blocker_step_v1|";
        private const string AttackerPlanCommitmentDomainV1 =
            "mtgo-visible-attacker-execution-plan-v1";
        private const string BlockerStepCommitmentDomainV1 =
            "mtgo-visible-multi-attacker-blocker-execution-step-v1";
        private const string SingleBlockerPlanCommitmentDomainV1 =
            "mtgo-visible-single-attacker-blocker-execution-plan-v1";
        private static readonly byte[] VisibleActionSubmitted = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}");
        private static readonly byte[] VisibleActionRejected = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}");
        private static readonly object DispatchedVisibleDecisionLockV1 = new object();
        private static readonly HashSet<string> DispatchedVisibleDecisionsV1 =
            new HashSet<string>(StringComparer.Ordinal);
        private static readonly SealedVisibleAttackerPlanLedgerV1
            VisibleAttackerPlanLedgerV1 = new SealedVisibleAttackerPlanLedgerV1();
        private static readonly SealedVisibleBlockerPlanLedgerV1
            VisibleBlockerPlanLedgerV1 = new SealedVisibleBlockerPlanLedgerV1();
        private static readonly SealedVisibleSingleBlockerPlanLedgerV1
            VisibleSingleBlockerPlanLedgerV1 = new SealedVisibleSingleBlockerPlanLedgerV1();

        internal enum SealedVisibleDispatchKindV1
        {
            OrdinaryAction,
            AttackerStep,
            BlockerStep,
            SingleBlockerStep
        }

        internal sealed class SealedVisibleAttackerBindingV1
        {
            internal object Card = new object();
            internal uint VisibleOrdinal;
            internal bool CurrentlyAttacking;
            internal object ToggleAction = new object();
        }

        internal sealed class SealedVisibleBlockerBindingV1
        {
            internal object BlockerCard = new object();
            internal uint BlockerVisibleOrdinal;
            internal object BlockAction = new object();
        }

        internal sealed class SealedVisibleBlockerTargetBindingV1
        {
            internal object AttackerCard = new object();
            internal uint AttackerVisibleOrdinal;
        }

        internal sealed class SealedVisibleBlockerAssignmentV1
        {
            internal uint AttackerVisibleOrdinal;
            internal List<uint> OrderedBlockerVisibleOrdinals = new List<uint>();
        }

        internal sealed class SealedVisibleSingleBlockerBindingV1
        {
            internal object BlockerCard = new object();
            internal uint BlockerVisibleOrdinal;
            internal bool CurrentlyBlocking;
            internal object? BlockAction;
        }

        internal sealed class SealedVisibleActionDispatchRequestV1
        {
            internal SealedVisibleDispatchKindV1 Kind;
            internal string ExpectedDecisionSha256 = string.Empty;
            internal int SelectedIndex;
            internal int AttackerCandidateCount;
            internal ulong DesiredAttackerMask;
            internal string DesiredAttackerMaskHex = string.Empty;
            internal string AttackerPlanCommitmentSha256 = string.Empty;
            internal char BlockerOperationKind;
            internal uint BlockerVisibleOrdinal;
            internal uint AttackerVisibleOrdinal;
            internal string ModelSelectionCommitmentSha256 = string.Empty;
            internal string BlockerStepCommitmentSha256 = string.Empty;
        }

        private sealed class SealedVisibleAttackerPlanStateV1
        {
            internal string PlanCommitmentSha256 = string.Empty;
            internal int CandidateCount;
            internal ulong DesiredMask;
            internal uint Turn;
            internal string VisibleUniverseSha256 = string.Empty;
            internal uint[] CandidateVisibleOrdinals = Array.Empty<uint>();
            internal bool[] ExpectedCurrentSelection = Array.Empty<bool>();
        }

        private sealed class SealedVisibleAttackerPlanLedgerV1
        {
            internal SealedVisibleAttackerPlanStateV1? ActivePlan;
            internal HashSet<string> UsedPlanCommitments =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> UsedSourceSelectionHashes =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> CompletedVisibleUniverseHashes =
                new HashSet<string>(StringComparer.Ordinal);
        }

        private enum SealedVisibleBlockerPlanPhaseV1
        {
            AwaitingTargetPrompt,
            AwaitingVisibleAssignment
        }

        private sealed class SealedVisibleBlockerPlanStateV1
        {
            internal SealedVisibleBlockerPlanPhaseV1 Phase;
            internal uint BlockerVisibleOrdinal;
            internal uint AttackerVisibleOrdinal;
            internal uint Turn;
            internal string VisibleUniverseSha256 = string.Empty;
        }

        private sealed class SealedVisibleBlockerPlanLedgerV1
        {
            internal SealedVisibleBlockerPlanStateV1? ActivePlan;
            internal HashSet<string> UsedSourceSelectionHashes =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> UsedModelSelectionCommitments =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> UsedStepCommitments =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> CompletedVisibleUniverseHashes =
                new HashSet<string>(StringComparer.Ordinal);
        }

        private sealed class SealedVisibleSingleBlockerPlanStateV1
        {
            internal string PlanCommitmentSha256 = string.Empty;
            internal int CandidateCount;
            internal ulong DesiredMask;
            internal uint Turn;
            internal string VisibleUniverseSha256 = string.Empty;
            internal uint[] CandidateVisibleOrdinals = Array.Empty<uint>();
            internal bool[] ExpectedCurrentSelection = Array.Empty<bool>();
        }

        private sealed class SealedVisibleSingleBlockerPlanLedgerV1
        {
            internal SealedVisibleSingleBlockerPlanStateV1? ActivePlan;
            internal HashSet<string> UsedPlanCommitments =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> UsedSourceSelectionHashes =
                new HashSet<string>(StringComparer.Ordinal);
            internal HashSet<string> CompletedVisibleUniverseHashes =
                new HashSet<string>(StringComparer.Ordinal);
        }

        private static bool TryReadSealedVisibleActionDispatchRequestV1(
            string channelName,
            SealedVisibleDispatchKindV1 expectedKind,
            out SealedVisibleActionDispatchRequestV1? request)
        {
            request = null;
            try
            {
                using (MemoryMappedFile channel = MemoryMappedFile.OpenExisting(
                    channelName,
                    MemoryMappedFileRights.ReadWrite))
                using (MemoryMappedViewAccessor view = channel.CreateViewAccessor(
                    0,
                    MaximumOutputBytes,
                    MemoryMappedFileAccess.ReadWrite))
                {
                    int length = view.ReadInt32(0);
                    int schema = view.ReadInt32(4);
                    if (length == 0 && schema == 0)
                    {
                        return false;
                    }
                    int expectedSchema = expectedKind == SealedVisibleDispatchKindV1.OrdinaryAction
                        ? OrdinaryDispatchCommandSchemaV1
                        : expectedKind == SealedVisibleDispatchKindV1.AttackerStep
                            ? AttackerDispatchCommandSchemaV1
                            : expectedKind == SealedVisibleDispatchKindV1.BlockerStep
                                ? BlockerDispatchCommandSchemaV1
                                : SingleBlockerDispatchCommandSchemaV1;
                    if (schema != expectedSchema || length <= 0 ||
                        length > MaximumDispatchCommandBytesV1)
                    {
                        return false;
                    }
                    var bytes = new byte[length];
                    if (view.ReadArray(OutputPayloadOffset, bytes, 0, length) != length ||
                        bytes.Any(value => value < 0x20 || value > 0x7e))
                    {
                        return false;
                    }
                    string command = Encoding.ASCII.GetString(bytes);
                    string[] parts = command.Split('|');
                    if (expectedKind == SealedVisibleDispatchKindV1.OrdinaryAction)
                    {
                        if (parts.Length != 3 ||
                            !string.Equals(parts[0], DispatchCommandPrefixV1.TrimEnd('|'), StringComparison.Ordinal) ||
                            !IsLowerSha256V1(parts[1]) ||
                            !int.TryParse(parts[2], NumberStyles.None, CultureInfo.InvariantCulture, out int index) ||
                            index < 0 || index >= 64 ||
                            !string.Equals(parts[2], index.ToString(CultureInfo.InvariantCulture), StringComparison.Ordinal))
                        {
                            return false;
                        }
                        request = new SealedVisibleActionDispatchRequestV1
                        {
                            Kind = expectedKind,
                            ExpectedDecisionSha256 = parts[1],
                            SelectedIndex = index
                        };
                    }
                    else if (expectedKind == SealedVisibleDispatchKindV1.AttackerStep ||
                        expectedKind == SealedVisibleDispatchKindV1.SingleBlockerStep)
                    {
                        if (parts.Length != 5 ||
                            !string.Equals(
                                parts[0],
                                (expectedKind == SealedVisibleDispatchKindV1.AttackerStep
                                    ? AttackerDispatchCommandPrefixV1
                                    : SingleBlockerDispatchCommandPrefixV1).TrimEnd('|'),
                                StringComparison.Ordinal) ||
                            !IsLowerSha256V1(parts[1]) ||
                            !int.TryParse(parts[2], NumberStyles.None, CultureInfo.InvariantCulture, out int candidateCount) ||
                            candidateCount < 0 || candidateCount > 64 ||
                            !string.Equals(parts[2], candidateCount.ToString(CultureInfo.InvariantCulture), StringComparison.Ordinal) ||
                            parts[3].Length != 16 ||
                            parts[3].Any(character => !((character >= '0' && character <= '9') || (character >= 'a' && character <= 'f'))) ||
                            !ulong.TryParse(parts[3], NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out ulong desiredMask) ||
                            (candidateCount < 64 && (desiredMask >> candidateCount) != 0) ||
                            !IsLowerSha256V1(parts[4]))
                        {
                            return false;
                        }
                        request = new SealedVisibleActionDispatchRequestV1
                        {
                            Kind = expectedKind,
                            ExpectedDecisionSha256 = parts[1],
                            AttackerCandidateCount = candidateCount,
                            DesiredAttackerMask = desiredMask,
                            DesiredAttackerMaskHex = parts[3],
                            AttackerPlanCommitmentSha256 = parts[4]
                        };
                    }
                    else
                    {
                        if (parts.Length != 8 ||
                            !string.Equals(parts[0], BlockerDispatchCommandPrefixV1.TrimEnd('|'), StringComparison.Ordinal) ||
                            !IsLowerSha256V1(parts[1]) ||
                            !int.TryParse(parts[2], NumberStyles.None, CultureInfo.InvariantCulture, out int selectedIndex) ||
                            selectedIndex < 0 || selectedIndex >= 64 ||
                            !string.Equals(parts[2], selectedIndex.ToString(CultureInfo.InvariantCulture), StringComparison.Ordinal) ||
                            parts[3].Length != 1 || "fbt".IndexOf(parts[3][0]) < 0 ||
                            !TryParseVisibleOrdinalOrSentinelV1(parts[4], parts[3][0] != 'f', out uint blockerOrdinal) ||
                            !TryParseVisibleOrdinalOrSentinelV1(parts[5], parts[3][0] == 't', out uint attackerOrdinal) ||
                            !IsLowerSha256V1(parts[6]) ||
                            !IsLowerSha256V1(parts[7]))
                        {
                            return false;
                        }
                        request = new SealedVisibleActionDispatchRequestV1
                        {
                            Kind = expectedKind,
                            ExpectedDecisionSha256 = parts[1],
                            SelectedIndex = selectedIndex,
                            BlockerOperationKind = parts[3][0],
                            BlockerVisibleOrdinal = blockerOrdinal,
                            AttackerVisibleOrdinal = attackerOrdinal,
                            ModelSelectionCommitmentSha256 = parts[6],
                            BlockerStepCommitmentSha256 = parts[7]
                        };
                    }
                    Array.Clear(bytes, 0, bytes.Length);
                    return true;
                }
            }
            catch
            {
                request = null;
                return false;
            }
        }

        private static bool TryParseVisibleOrdinalOrSentinelV1(
            string value,
            bool required,
            out uint ordinal)
        {
            ordinal = 0;
            if (!required)
            {
                return string.Equals(value, "-", StringComparison.Ordinal);
            }
            return uint.TryParse(
                    value,
                    NumberStyles.None,
                    CultureInfo.InvariantCulture,
                    out ordinal) &&
                string.Equals(
                    value,
                    ordinal.ToString(CultureInfo.InvariantCulture),
                    StringComparison.Ordinal);
        }

        private static bool TryExecuteSealedVisibleActionV1(
            object viewModel,
            byte[] currentSanitizedDecision,
            List<object> boundClientActions,
            SealedVisibleActionDispatchRequestV1 request)
        {
            if (!string.Equals(
                    LowerSha256V1(currentSanitizedDecision),
                    request.ExpectedDecisionSha256,
                    StringComparison.Ordinal) ||
                request.SelectedIndex < 0 ||
                request.SelectedIndex >= boundClientActions.Count)
            {
                return false;
            }
            object action = boundClientActions[request.SelectedIndex];
            if (!TryResolveExactGameActionExecutorV1(
                    viewModel,
                    action,
                    out object? game,
                    out MethodInfo? execute) || game == null || execute == null)
            {
                return false;
            }
            // One exact visible decision authorizes at most one client call.
            // The guard retains only a sanitized hash, never the game object.
            lock (DispatchedVisibleDecisionLockV1)
            {
                if (DispatchedVisibleDecisionsV1.Count >= 4096 ||
                    !DispatchedVisibleDecisionsV1.Add(request.ExpectedDecisionSha256))
                {
                    return false;
                }
            }
            // Consumption precedes the client call. If dispatch throws after
            // an unknown amount of work, the same visible decision cannot be
            // retried and accidentally send a second input.
            execute.Invoke(game, new[] { action });
            return true;
        }

        private static bool TryExecuteSealedVisibleAttackerStepV1(
            object viewModel,
            byte[] currentSanitizedSelection,
            List<SealedVisibleAttackerBindingV1> bindings,
            object doneAction,
            uint turn,
            string visibleUniverseSha256,
            SealedVisibleActionDispatchRequestV1 request)
        {
            string currentSha256 = LowerSha256V1(currentSanitizedSelection);
            if (request.Kind != SealedVisibleDispatchKindV1.AttackerStep ||
                !string.Equals(currentSha256, request.ExpectedDecisionSha256, StringComparison.Ordinal) ||
                request.AttackerCandidateCount != bindings.Count ||
                !IsLowerSha256V1(visibleUniverseSha256))
            {
                return false;
            }
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Game",
                    out object? game) || game == null)
            {
                return false;
            }

            SealedVisibleAttackerPlanLedgerV1 ledger = VisibleAttackerPlanLedgerV1;
            lock (ledger)
            {
                SealedVisibleAttackerPlanStateV1? state = ledger.ActivePlan;
                if (state == null)
                {
                    string expectedPlanCommitment = AttackerPlanCommitmentSha256V1(
                        currentSha256,
                        request.AttackerCandidateCount,
                        request.DesiredAttackerMaskHex);
                    if (!string.Equals(
                            expectedPlanCommitment,
                        request.AttackerPlanCommitmentSha256,
                            StringComparison.Ordinal) ||
                        ledger.UsedPlanCommitments.Count >= 4096 ||
                        ledger.UsedSourceSelectionHashes.Count >= 4096 ||
                        ledger.CompletedVisibleUniverseHashes.Contains(visibleUniverseSha256) ||
                        !ledger.UsedPlanCommitments.Add(request.AttackerPlanCommitmentSha256) ||
                        !ledger.UsedSourceSelectionHashes.Add(currentSha256))
                    {
                        return false;
                    }
                    state = new SealedVisibleAttackerPlanStateV1
                    {
                        PlanCommitmentSha256 = request.AttackerPlanCommitmentSha256,
                        CandidateCount = request.AttackerCandidateCount,
                        DesiredMask = request.DesiredAttackerMask,
                        Turn = turn,
                        VisibleUniverseSha256 = visibleUniverseSha256,
                        CandidateVisibleOrdinals = bindings
                            .Select(binding => binding.VisibleOrdinal)
                            .ToArray(),
                        ExpectedCurrentSelection = bindings.Select(binding => binding.CurrentlyAttacking).ToArray()
                    };
                    ledger.ActivePlan = state;
                }
                else if (!string.Equals(
                        state.PlanCommitmentSha256,
                        request.AttackerPlanCommitmentSha256,
                        StringComparison.Ordinal) ||
                    state.CandidateCount != request.AttackerCandidateCount ||
                    state.DesiredMask != request.DesiredAttackerMask ||
                    state.Turn != turn ||
                    !string.Equals(
                        state.VisibleUniverseSha256,
                        visibleUniverseSha256,
                        StringComparison.Ordinal))
                {
                    ledger.ActivePlan = null;
                    return false;
                }

                if (state.CandidateVisibleOrdinals.Length != bindings.Count ||
                    state.ExpectedCurrentSelection.Length != bindings.Count)
                {
                    return false;
                }
                for (int index = 0; index < bindings.Count; index++)
                {
                    if (state.CandidateVisibleOrdinals[index] != bindings[index].VisibleOrdinal ||
                        state.ExpectedCurrentSelection[index] != bindings[index].CurrentlyAttacking)
                    {
                        // A missing or contradictory visible postcondition
                        // never unlocks the next input.
                        ledger.ActivePlan = null;
                        return false;
                    }
                }

                int nextToggleIndex = -1;
                for (int index = 0; index < bindings.Count; index++)
                {
                    bool desired = ((state.DesiredMask >> index) & 1UL) != 0;
                    if (bindings[index].CurrentlyAttacking != desired)
                    {
                        nextToggleIndex = index;
                        break;
                    }
                }
                object selectedAction = nextToggleIndex >= 0
                    ? bindings[nextToggleIndex].ToggleAction
                    : doneAction;
                if (!TryResolveExactGameActionExecutorV1(
                        viewModel,
                        selectedAction,
                        out object? resolvedGame,
                        out MethodInfo? execute) ||
                    resolvedGame == null || execute == null ||
                    !ReferenceEquals(game, resolvedGame))
                {
                    ledger.ActivePlan = null;
                    return false;
                }

                if (nextToggleIndex >= 0)
                {
                    state.ExpectedCurrentSelection[nextToggleIndex] =
                        ((state.DesiredMask >> nextToggleIndex) & 1UL) != 0;
                }
                else
                {
                    // Completion is consumed before the client call. An
                    // uncertain Done invocation can never be retried.
                    if (ledger.CompletedVisibleUniverseHashes.Count >= 4096 ||
                        !ledger.CompletedVisibleUniverseHashes.Add(visibleUniverseSha256))
                    {
                        ledger.ActivePlan = null;
                        return false;
                    }
                    ledger.ActivePlan = null;
                }
                execute.Invoke(game, new[] { selectedAction });
                return true;
            }
        }

        private static bool TryExecuteSealedVisibleSingleBlockerStepV1(
            object viewModel,
            byte[] currentSanitizedSelection,
            List<SealedVisibleSingleBlockerBindingV1> bindings,
            object doneAction,
            uint turn,
            string visibleUniverseSha256,
            SealedVisibleActionDispatchRequestV1 request)
        {
            string currentSha256 = LowerSha256V1(currentSanitizedSelection);
            if (request.Kind != SealedVisibleDispatchKindV1.SingleBlockerStep ||
                !string.Equals(currentSha256, request.ExpectedDecisionSha256, StringComparison.Ordinal) ||
                request.AttackerCandidateCount != bindings.Count ||
                !IsLowerSha256V1(visibleUniverseSha256) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Game",
                    out object? game) || game == null)
            {
                return false;
            }

            SealedVisibleSingleBlockerPlanLedgerV1 ledger = VisibleSingleBlockerPlanLedgerV1;
            lock (ledger)
            {
                SealedVisibleSingleBlockerPlanStateV1? state = ledger.ActivePlan;
                if (state == null)
                {
                    string expectedPlanCommitment = SingleBlockerPlanCommitmentSha256V1(
                        currentSha256,
                        request.AttackerCandidateCount,
                        request.DesiredAttackerMaskHex);
                    if (!string.Equals(
                            expectedPlanCommitment,
                            request.AttackerPlanCommitmentSha256,
                            StringComparison.Ordinal) ||
                        bindings.Any(binding => binding.CurrentlyBlocking) ||
                        ledger.UsedPlanCommitments.Count >= 4096 ||
                        ledger.UsedSourceSelectionHashes.Count >= 4096 ||
                        ledger.CompletedVisibleUniverseHashes.Contains(visibleUniverseSha256) ||
                        !ledger.UsedPlanCommitments.Add(request.AttackerPlanCommitmentSha256) ||
                        !ledger.UsedSourceSelectionHashes.Add(currentSha256))
                    {
                        return false;
                    }
                    state = new SealedVisibleSingleBlockerPlanStateV1
                    {
                        PlanCommitmentSha256 = request.AttackerPlanCommitmentSha256,
                        CandidateCount = request.AttackerCandidateCount,
                        DesiredMask = request.DesiredAttackerMask,
                        Turn = turn,
                        VisibleUniverseSha256 = visibleUniverseSha256,
                        CandidateVisibleOrdinals = bindings
                            .Select(binding => binding.BlockerVisibleOrdinal)
                            .ToArray(),
                        ExpectedCurrentSelection = bindings
                            .Select(binding => binding.CurrentlyBlocking)
                            .ToArray()
                    };
                    ledger.ActivePlan = state;
                }
                else if (!string.Equals(
                        state.PlanCommitmentSha256,
                        request.AttackerPlanCommitmentSha256,
                        StringComparison.Ordinal) ||
                    state.CandidateCount != request.AttackerCandidateCount ||
                    state.DesiredMask != request.DesiredAttackerMask ||
                    state.Turn != turn ||
                    !string.Equals(
                        state.VisibleUniverseSha256,
                        visibleUniverseSha256,
                        StringComparison.Ordinal))
                {
                    ledger.ActivePlan = null;
                    return false;
                }

                if (state.CandidateVisibleOrdinals.Length != bindings.Count ||
                    state.ExpectedCurrentSelection.Length != bindings.Count)
                {
                    ledger.ActivePlan = null;
                    return false;
                }
                for (int index = 0; index < bindings.Count; index++)
                {
                    bool desired = ((state.DesiredMask >> index) & 1UL) != 0;
                    if (state.CandidateVisibleOrdinals[index] != bindings[index].BlockerVisibleOrdinal ||
                        state.ExpectedCurrentSelection[index] != bindings[index].CurrentlyBlocking ||
                        (bindings[index].CurrentlyBlocking && !desired))
                    {
                        ledger.ActivePlan = null;
                        return false;
                    }
                }

                int nextAddIndex = -1;
                for (int index = 0; index < bindings.Count; index++)
                {
                    bool desired = ((state.DesiredMask >> index) & 1UL) != 0;
                    if (desired && !bindings[index].CurrentlyBlocking)
                    {
                        if (bindings[index].BlockAction == null)
                        {
                            ledger.ActivePlan = null;
                            return false;
                        }
                        nextAddIndex = index;
                        break;
                    }
                }
                object selectedAction = nextAddIndex >= 0
                    ? bindings[nextAddIndex].BlockAction!
                    : doneAction;
                if (!TryResolveExactGameActionExecutorV1(
                        viewModel,
                        selectedAction,
                        out object? resolvedGame,
                        out MethodInfo? execute) ||
                    resolvedGame == null || execute == null ||
                    !ReferenceEquals(game, resolvedGame))
                {
                    ledger.ActivePlan = null;
                    return false;
                }

                if (nextAddIndex >= 0)
                {
                    state.ExpectedCurrentSelection[nextAddIndex] = true;
                }
                else
                {
                    if (ledger.CompletedVisibleUniverseHashes.Count >= 4096 ||
                        !ledger.CompletedVisibleUniverseHashes.Add(visibleUniverseSha256))
                    {
                        ledger.ActivePlan = null;
                        return false;
                    }
                    ledger.ActivePlan = null;
                }
                execute.Invoke(game, new[] { selectedAction });
                return true;
            }
        }

        private static bool TryExecuteSealedVisibleBlockerStepV1(
            FrameworkElement duelRoot,
            object viewModel,
            byte[] currentSanitizedSelection,
            List<SealedVisibleBlockerBindingV1>? blockerBindings,
            List<SealedVisibleBlockerAssignmentV1>? visibleAssignments,
            object? doneAction,
            object? targetingBlockerCard,
            uint targetingBlockerVisibleOrdinal,
            List<SealedVisibleBlockerTargetBindingV1>? targetBindings,
            uint turn,
            string visibleUniverseSha256,
            SealedVisibleActionDispatchRequestV1 request)
        {
            string currentSha256 = LowerSha256V1(currentSanitizedSelection);
            if (request.Kind != SealedVisibleDispatchKindV1.BlockerStep ||
                !string.Equals(currentSha256, request.ExpectedDecisionSha256, StringComparison.Ordinal) ||
                !IsLowerSha256V1(visibleUniverseSha256) ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Game",
                    out object? game) || game == null)
            {
                return false;
            }

            SealedVisibleBlockerPlanLedgerV1 ledger = VisibleBlockerPlanLedgerV1;
            lock (ledger)
            {
                if (request.BlockerOperationKind != 't' && ledger.ActivePlan != null)
                {
                    SealedVisibleBlockerPlanStateV1 pending = ledger.ActivePlan;
                    if (pending.Turn != turn ||
                        !string.Equals(
                            pending.VisibleUniverseSha256,
                            visibleUniverseSha256,
                            StringComparison.Ordinal))
                    {
                        // A visibly different game/turn/universe retires the
                        // stale transaction but never authorizes this call.
                        ledger.ActivePlan = null;
                        return false;
                    }
                    if (pending.Phase != SealedVisibleBlockerPlanPhaseV1.AwaitingVisibleAssignment ||
                        blockerBindings == null || visibleAssignments == null ||
                        !visibleAssignments.Any(assignment =>
                            assignment.AttackerVisibleOrdinal == pending.AttackerVisibleOrdinal &&
                            assignment.OrderedBlockerVisibleOrdinals.Count(
                                blocker => blocker == pending.BlockerVisibleOrdinal) == 1))
                    {
                        return false;
                    }
                    ledger.ActivePlan = null;
                }

                if (!TryValidateAndConsumeVisibleBlockerStepCommitmentsV1(
                        ledger,
                        currentSha256,
                        request))
                {
                    return false;
                }

                if (request.BlockerOperationKind == 'f')
                {
                    if (ledger.ActivePlan != null || blockerBindings == null ||
                        doneAction == null || request.SelectedIndex != 0 ||
                        !TryResolveExactGameActionExecutorV1(
                            viewModel,
                            doneAction,
                            out object? resolvedGame,
                            out MethodInfo? execute) ||
                        resolvedGame == null || execute == null ||
                        !ReferenceEquals(game, resolvedGame) ||
                        ledger.CompletedVisibleUniverseHashes.Count >= 4096 ||
                        !ledger.CompletedVisibleUniverseHashes.Add(visibleUniverseSha256))
                    {
                        return false;
                    }
                    execute.Invoke(game, new[] { doneAction });
                    return true;
                }

                if (request.BlockerOperationKind == 'b')
                {
                    if (ledger.ActivePlan != null || blockerBindings == null ||
                        request.SelectedIndex <= 0 || request.SelectedIndex > blockerBindings.Count)
                    {
                        return false;
                    }
                    SealedVisibleBlockerBindingV1 binding =
                        blockerBindings[request.SelectedIndex - 1];
                    if (binding.BlockerVisibleOrdinal != request.BlockerVisibleOrdinal ||
                        !TryResolveExactGameActionExecutorV1(
                            viewModel,
                            binding.BlockAction,
                            out object? resolvedGame,
                            out MethodInfo? execute) ||
                        resolvedGame == null || execute == null ||
                        !ReferenceEquals(game, resolvedGame) ||
                        ledger.CompletedVisibleUniverseHashes.Contains(visibleUniverseSha256))
                    {
                        return false;
                    }
                    ledger.ActivePlan = new SealedVisibleBlockerPlanStateV1
                    {
                        Phase = SealedVisibleBlockerPlanPhaseV1.AwaitingTargetPrompt,
                        BlockerVisibleOrdinal = binding.BlockerVisibleOrdinal,
                        Turn = turn,
                        VisibleUniverseSha256 = visibleUniverseSha256
                    };
                    execute.Invoke(game, new[] { binding.BlockAction });
                    return true;
                }

                if (request.BlockerOperationKind != 't' ||
                    ledger.ActivePlan == null || targetBindings == null ||
                    targetingBlockerCard == null || request.SelectedIndex < 0 ||
                    request.SelectedIndex >= targetBindings.Count)
                {
                    return false;
                }
                SealedVisibleBlockerPlanStateV1 state = ledger.ActivePlan;
                SealedVisibleBlockerTargetBindingV1 target =
                    targetBindings[request.SelectedIndex];
                if (state.Turn != turn ||
                    !string.Equals(state.VisibleUniverseSha256, visibleUniverseSha256, StringComparison.Ordinal))
                {
                    ledger.ActivePlan = null;
                    return false;
                }
                if (state.Phase != SealedVisibleBlockerPlanPhaseV1.AwaitingTargetPrompt ||
                    state.BlockerVisibleOrdinal != targetingBlockerVisibleOrdinal ||
                    state.BlockerVisibleOrdinal != request.BlockerVisibleOrdinal ||
                    target.AttackerVisibleOrdinal != request.AttackerVisibleOrdinal ||
                    !TryRequireExactCurrentVisibleBlockActionSourceV1(
                        viewModel,
                        targetingBlockerCard) ||
                    !TryResolveExactVisibleCardClickV1(
                        duelRoot,
                        viewModel,
                        target.AttackerCard,
                        out object? cardView,
                        out MethodInfo? leftClick) ||
                    cardView == null || leftClick == null)
                {
                    return false;
                }
                state.Phase = SealedVisibleBlockerPlanPhaseV1.AwaitingVisibleAssignment;
                state.AttackerVisibleOrdinal = target.AttackerVisibleOrdinal;
                leftClick.Invoke(viewModel, new[] { cardView, cardView });
                return true;
            }
        }

        private static bool TryRequireExactCurrentVisibleBlockActionSourceV1(
            object viewModel,
            object blockerCard)
        {
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    blockerCard,
                    "DuelScene",
                    "Shiny.Play.Duel.ViewModel.DuelSceneCardViewModel",
                    "Actions",
                    out object? actionsValue) ||
                !TryBoundedCollectionV1(actionsValue, 64, out List<object> actions) ||
                !TryReadExactPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "InteractionState",
                    out object? interactionState) || interactionState == null ||
                !TryReadExactPrivateVisibleActionPropertyV1(
                    interactionState,
                    "DuelScene",
                    "Shiny.Play.Duel.Utility.InteractionState",
                    "Source",
                    out object? source) || source == null)
            {
                return false;
            }
            var matches = new List<object>();
            foreach (object action in actions)
            {
                if (!TryReadExactPrivateVisibleActionPropertyV1(
                        action,
                        "WotC.MtGO.Client.Model.Reference",
                        "WotC.MtGO.Client.Model.Play.IGameAction",
                        "Name",
                        out object? nameValue) || !(nameValue is string name) ||
                    !IsBoundedVisibleStringV1(name, 512, true))
                {
                    return false;
                }
                if (string.Equals(name, "Block", StringComparison.Ordinal))
                {
                    if (!TryRequireSimpleVisibleMultiAttackerBlockerActionV1(action))
                    {
                        return false;
                    }
                    matches.Add(action);
                }
            }
            return matches.Count == 1 && ReferenceEquals(matches[0], source);
        }

        private static bool TryValidateAndConsumeVisibleBlockerStepCommitmentsV1(
            SealedVisibleBlockerPlanLedgerV1 ledger,
            string sourceSelectionSha256,
            SealedVisibleActionDispatchRequestV1 request)
        {
            string blocker = request.BlockerOperationKind == 'f'
                ? "-"
                : request.BlockerVisibleOrdinal.ToString(CultureInfo.InvariantCulture);
            string attacker = request.BlockerOperationKind == 't'
                ? request.AttackerVisibleOrdinal.ToString(CultureInfo.InvariantCulture)
                : "-";
            string expected = BlockerStepCommitmentSha256V1(
                sourceSelectionSha256,
                request.SelectedIndex,
                request.BlockerOperationKind.ToString(),
                blocker,
                attacker,
                request.ModelSelectionCommitmentSha256);
            if (!string.Equals(expected, request.BlockerStepCommitmentSha256, StringComparison.Ordinal) ||
                ledger.UsedSourceSelectionHashes.Count >= 4096 ||
                ledger.UsedModelSelectionCommitments.Count >= 4096 ||
                ledger.UsedStepCommitments.Count >= 4096 ||
                !ledger.UsedSourceSelectionHashes.Add(sourceSelectionSha256) ||
                !ledger.UsedModelSelectionCommitments.Add(request.ModelSelectionCommitmentSha256) ||
                !ledger.UsedStepCommitments.Add(request.BlockerStepCommitmentSha256))
            {
                return false;
            }
            return true;
        }

        private static bool TryResolveExactVisibleCardClickV1(
            FrameworkElement duelRoot,
            object viewModel,
            object targetCard,
            out object? cardView,
            out MethodInfo? leftClick)
        {
            cardView = null;
            leftClick = null;
            var matches = new List<FrameworkElement>(2);
            int visited = 0;
            if (!CollectExactVisibleCardViewsForDataContextV1(
                    duelRoot,
                    targetCard,
                    0,
                    ref visited,
                    matches) || matches.Count != 1)
            {
                return false;
            }
            FrameworkElement match = matches[0];
            Type viewModelType = viewModel.GetType();
            MethodInfo[] methods = viewModelType.GetMethods(
                BindingFlags.Instance | BindingFlags.NonPublic);
            MethodInfo? method = methods.SingleOrDefault(candidate =>
            {
                ParameterInfo[] parameters = candidate.GetParameters();
                return string.Equals(candidate.Name, "LeftClickDuringSelectTargets", StringComparison.Ordinal) &&
                    candidate.ReturnType == typeof(void) && parameters.Length == 2 &&
                    parameters[0].ParameterType.FullName == "Shiny.Play.Duel.Interfaces.IInteractableItem" &&
                    parameters[1].ParameterType.FullName == "Shiny.Play.Duel.Card_View" &&
                    parameters[0].ParameterType.IsInstanceOfType(match) &&
                    parameters[1].ParameterType.IsInstanceOfType(match);
            });
            if (method == null)
            {
                return false;
            }
            cardView = match;
            leftClick = method;
            return true;
        }

        private static bool CollectExactVisibleCardViewsForDataContextV1(
            DependencyObject node,
            object targetCard,
            int depth,
            ref int visited,
            List<FrameworkElement> matches)
        {
            if (depth > MaximumVisualDepth || visited >= MaximumVisualNodes)
            {
                return false;
            }
            visited++;
            if (node is FrameworkElement element &&
                element.GetType().FullName == "Shiny.Play.Duel.Card_View" &&
                ReferenceEquals(element.DataContext, targetCard))
            {
                if (!element.IsLoaded || !element.IsVisible ||
                    element.ActualWidth <= 0 || element.ActualHeight <= 0)
                {
                    return false;
                }
                matches.Add(element);
                if (matches.Count > 1)
                {
                    return false;
                }
            }
            int childCount = VisualTreeHelper.GetChildrenCount(node);
            for (int index = 0; index < childCount; index++)
            {
                DependencyObject child = VisualTreeHelper.GetChild(node, index);
                if (child == null || !CollectExactVisibleCardViewsForDataContextV1(
                        child,
                        targetCard,
                        depth + 1,
                        ref visited,
                        matches))
                {
                    return false;
                }
            }
            return true;
        }

        private static string BlockerStepCommitmentSha256V1(
            string sourceSelectionSha256,
            int selectedIndex,
            string operationKind,
            string blocker,
            string attacker,
            string modelSelectionCommitmentSha256)
        {
            using (SHA256 sha256 = SHA256.Create())
            {
                var committed = new List<byte>(384);
                committed.AddRange(Encoding.ASCII.GetBytes(BlockerStepCommitmentDomainV1));
                foreach (string part in new[]
                {
                    sourceSelectionSha256,
                    selectedIndex.ToString(CultureInfo.InvariantCulture),
                    operationKind,
                    blocker,
                    attacker,
                    modelSelectionCommitmentSha256
                })
                {
                    byte[] bytes = Encoding.ASCII.GetBytes(part);
                    var length = new byte[8];
                    ulong count = (ulong)bytes.Length;
                    for (int index = 7; index >= 0; index--)
                    {
                        length[index] = (byte)(count & 0xff);
                        count >>= 8;
                    }
                    committed.AddRange(length);
                    committed.AddRange(bytes);
                }
                return string.Concat(sha256.ComputeHash(committed.ToArray()).Select(
                    item => item.ToString("x2", CultureInfo.InvariantCulture)));
            }
        }

        private static string SingleBlockerPlanCommitmentSha256V1(
            string sourceSelectionSha256,
            int candidateCount,
            string desiredMaskHex)
        {
            using (SHA256 sha256 = SHA256.Create())
            {
                var committed = new List<byte>(256);
                committed.AddRange(Encoding.ASCII.GetBytes(
                    SingleBlockerPlanCommitmentDomainV1));
                foreach (string part in new[]
                {
                    sourceSelectionSha256,
                    candidateCount.ToString(CultureInfo.InvariantCulture),
                    desiredMaskHex
                })
                {
                    byte[] bytes = Encoding.ASCII.GetBytes(part);
                    var length = new byte[8];
                    ulong count = (ulong)bytes.Length;
                    for (int index = 7; index >= 0; index--)
                    {
                        length[index] = (byte)(count & 0xff);
                        count >>= 8;
                    }
                    committed.AddRange(length);
                    committed.AddRange(bytes);
                }
                return string.Concat(sha256.ComputeHash(committed.ToArray()).Select(
                    item => item.ToString("x2", CultureInfo.InvariantCulture)));
            }
        }

        private static bool TryResolveExactGameActionExecutorV1(
            object viewModel,
            object action,
            out object? game,
            out MethodInfo? execute)
        {
            game = null;
            execute = null;
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Game",
                    out object? gameValue) || gameValue == null)
            {
                return false;
            }
            const string gameInterfaceName = "WotC.MtGO.Client.Model.Play.IGame";
            const string actionInterfaceName = "WotC.MtGO.Client.Model.Play.IGameAction";
            Type? gameInterface = gameValue.GetType().GetInterface(gameInterfaceName, false);
            if (gameInterface == null ||
                !string.Equals(gameInterface.Assembly.GetName().Name, "WotC.MtGO.Client.Model.Reference", StringComparison.Ordinal))
            {
                return false;
            }
            Type? actionInterface = gameInterface.Assembly.GetType(actionInterfaceName, false, false);
            if (actionInterface == null || !actionInterface.IsInstanceOfType(action))
            {
                return false;
            }
            MethodInfo? method = gameInterface.GetMethods(
                    BindingFlags.Instance | BindingFlags.Public)
                .SingleOrDefault(candidate =>
                {
                    ParameterInfo[] parameters = candidate.GetParameters();
                    return string.Equals(candidate.Name, "ExecuteAction", StringComparison.Ordinal) &&
                        candidate.ReturnType == typeof(void) &&
                        parameters.Length == 1 &&
                        parameters[0].ParameterType == actionInterface;
                });
            if (method == null)
            {
                return false;
            }
            game = gameValue;
            execute = method;
            return true;
        }

        private static string AttackerPlanCommitmentSha256V1(
            string sourceSelectionSha256,
            int candidateCount,
            string desiredMaskHex)
        {
            using (SHA256 sha256 = SHA256.Create())
            {
                var committed = new List<byte>(192);
                committed.AddRange(Encoding.ASCII.GetBytes(AttackerPlanCommitmentDomainV1));
                foreach (string part in new[]
                {
                    sourceSelectionSha256,
                    candidateCount.ToString(CultureInfo.InvariantCulture),
                    desiredMaskHex
                })
                {
                    byte[] bytes = Encoding.ASCII.GetBytes(part);
                    var length = new byte[8];
                    ulong count = (ulong)bytes.Length;
                    for (int index = 7; index >= 0; index--)
                    {
                        length[index] = (byte)(count & 0xff);
                        count >>= 8;
                    }
                    committed.AddRange(length);
                    committed.AddRange(bytes);
                }
                return string.Concat(sha256.ComputeHash(committed.ToArray()).Select(
                    item => item.ToString("x2", CultureInfo.InvariantCulture)));
            }
        }

        private static bool IsLowerSha256V1(string value)
        {
            return value.Length == 64 && value.All(character =>
                (character >= '0' && character <= '9') ||
                (character >= 'a' && character <= 'f'));
        }

        private static string LowerSha256V1(byte[] value)
        {
            using (SHA256 sha256 = SHA256.Create())
            {
                return string.Concat(sha256.ComputeHash(value).Select(
                    item => item.ToString("x2", CultureInfo.InvariantCulture)));
            }
        }
    }
}
