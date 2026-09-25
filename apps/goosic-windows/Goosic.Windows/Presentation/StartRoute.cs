using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Presentation;

/// <summary>Which page Goosic opens on.</summary>
public static class StartRoute
{
    /// <summary>
    /// Pages worth reopening. Settings and Downloads are places you visit, not places to start,
    /// and a remembered route from a newer or older build that this one does not know is ignored.
    /// </summary>
    public static IReadOnlySet<string> Reopenable { get; } = new HashSet<string>
    {
        "home", "explore", "charts", "moodsAndGenres", "newReleases", "library", "liked", "history",
    };

    /// <summary>The route for a start-page preference: home, library, liked, or last.</summary>
    public static string For(string? startPage, string? lastRoute) => startPage switch
    {
        "library" => "library",
        "liked" => "liked",
        "last" when lastRoute is not null && Reopenable.Contains(lastRoute) => lastRoute,
        _ => "home",
    };

    /// <summary>Whether a page just shown should become the one "last page" reopens.</summary>
    public static bool IsRemembered(string route) => Reopenable.Contains(route);

    /// <summary>The choices the Settings page offers, in order, with their labels.</summary>
    public static IReadOnlyList<(string Value, string Label)> Choices { get; } =
    [
        ("home", "Home"),
        ("library", "Library"),
        ("liked", "Liked Music"),
        ("last", "The page I was on last"),
    ];

    public static int IndexOf(string startPage) =>
        Choices.Select((choice, index) => (choice, index)).FirstOrDefault(pair => pair.choice.Value == startPage).index;
}
