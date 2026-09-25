using System.Collections.Generic;

namespace Goosic.Windows.ViewModels;

/// <summary>One keyboard shortcut, as the Settings page lists it: what it does, then its keys.</summary>
public sealed record ShortcutEntry(string Action, string Keys)
{
    /// <summary>Every shortcut <c>ShellKeyboard</c> handles, in the order a listener looks for them.</summary>
    public static IReadOnlyList<ShortcutEntry> All { get; } =
    [
        new("Play or pause", "Space"),
        new("Previous / next song", "Ctrl+← / Ctrl+→"),
        new("Seek back / forward", "Shift+← / Shift+→"),
        new("Volume down / up", "Ctrl+↓ / Ctrl+↑"),
        new("Mute", "Ctrl+M"),
        new("Shuffle", "Ctrl+S"),
        new("Repeat", "Ctrl+R"),
        new("Search", "Ctrl+F"),
        new("Lyrics", "Ctrl+L"),
        new("Queue", "Ctrl+Q"),
        new("Show or hide the sidebar", "Ctrl+B"),
        new("Like the song playing", "Alt+Shift+B"),
        new("Settings", "Ctrl+,"),
        new("Full-screen player", "Ctrl+Shift+F"),
        new("Full screen", "F11"),
        new("Back", "Alt+←"),
        new("Close a panel or the full-screen player", "Esc"),
    ];
}
