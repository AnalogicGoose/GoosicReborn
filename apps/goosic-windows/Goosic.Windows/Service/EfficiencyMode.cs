using System;
using System.Runtime.InteropServices;

namespace Goosic.Windows.Service;

/// <summary>
/// Windows' EcoQoS for Goosic's own process: the scheduler prefers efficient cores and lower
/// clocks, which is what Task Manager calls Efficiency mode.
/// </summary>
/// <remarks>
/// It throttles this process only. The audio is decoded in WebView2's own processes, which
/// Windows does not throttle along with it, so playback is not what slows down: the shell's
/// layout and drawing are.
/// </remarks>
internal static class EfficiencyMode
{
    private const int ProcessPowerThrottling = 4;
    private const uint ExecutionSpeed = 0x1;

    internal static void Apply(bool enabled)
    {
        var state = new PowerThrottlingState
        {
            Version = 1,
            ControlMask = ExecutionSpeed,
            StateMask = enabled ? ExecutionSpeed : 0,
        };
        if (!SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, ref state,
                (uint)Marshal.SizeOf<PowerThrottlingState>()))
        {
            BridgeLog.Write($"efficiency mode not applied ({Marshal.GetLastWin32Error()})");
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PowerThrottlingState
    {
        public uint Version, ControlMask, StateMask;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetProcessInformation(IntPtr process, int informationClass,
        ref PowerThrottlingState information, uint size);

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();
}
