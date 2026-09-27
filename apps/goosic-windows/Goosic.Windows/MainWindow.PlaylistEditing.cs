using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Editing an owned playlist -------------------------------------------------------------

    private void OnEditPagePlaylistSongs(object sender, RoutedEventArgs e) => Model.BeginPlaylistEdit();

    /// <summary>Fills the Sort menu each time, so the check follows the order on screen.</summary>
    private void OnSortMenuOpening(object? sender, object e)
    {
        if (sender is not MenuFlyout menu)
        {
            return;
        }

        menu.Items.Clear();
        foreach (var choice in Model.TrackSortChoices)
        {
            var item = new RadioMenuFlyoutItem
            {
                Text = choice.Label,
                GroupName = "sort",
                IsChecked = Model.TrackSortLabel.EndsWith(choice.Label, System.StringComparison.Ordinal),
            };
            item.Click += (_, _) => Model.SortTracks(choice.Key);
            menu.Items.Add(item);
        }
    }

    private void OnDonePlaylistEdit(object sender, RoutedEventArgs e) => Model.EndPlaylistEdit();

    private void OnSelectAllPlaylistSongs(object sender, RoutedEventArgs e) => Model.SelectAllForEdit(true);

    private void OnClearPlaylistSelection(object sender, RoutedEventArgs e) => Model.SelectAllForEdit(false);

    private async void OnMovePlaylistSongs(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string direction })
        {
            await Model.MoveSelectedAsync(up: direction == "up");
        }
    }

    private async void OnRemovePlaylistSongs(object sender, RoutedEventArgs e) => await Model.RemoveSelectedAsync();
}
