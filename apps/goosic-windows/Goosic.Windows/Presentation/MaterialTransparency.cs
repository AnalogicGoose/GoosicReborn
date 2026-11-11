using System;

namespace Goosic.Windows.Presentation;

/// <summary>
/// How see-through the sidebar, the floating bars and the menus are.
/// </summary>
/// <remarks>
/// The listener picks a transparency from 0 (solid) to 100 (as clear as the material allows).
/// The midpoint is the material as designed, so moving the slider to 50 gives back exactly the
/// tints in Palette.xaml and nobody's window changes until they ask for it. Below the midpoint each
/// brush's tint closes toward fully opaque; above it, the tint opens toward a floor that still
/// leaves some tint, because a sidebar with none would put its labels straight over album art.
/// The rule maps any brush's designed opacity, so the three brushes keep their relationship to one
/// another at every setting.
/// </remarks>
public static class MaterialTransparency
{
    public const int Minimum = 0;
    public const int Maximum = 100;

    /// <summary>The material as designed.</summary>
    public const int Default = 50;

    /// <summary>At full transparency a tint keeps this fraction of its designed opacity.</summary>
    public const double ClearestFraction = 0.15;

    /// <summary>A stored value brought back into range; anything unreadable is the default.</summary>
    public static int Sanitize(int? stored) => stored is { } value ? Math.Clamp(value, Minimum, Maximum) : Default;

    /// <summary>The opacity a tint designed at <paramref name="designed"/> takes at this transparency.</summary>
    public static double Opacity(double designed, int transparency)
    {
        designed = Math.Clamp(designed, 0, 1);
        var t = Sanitize(transparency);
        if (t <= Default)
        {
            // Toward solid: 0 is fully opaque, 50 is the designed tint.
            return designed + (1 - designed) * (Default - t) / (double)Default;
        }

        var clearest = designed * ClearestFraction;
        return designed - (designed - clearest) * (t - Default) / (double)(Maximum - Default);
    }

    /// <summary>The slider's spoken and written value.</summary>
    public static string Label(int transparency) => Sanitize(transparency) switch
    {
        Minimum => "Solid",
        Default => "Default",
        var t => $"{t}%",
    };
}
