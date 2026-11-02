namespace Goosic.Windows.Presentation;

/// <summary>Logical-pixel offsets and edge timing from the uncropped navigation reference.</summary>
public static class NavigationMotionRules
{
    public const float HeadingOffset = 22;
    public const float ItemOffset = 44;
    public const int ArrivalMilliseconds = 240;
    public const float IndicatorHeight = 16;
    public const int LeadingMilliseconds = 80;
    public const int TrailingMilliseconds = 280;
    public const int TrailingDelayMilliseconds = 20;

    public static bool TopEdgeLeads(float previousTop, float targetTop) => targetTop < previousTop;
}
