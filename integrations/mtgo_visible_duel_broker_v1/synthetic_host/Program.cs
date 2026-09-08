using System;
using System.Threading;

namespace MtgKernel.Mtgo.SyntheticManagedHost.V1
{
    internal static class Program
    {
        private static int Main()
        {
            Thread.Sleep(TimeSpan.FromMinutes(2));
            return 0;
        }
    }
}
