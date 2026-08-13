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
        private const int DispatchCommandSchemaV1 = 2;
        private const int MaximumDispatchCommandBytesV1 = 128;
        private const string DispatchCommandPrefixV1 = "execute_visible_action_v1|";
        private static readonly byte[] VisibleActionSubmitted = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"submitted\"}");
        private static readonly byte[] VisibleActionRejected = Encoding.UTF8.GetBytes(
            "{\"result_kind\":\"action_dispatch_receipt\",\"status\":\"rejected\"}");
        private static readonly ConditionalWeakTable<object, HashSet<string>>
            DispatchedVisibleDecisionsByGameV1 =
                new ConditionalWeakTable<object, HashSet<string>>();

        internal sealed class SealedVisibleActionDispatchRequestV1
        {
            internal string ExpectedDecisionSha256 = string.Empty;
            internal int SelectedIndex;
        }

        private static bool TryReadSealedVisibleActionDispatchRequestV1(
            string channelName,
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
                    if (schema != DispatchCommandSchemaV1 || length <= 0 ||
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
                        ExpectedDecisionSha256 = parts[1],
                        SelectedIndex = index
                    };
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
            if (!TryReadExactPrivateVisibleActionPropertyV1(
                    viewModel,
                    "DuelScene",
                    DuelViewModelType,
                    "Game",
                    out object? game) || game == null)
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
            const string gameInterfaceName = "WotC.MtGO.Client.Model.Play.IGame";
            const string actionInterfaceName = "WotC.MtGO.Client.Model.Play.IGameAction";
            Type? gameInterface = game.GetType().GetInterface(gameInterfaceName, false);
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
            MethodInfo? execute = gameInterface.GetMethods(
                    BindingFlags.Instance | BindingFlags.Public)
                .SingleOrDefault(method =>
                {
                    ParameterInfo[] parameters = method.GetParameters();
                    return string.Equals(method.Name, "ExecuteAction", StringComparison.Ordinal) &&
                        method.ReturnType == typeof(void) &&
                        parameters.Length == 1 &&
                        parameters[0].ParameterType == actionInterface;
                });
            if (execute == null || execute.ReturnType != typeof(void))
            {
                return false;
            }
            // Consumption precedes the client call. If dispatch throws after
            // an unknown amount of work, the same visible decision cannot be
            // retried and accidentally send a second input.
            execute.Invoke(game, new[] { action });
            return true;
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
