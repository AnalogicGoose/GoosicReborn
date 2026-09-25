using System;
using System.IO;
using Microsoft.Win32;

namespace Goosic.Windows.Service;

/// <summary>Starts Goosic when the user signs in to Windows, through the per-user Run key.</summary>
/// <remarks>
/// The Run key is the source of truth rather than a saved preference, so turning it off in Task
/// Manager's Startup apps is reflected here. It is offered only to a copy Setup installed: a
/// development or portable build moves, and a Run entry pointing at a folder that is gone is
/// the kind of leftover nobody can find.
/// </remarks>
internal static class StartupRegistration
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "Goosic";

    /// <summary>The argument that tells a launch it came from sign-in, not from the user.</summary>
    internal const string BackgroundArgument = "--background";

    internal static bool Available => File.Exists(Path.Combine(AppContext.BaseDirectory, "unins000.exe"));

    private static string Command => $"\"{Environment.ProcessPath}\" {BackgroundArgument}";

    internal static bool Enabled
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKey);
            return key?.GetValue(ValueName) is string value
                && value.Equals(Command, StringComparison.OrdinalIgnoreCase);
        }
        set
        {
            using var key = Registry.CurrentUser.CreateSubKey(RunKey);
            if (value)
            {
                key.SetValue(ValueName, Command);
            }
            else
            {
                key.DeleteValue(ValueName, throwOnMissingValue: false);
            }
        }
    }

    /// <summary>Whether this process was started at sign-in.</summary>
    internal static bool LaunchedInBackground =>
        Array.Exists(Environment.GetCommandLineArgs(), argument => argument == BackgroundArgument);
}
