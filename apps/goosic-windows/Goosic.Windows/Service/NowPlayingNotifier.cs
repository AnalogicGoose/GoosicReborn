using System;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;

namespace Goosic.Windows.Service;

/// <summary>A quiet Windows notification naming the song that just started.</summary>
/// <remarks>
/// Each notification replaces the last one rather than stacking, plays no sound, and is sent only
/// while Goosic is not the window in front: someone looking at the player already knows.
/// </remarks>
internal sealed class NowPlayingNotifier : IDisposable
{
    private const string Tag = "now-playing";
    private const string Group = "goosic";
    private bool _registered;

    /// <summary>Raised on the UI thread's dispatcher when the notification is clicked.</summary>
    internal event Action? Clicked;

    internal NowPlayingNotifier()
    {
        try
        {
            AppNotificationManager.Default.NotificationInvoked += (_, _) => Clicked?.Invoke();
            AppNotificationManager.Default.Register();
            _registered = true;
        }
        catch (Exception error)
        {
            BridgeLog.Write($"notifications unavailable: {error.GetType().Name}: {error.Message}");
        }
    }

    internal void Show(string title, string subtitle, string? artworkFile)
    {
        if (!_registered)
        {
            return;
        }

        try
        {
            var builder = new AppNotificationBuilder()
                .AddText(title)
                .AddText(subtitle)
                .MuteAudio()
                .SetTag(Tag)
                .SetGroup(Group);
            if (artworkFile is not null)
            {
                builder.SetAppLogoOverride(new Uri(artworkFile), AppNotificationImageCrop.Default);
            }

            var notification = builder.BuildNotification();
            notification.ExpiresOnReboot = true;
            AppNotificationManager.Default.Show(notification);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"now-playing notification failed: {error.GetType().Name}: {error.Message}");
        }
    }

    public void Dispose()
    {
        if (_registered)
        {
            _registered = false;
            try
            {
                AppNotificationManager.Default.Unregister();
            }
            catch (Exception error)
            {
                BridgeLog.Write($"notifications not unregistered: {error.GetType().Name}");
            }
        }
    }
}
