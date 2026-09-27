using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Goosic.Windows.Views;

/// <summary>
/// One row of the Settings page, laid out the way iOS Settings lays one out: a coloured icon
/// tile, the setting's name with a line saying what it does, and its control on the right.
/// </summary>
/// <remarks>
/// The control is the row's content, so a switch, a list or a button sits in the same place on
/// every row, and the explanation that used to change with the switch's state stays put under the
/// name where it can be read before choosing. Its look is the implicit style for it in Controls.xaml.
/// </remarks>
public sealed partial class SettingsRow : ContentControl
{
    public static readonly DependencyProperty GlyphProperty =
        DependencyProperty.Register(nameof(Glyph), typeof(string), typeof(SettingsRow), new PropertyMetadata(""));

    public static readonly DependencyProperty TileProperty =
        DependencyProperty.Register(nameof(Tile), typeof(Brush), typeof(SettingsRow), new PropertyMetadata(null));

    public static readonly DependencyProperty TitleProperty =
        DependencyProperty.Register(nameof(Title), typeof(string), typeof(SettingsRow), new PropertyMetadata(""));

    public static readonly DependencyProperty DescriptionProperty =
        DependencyProperty.Register(nameof(Description), typeof(string), typeof(SettingsRow),
            new PropertyMetadata("", (row, _) => ((SettingsRow)row).ShowDescription()));

    private TextBlock? _description;

    protected override void OnApplyTemplate()
    {
        base.OnApplyTemplate();
        _description = GetTemplateChild("DescriptionText") as TextBlock;
        ShowDescription();
    }

    /// <summary>An empty line would still take its height and push the name off centre.</summary>
    private void ShowDescription()
    {
        if (_description is not null)
        {
            _description.Visibility = string.IsNullOrEmpty(Description) ? Visibility.Collapsed : Visibility.Visible;
        }
    }

    /// <summary>A Segoe Fluent Icons code point drawn white on the tile.</summary>
    public string Glyph { get => (string)GetValue(GlyphProperty); set => SetValue(GlyphProperty, value); }

    /// <summary>The tile's colour, which is what the eye finds a setting by.</summary>
    public Brush? Tile { get => (Brush?)GetValue(TileProperty); set => SetValue(TileProperty, value); }

    public string Title { get => (string)GetValue(TitleProperty); set => SetValue(TitleProperty, value); }

    /// <summary>What the setting does, in one short line. Empty hides the line.</summary>
    public string Description { get => (string)GetValue(DescriptionProperty); set => SetValue(DescriptionProperty, value); }
}
