using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Goosic.Windows.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;

namespace Goosic.Windows;

/// <summary>
/// The listener's own sidebar: destinations they hid, and things they pinned to the top.
/// </summary>
/// <remarks>
/// One rule, <see cref="SidebarCustomization"/>, decides both the expanded list and the rail, so
/// a hidden destination leaves both and a pin appears in both, in the same order.
/// </remarks>
public sealed partial class MainWindow : Window
{
    private readonly ObservableCollection<SidebarPinItem> _pins = [];

    private void WireSidebarCustomization()
    {
        SidebarPinList.ItemsSource = _pins;
        SidebarRailPins.ItemsSource = _pins;
        ReloadPins();
        ApplyHiddenSidebarItems();
    }

    private void ReloadPins()
    {
        _pins.Clear();
        foreach (var pin in ShellPreferences.SidebarPins)
        {
            _pins.Add(new SidebarPinItem(pin));
        }

        var visible = _pins.Count > 0 ? Visibility.Visible : Visibility.Collapsed;
        SidebarPinsHeader.Visibility = visible;
        SidebarPinList.Visibility = visible;
    }

    private void SavePins(IReadOnlyList<SidebarPin> pins)
    {
        ShellPreferences.SidebarPins = pins;
        ReloadPins();
    }

    /// <summary>Shows or hides each destination, and each heading left with nothing under it.</summary>
    private void ApplyHiddenSidebarItems()
    {
        var hidden = ShellPreferences.HiddenSidebarItems.ToArray();
        foreach (var panel in new[] { SidebarItems, SidebarRailItems })
        {
            foreach (var child in panel.Children)
            {
                switch (child)
                {
                    case ToggleButton { Tag: string tag } item:
                        item.Visibility = SidebarCustomization.IsShown(tag, hidden) ? Visibility.Visible : Visibility.Collapsed;
                        break;
                    case TextBlock { Text: "Discover" or "Library" } heading:
                        heading.Visibility = SidebarCustomization.SectionHasItems(heading.Text, hidden)
                            ? Visibility.Visible
                            : Visibility.Collapsed;
                        break;
                }
            }
        }

        UpdateSidebarIndicator();
    }

    // ---- Pins -----------------------------------------------------------------------------

    internal bool IsPinned(string kind, string id) =>
        SidebarCustomization.IsPinned(ShellPreferences.SidebarPins, kind, id);

    private void TogglePin(SidebarPin pin)
    {
        var pins = ShellPreferences.SidebarPins;
        if (SidebarCustomization.IsPinned(pins, pin.Kind, pin.Id))
        {
            SavePins(SidebarCustomization.Unpin(pins, pin.Kind, pin.Id));
            return;
        }

        if (pins.Count >= SidebarCustomization.MaximumPins)
        {
            Model.ReportStatus($"The sidebar holds {SidebarCustomization.MaximumPins} pins. Unpin one to make room.");
            return;
        }

        SavePins(SidebarCustomization.Pin(pins, pin));
    }

    /// <summary>"Pin Playlist" or "Unpin Playlist", as Apple Music words it.</summary>
    private void AddPinItem(MenuFlyout menu, SidebarPin pin)
    {
        var noun = pin.Kind switch
        {
            "album" => "Album",
            "artist" => "Artist",
            "playlist" => "Playlist",
            _ => "Song",
        };
        var pinned = IsPinned(pin.Kind, pin.Id);
        Add(menu, pinned ? $"Unpin {noun}" : $"Pin {noun}", pinned ? "\uE77A" : "\uE718", () => TogglePin(pin));
    }

    private async void OpenPin(SidebarPin pin)
    {
        DismissOverlaySidebar();
        if (pin.IsSong)
        {
            var card = new CardViewModel(new CatalogItem
            {
                Kind = pin.Kind,
                Id = pin.Id,
                VideoId = pin.Id,
                Title = pin.Title,
                Subtitle = pin.Subtitle,
                Thumbnail = pin.Thumbnail,
            });
            await PlayEntryAsync(Model.PlayFromShelf(card));
            return;
        }

        Model.RememberEntityThumbnail(pin.Kind, pin.Id, pin.Thumbnail);
        await OpenAsync(pin.Kind, pin.Id, pin.Title);
    }

    private void OnSidebarPinClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is SidebarPinItem item)
        {
            OpenPin(item.Pin);
        }
    }

    private void OnSidebarRailPin(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: SidebarPinItem item })
        {
            OpenPin(item.Pin);
        }
    }

    /// <summary>A drag in the sidebar reorders the collection itself; this keeps that order.</summary>
    private void OnSidebarPinsReordered(ListViewBase sender, DragItemsCompletedEventArgs args) =>
        ShellPreferences.SidebarPins = _pins.Select(item => item.Pin).ToArray();

    /// <summary>
    /// A pin's menu, by right-click or by the keyboard's menu key: open it, move it, or unpin it.
    /// Move Up and Move Down are the reorder for anyone not using a pointer.
    /// </summary>
    private void OnSidebarPinContextRequested(UIElement sender, ContextRequestedEventArgs args)
    {
        var item = (args.OriginalSource as ListViewItem)?.Content as SidebarPinItem
            ?? (args.OriginalSource as FrameworkElement)?.DataContext as SidebarPinItem;
        if (item is null)
        {
            return;
        }

        var index = _pins.IndexOf(item);
        var menu = new MenuFlyout();
        Add(menu, item.Pin.IsSong ? "Play" : "Open", item.Pin.IsSong ? "\uE768" : "\uE8A7", () => OpenPin(item.Pin));
        menu.Items.Add(new MenuFlyoutSeparator());
        var up = Add(menu, "Move Up", "\uE74A", () => MovePin(index, -1));
        up.IsEnabled = index > 0;
        var down = Add(menu, "Move Down", "\uE74B", () => MovePin(index, 1));
        down.IsEnabled = index < _pins.Count - 1;
        menu.Items.Add(new MenuFlyoutSeparator());
        AddPinItem(menu, item.Pin);

        args.Handled = true;
        var target = SidebarPinList.ContainerFromItem(item) as FrameworkElement ?? SidebarPinList;
        var options = new FlyoutShowOptions();
        if (args.TryGetPosition(target, out var point))
        {
            options.Position = point;
        }

        menu.ShowAt(target, options);
    }

    private void MovePin(int index, int offset)
    {
        SavePins(SidebarCustomization.Move(ShellPreferences.SidebarPins, index, offset));
        var moved = Math.Clamp(index + offset, 0, _pins.Count - 1);
        if (SidebarPinList.ContainerFromIndex(moved) is ListViewItem container)
        {
            container.Focus(FocusState.Keyboard);
        }
    }

    // ---- Edit Sidebar ---------------------------------------------------------------------

    /// <summary>
    /// The edit sheet: a switch for each destination that can be hidden, grouped under its heading,
    /// and the pins with buttons to move or remove each one. Changes apply as they are made.
    /// </summary>
    private async void OnEditSidebar(object sender, RoutedEventArgs e)
    {
        var content = new StackPanel { Spacing = 2, MinWidth = 340 };
        string? section = null;
        foreach (var (tag, title, heading) in SidebarCustomization.Hideable)
        {
            if (heading != section)
            {
                section = heading;
                content.Children.Add(new TextBlock
                {
                    Text = heading,
                    FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                    Margin = new Thickness(0, content.Children.Count == 0 ? 0 : 12, 0, 2),
                });
            }

            var box = new CheckBox
            {
                Content = title,
                IsChecked = SidebarCustomization.IsShown(tag, ShellPreferences.HiddenSidebarItems.ToArray()),
            };
            box.Checked += (_, _) => SetShown(tag, true);
            box.Unchecked += (_, _) => SetShown(tag, false);
            content.Children.Add(box);
        }

        content.Children.Add(new TextBlock
        {
            Text = "Search, Home and Settings always stay in the sidebar.",
            FontSize = 12,
            TextWrapping = TextWrapping.Wrap,
            Opacity = 0.7,
            Margin = new Thickness(0, 8, 0, 0),
        });

        var pinsPanel = new StackPanel { Spacing = 4 };
        content.Children.Add(new TextBlock
        {
            Text = "Pins",
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Margin = new Thickness(0, 16, 0, 2),
        });
        content.Children.Add(pinsPanel);
        FillPinEditor(pinsPanel);

        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = "Edit Sidebar",
            Content = new ScrollViewer { Content = content, MaxHeight = 460 },
            CloseButtonText = "Done",
            DefaultButton = ContentDialogButton.Close,
        };
        await dialog.ShowAsync();

        void SetShown(string tag, bool shown)
        {
            ShellPreferences.HiddenSidebarItems =
                SidebarCustomization.SetShown(ShellPreferences.HiddenSidebarItems, tag, shown);
            ApplyHiddenSidebarItems();
        }
    }

    private void FillPinEditor(StackPanel panel)
    {
        panel.Children.Clear();
        var pins = ShellPreferences.SidebarPins;
        if (pins.Count == 0)
        {
            panel.Children.Add(new TextBlock
            {
                Text = "Pin a playlist, album, artist or song from its menu to keep it at the top of the sidebar.",
                FontSize = 12,
                TextWrapping = TextWrapping.Wrap,
                Opacity = 0.7,
            });
            return;
        }

        for (var index = 0; index < pins.Count; index++)
        {
            var at = index;
            var pin = pins[index];
            var row = new Grid { ColumnSpacing = 4 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            row.Children.Add(new TextBlock
            {
                Text = pin.Title,
                VerticalAlignment = VerticalAlignment.Center,
                TextTrimming = TextTrimming.CharacterEllipsis,
            });
            AddButton(1, "\uE74A", $"Move {pin.Title} up", at > 0, () => SavePins(SidebarCustomization.Move(pins, at, -1)));
            AddButton(2, "\uE74B", $"Move {pin.Title} down", at < pins.Count - 1, () => SavePins(SidebarCustomization.Move(pins, at, 1)));
            AddButton(3, "\uE711", $"Unpin {pin.Title}", true, () => SavePins(SidebarCustomization.Unpin(pins, pin.Kind, pin.Id)));
            panel.Children.Add(row);

            void AddButton(int column, string glyph, string name, bool enabled, Action action)
            {
                var button = new Button
                {
                    Content = new FontIcon { Glyph = glyph, FontSize = 12 },
                    Padding = new Thickness(8, 6, 8, 6),
                    IsEnabled = enabled,
                };
                Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, name);
                ToolTipService.SetToolTip(button, name);
                button.Click += (_, _) =>
                {
                    action();
                    FillPinEditor(panel);
                };
                Grid.SetColumn(button, column);
                row.Children.Add(button);
            }
        }
    }
}
