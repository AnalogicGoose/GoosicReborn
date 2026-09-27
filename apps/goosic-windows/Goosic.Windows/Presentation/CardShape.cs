namespace Goosic.Windows.Presentation;

/// <summary>The sizes a shelf card is drawn at.</summary>
public static class CardShape
{
    /// <summary>A square card's side, the same as the <c>GoosicCardSize</c> resource.</summary>
    public const double Size = 196;

    /// <summary>A music video's card is the square card's height at 16:9, as YouTube Music shows it.</summary>
    public const double VideoWidth = Size * 16 / 9;
}
