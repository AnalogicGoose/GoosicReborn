using System;
using System.IO;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Goosic.Windows.Service;

/// <summary>Where the window was, so it reopens there.</summary>
internal sealed record WindowPlacement(int X, int Y, int Width, int Height, bool Maximized, bool SidebarHidden);

/// <summary>
/// Preferences about this Windows program rather than about listening.
/// </summary>
/// <remarks>
/// Listening preferences — what plays, what is hidden, where Goosic opens — are Rust's and shared
/// by every shell. These are not: a tray icon, a Run-key entry, a toast, a Discord pipe, window
/// geometry, and Windows' EcoQoS are how this program behaves on this operating system, and a
/// field for each in the shared store would be a promise every other shell had to answer. They
/// live beside the shell's other local state, in <c>%LOCALAPPDATA%\Goosic\windows-shell.json</c>.
/// </remarks>
internal static class ShellPreferences
{
    private static readonly string StorePath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Goosic", "windows-shell.json");

    private static readonly Lazy<JsonObject> Store = new(Load);

    /// <summary>Closing the window hides it to the notification area and keeps the music playing.</summary>
    internal static bool CloseToTray { get => Read("closeToTray", false); set => Write("closeToTray", value); }

    /// <summary>A small Windows notification when the song changes while Goosic is not in front.</summary>
    internal static bool NowPlayingNotifications { get => Read("nowPlayingNotifications", false); set => Write("nowPlayingNotifications", value); }

    /// <summary>Show the playing song on the listener's Discord profile.</summary>
    internal static bool DiscordStatus { get => Read("discordStatus", false); set => Write("discordStatus", value); }

    /// <summary>Reopen at the same size, position and sidebar state.</summary>
    internal static bool RememberWindow { get => Read("rememberWindow", true); set => Write("rememberWindow", value); }

    /// <summary>
    /// EcoQoS for this process and no decorative motion. WebView2's low-memory target is not part
    /// of it: Microsoft means it for an inactive WebView, and the player's is always playing.
    /// </summary>
    internal static bool EfficiencyMode { get => Read("efficiencyMode", false); set => Write("efficiencyMode", value); }

    internal static WindowPlacement? Placement
    {
        get
        {
            try
            {
                return Store.Value["window"]?.Deserialize<WindowPlacement>();
            }
            catch (JsonException)
            {
                return null;
            }
        }
        set
        {
            Store.Value["window"] = value is null ? null : JsonSerializer.SerializeToNode(value);
            Save();
        }
    }

    private static bool Read(string name, bool fallback)
    {
        try
        {
            return Store.Value[name]?.GetValue<bool>() ?? fallback;
        }
        catch (InvalidOperationException)
        {
            return fallback;
        }
    }

    private static void Write(string name, bool value)
    {
        Store.Value[name] = value;
        Save();
    }

    private static void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(StorePath)!);
            File.WriteAllText(StorePath, Store.Value.ToJsonString());
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            BridgeLog.Write($"shell preferences not saved: {error.Message}");
        }
    }

    private static JsonObject Load()
    {
        try
        {
            return File.Exists(StorePath) && JsonNode.Parse(File.ReadAllText(StorePath)) is JsonObject stored
                ? stored
                : [];
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or JsonException)
        {
            BridgeLog.Write($"shell preferences unreadable: {error.Message}");
            return [];
        }
    }
}
