using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO.MemoryMappedFiles;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text;

namespace MtgKernel.Mtgo.VisibleDuelProducer.V1
{
    public static partial class VisibleDuelProducerV1
    {
        private const int OrdinaryDispatchCommandSchemaV1 = 2;
        private const int AttackerDispatchCommandSchemaV1 = 3;
        private const int MaximumDispatchCommandBytesV1 = 256;
        private const string DispatchCommandPrefixV1 = "execute_visible_action_v1|";
        private const string AttackerDispatchCommandPrefixV1 =
            "execute_visible_attacker_step_v1|";
        private const string AttackerPlanCommitmentDomainV1 =
            "mtgo-visible-attacker-execution-plan-v1";
        private static readonly byte[] VisibleActionSubmitted = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}");
        private static readonly byte[] VisibleActionRejected = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}");
        private static readonly ConditionalWeakTable<object, HashSet<string>>
            DispatchedVisibleDecisionsByGameV1 =
                new ConditionalWeakTable<object, HashSet<string>>();
        private static readonly object VisibleAttackerPlanLedgerLockV1 = new object();
        private static readonly List<SealedVisibleAttackerPlanLedgerV1>
            VisibleAttackerPlanLedgersV1 = new List<SealedVisibleAttackerPlanLedgerV1>();

        internal enum SealedVisibleDispatchKindV1
        {
            OrdinaryAction,
            AttackerStep
        }

        internal sealed class SealedVisibleAttackerBindingV1
        {
            internal object Card = new object();
            internal uint VisibleOrdinal;
            internal bool CurrentlyAttacking;
            internal object ToggleAction = new object();
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
        }

        private sealed class SealedVisibleAttackerPlanStateV1
        {
            internal string PlanCommitmentSha256 = string.Empty;
            internal int CandidateCount;
            internal ulong DesiredMask;
            internal uint Turn;
            internal string VisibleUniverseSha256 = string.Empty;
            internal List<object> CandidateCards = new List<object>();
            internal bool[] ExpectedCurrentSelection = Array.Empty<bool>();
        }

        private sealed class SealedVisibleAttackerPlanLedgerV1
        {
            internal WeakReference Game = new WeakReference(new object());
            internal SealedVisibleAttackerPlanStateV1? ActivePlan;
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
                        : AttackerDispatchCommandSchemaV1;
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
                    else
                    {
                        if (parts.Length != 5 ||
                            !string.Equals(parts[0], AttackerDispatchCommandPrefixV1.TrimEnd('|'), StringComparison.Ordinal) ||
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
            HashSet<string> dispatched = DispatchedVisibleDecisionsByGameV1.GetValue(
                game,
                _ => new HashSet<string>(StringComparer.Ordinal));
            // One exact visible decision authorizes at most one client call,
            // regardless of which index a later request supplies. This is a
            // deliberately fail-closed v1 key: a byte-identical state later in
            // the same game remains unavailable until the broker owns a
            // private observation-generation token.
            if (dispatched.Count >= 4096 ||
                !dispatched.Add(request.ExpectedDecisionSha256))
            {
                return false;
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

            SealedVisibleAttackerPlanLedgerV1 ledger =
                GetVisibleAttackerPlanLedgerV1(game);
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
                        CandidateCards = bindings.Select(binding => binding.Card).ToList(),
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

                if (state.CandidateCards.Count != bindings.Count ||
                    state.ExpectedCurrentSelection.Length != bindings.Count)
                {
                    return false;
                }
                for (int index = 0; index < bindings.Count; index++)
                {
                    if (!ReferenceEquals(state.CandidateCards[index], bindings[index].Card) ||
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

        private static SealedVisibleAttackerPlanLedgerV1 GetVisibleAttackerPlanLedgerV1(
            object game)
        {
            lock (VisibleAttackerPlanLedgerLockV1)
            {
                for (int index = VisibleAttackerPlanLedgersV1.Count - 1; index >= 0; index--)
                {
                    object? target = VisibleAttackerPlanLedgersV1[index].Game.Target;
                    if (target == null)
                    {
                        VisibleAttackerPlanLedgersV1.RemoveAt(index);
                    }
                    else if (ReferenceEquals(target, game))
                    {
                        return VisibleAttackerPlanLedgersV1[index];
                    }
                }
                var ledger = new SealedVisibleAttackerPlanLedgerV1
                {
                    Game = new WeakReference(game)
                };
                VisibleAttackerPlanLedgersV1.Add(ledger);
                return ledger;
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
