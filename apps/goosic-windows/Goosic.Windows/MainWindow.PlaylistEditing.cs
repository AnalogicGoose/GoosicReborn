using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Goosic.Windows;

public sealed partial class MainWindow : Window
{
    // ---- Editing an owned playlist -------------------------------------------------------------

    private void OnEditPagePlaylistSongs(object sender, RoutedEventArgs e) => Model.BeginPlaylistEdit();

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
