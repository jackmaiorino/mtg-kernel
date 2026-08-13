using System;
using System.IO.MemoryMappedFiles;
using System.Text;
using System.Windows;
using MtgKernel.Mtgo.VisibleDuelProducer.V1;
using Shiny.Play.Duel;
using Shiny.Play.Duel.ViewModel;

namespace MtgKernel.Mtgo.VisibleChromeFixtureHost.V1
{
    internal static class Program
    {
        private const int Capacity = 1048576;

        [STAThread]
        private static int Main()
        {
            string channelName = "Local\\mtgkernel_mtgo_visible_v1_" + new string('b', 64);
            using (var channel = MemoryMappedFile.CreateNew(channelName, Capacity))
            using (var view = channel.CreateViewAccessor(
                0,
                Capacity,
                MemoryMappedFileAccess.ReadWrite))
            {
                var application = new Application();
                var viewModel = new DuelSceneViewModel();
                var seated = new PlayerViewModel
                {
                    IsLocalFixture = true,
                    IsActiveFixture = true,
                    IsPriorityFixture = true
                };
                seated.ManaItems.Add(new ManaPoolItemViewModel());
                var opponent = new PlayerViewModel();
                opponent.ManaItems.Add(new ManaPoolItemViewModel());
                viewModel.PlayerItems.Add(seated);
                viewModel.PlayerItems.Add(opponent);
                viewModel.Prompt.Buttons.Add(new OptionButton());

                var root = new DuelScene
                {
                    DataContext = viewModel,
                    Width = 640,
                    Height = 480
                };
                var window = new Window
                {
                    Content = root,
                    Width = 640,
                    Height = 480,
                    Left = -10000,
                    Top = -10000,
                    ShowInTaskbar = false,
                    WindowStyle = WindowStyle.None
                };
                window.Show();
                try
                {
                    int status = VisibleDuelProducerV1.ExportVisibleDecisionOrAbstainV1(
                        channelName);
                    int length = view.ReadInt32(0);
                    int schema = view.ReadInt32(4);
                    if (status != 0 || schema != 1 || length <= 0 || length > 128)
                    {
                        return 2;
                    }
                    byte[] bytes = new byte[length];
                    view.ReadArray(8, bytes, 0, bytes.Length);
                    string observed = Encoding.UTF8.GetString(bytes);
                    const string expected =
                        "{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}";
                    if (!string.Equals(observed, expected, StringComparison.Ordinal) ||
                        !VisibleGetterProbeV1.SawEveryVisibleChromeGetterV1())
                    {
                        return 3;
                    }
                    return 0;
                }
                finally
                {
                    window.Close();
                    application.Shutdown();
                }
            }
        }
    }
}
