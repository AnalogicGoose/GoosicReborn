namespace Goosic.Windows.Glass;

/// <summary>
/// The product vocabulary for glass. A view names the role a surface plays; the renderer decides
/// what that costs and how it looks, so no view ever carries a blur radius or a tint constant.
/// </summary>
public enum GlassStyle
{
    /// <summary>A barely-there layer over content, for cards that only need to separate.</summary>
    Thin,
    Regular,
    /// <summary>Panels that hold their own content, such as Lyrics and Playing Next.</summary>
    Prominent,
    /// <summary>Small controls: toggles, fields, a selected row, a hover capsule.</summary>
    Control,
    Navigation,
    Menu,
    Player,
    /// <summary>Window chrome: the title bar edge that frosts content scrolled beneath it.</summary>
    Window,
}

/// <summary>
/// How much of the optical model the machine is asked to run. Chosen once per environment by
/// <see cref="GlassQualityPolicy"/>, never by a view.
/// </summary>
public enum GlassQuality
{
    Ultra,
    High,
    Medium,
    Low,
    Fallback,
}

/// <summary>What the debug overlay shows in place of, or on top of, the finished material.</summary>
public enum GlassDebugLayer
{
    None,
    Bounds,
    ZOrder,
    Backdrop,
    Mask,
    BlurredStack,
    OverlapResponse,
}
