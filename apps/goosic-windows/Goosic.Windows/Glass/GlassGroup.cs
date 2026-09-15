using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows.Glass;

/// <summary>Groups related glass controls without exposing renderer details.</summary>
public sealed class GlassGroup : ContentControl
{
    public GlassGroup()
    {
        DefaultStyleKey = typeof(ContentControl);
    }
}
