using System;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;

namespace Goosic.Windows.Service;

/// <summary>What a finished sign-in hands back: metadata and identities, never a credential.</summary>
internal sealed record AccountLoginResult(string AccountId, string ProfileId, AccountLoginSummary Summary);

internal sealed record AccountLoginSummary
{
    [JsonPropertyName("displayName")] public string DisplayName { get; init; } = "";
    [JsonPropertyName("email")] public string? Email { get; init; }
    [JsonPropertyName("channel")] public string? Channel { get; init; }
    [JsonPropertyName("avatarUrl")] public string? AvatarUrl { get; init; }
}

/// <summary>
/// The sign-in window: Google's own pages, in a profile staged for the account being added.
/// </summary>
/// <remarks>
/// <para>
/// The Windows counterpart of <c>AccountLoginHost+macOS.swift</c>, and deliberately as narrow. The
/// password is typed into Google's page and stays in that page's profile; the shell never reads a
/// field, a cookie or a header. What it does read, once the page is on YouTube Music and says it is
/// signed in, is a small metadata object -- and whether that object completes the sign-in is
/// decided by <c>goosic-shell-support</c>, not here.
/// </para>
/// <para>
/// Both identities are generated before the window opens. The profile stays staged until the
/// caller has stored the account in Rust and switched to it; on any other ending its data is
/// cleared, so an abandoned sign-in leaves no signed-in jar behind.
/// </para>
/// </remarks>
internal sealed class AccountLoginWindow
{
    /// <summary>How long one page may sit before the sign-in is abandoned; reset by each navigation.</summary>
    private static readonly TimeSpan CompletionTimeout = TimeSpan.FromMinutes(3);

    /// <summary>
    /// The page-side marker, as an async expression.
    /// </summary>
    /// <remarks>
    /// This is the macOS script, which reads the page's own <c>LOGGED_IN</c> flag and opens the
    /// account menu to find a second identity field. The copy in <c>goosic-shell-support</c> is the
    /// older one that says it moves when the macOS branch lands; until then this mirrors the one
    /// that is known to finish a sign-in, and its output is still judged by the Rust rules.
    /// </remarks>
    private const string CompletionScript = """
        (async () => {
        if (location.origin !== 'https://music.youtube.com') return '';
        const cfg = window.ytcfg;
        const loggedIn = !!(cfg && typeof cfg.get === 'function' && cfg.get('LOGGED_IN'));
        if (!loggedIn) return '';
        const clip = (value, limit) => (value || '').trim().slice(0, limit);
        const text = (selector, limit) => clip(document.querySelector(selector)?.textContent, limit);
        const isAvatar = (src) => /^https:\/\/(?:[a-z0-9-]+\.)*(?:googleusercontent\.com|ggpht\.com)\//i.test(src || '');
        const avatarOf = (root) => Array.from((root || document).querySelectorAll('img')).map((img) => img.src).find(isAvatar) || '';
        const marker = document.querySelector('ytmusic-settings-button, #avatar-btn, [aria-label*="ccount menu" i]');
        const read = () => ({
          name: text('#account-name', 128),
          email: text('#email, #account-email', 320),
          channel: text('#channel-handle, #channel-title', 128),
        });
        let identity = read();
        let avatarUrl = avatarOf(marker) || avatarOf(document.querySelector('ytmusic-nav-bar'));
        if (!identity.email && !identity.name && marker) {
          try { (marker.querySelector('button, tp-yt-paper-icon-button, yt-icon-button, [role="button"]') || marker).click(); } catch (_) {}
          await new Promise((resolve) => setTimeout(resolve, 400));
          identity = read();
          avatarUrl = avatarUrl || avatarOf(document.querySelector('ytmusic-popup-container, tp-yt-iron-dropdown'));
        }
        if (!identity.email && !identity.channel && !avatarUrl) return '';
        return JSON.stringify({
          displayName: identity.name || identity.email || 'YouTube Music account',
          email: identity.email || null, channel: identity.channel || null, avatarUrl: avatarUrl || null
        });
        })()
        """;

    private readonly Window _window = new();
    private readonly WebView2 _view = new();
    private readonly TaskCompletionSource<AccountLoginResult?> _outcome = new();
    private readonly string _accountId = Guid.NewGuid().ToString("D");
    private readonly string _profileId;
    private CancellationTokenSource? _polling;
    private bool _finished;
    private bool _promoted;
    private bool _closing;

    internal AccountLoginWindow()
    {
        var profile = Guid.NewGuid().ToString("D");
        while (profile == _accountId)
        {
            profile = Guid.NewGuid().ToString("D");
        }

        _profileId = profile;
        _window.Title = "Sign in to YouTube Music";
        _window.Content = new Grid { Children = { _view } };
        _window.AppWindow.Resize(new global::Windows.Graphics.SizeInt32(760, 720));
        _window.AppWindow.Closing += async (_, args) =>
        {
            if (_closing)
            {
                return;
            }

            // Closing the window is cancelling the sign-in. The staged profile is cleared first,
            // while its surface still exists to clear it, so a half-finished sign-in cannot leave
            // a signed-in jar on disk.
            args.Cancel = true;
            Finish(null);
            await DiscardAsync();
            Close();
        };
    }

    /// <summary>Opens the window and waits for a completed sign-in, or null if it did not complete.</summary>
    internal async Task<AccountLoginResult?> RunAsync()
    {
        _window.Activate();
        try
        {
            var environment = await WebProfiles.EnvironmentAsync();
            await _view.EnsureCoreWebView2Async(environment, WebProfiles.OptionsFor(environment, WebProfiles.NameFor(_profileId)));
        }
        catch (Exception error)
        {
            BridgeLog.Write($"sign-in surface failed: {error.Message}");
            Finish(null);
            return await _outcome.Task;
        }

        var core = _view.CoreWebView2;

        // Login is a browser surface, not a playback bridge: no message port, no script dialogs
        // answered on the page's behalf, and no second window to lose track of.
        core.Settings.AreDevToolsEnabled = false;
        core.Settings.IsWebMessageEnabled = false;
        core.IsMuted = true;
        core.NewWindowRequested += (_, args) =>
        {
            // Popups are refused outright, as on macOS: sign-in has no use for a second window.
            args.Handled = true;
        };
        core.NavigationStarting += (_, args) =>
        {
            // A window that follows an arbitrary redirect is a window collecting a password for
            // somebody else; where it may go is Rust's rule.
            if (!ShellSupport.IsAllowedLoginUrl(args.Uri))
            {
                BridgeLog.Write($"sign-in navigation refused to {BridgeLog.Describe(args.Uri)}");
                args.Cancel = true;
            }
        };
        // Subframes are conceded, as the shared policy does: they are how sign-in serves its
        // challenges, and refusing them breaks the surface without making it any safer.
        core.NavigationCompleted += (_, _) => StartPolling(core);

        core.Navigate("https://accounts.google.com/ServiceLogin?continue="
            + Uri.EscapeDataString("https://music.youtube.com"));
        return await _outcome.Task;
    }

    /// <summary>
    /// Keeps the account's profile once Rust has stored and activated the account.
    /// </summary>
    internal void CommitPromotion() => _promoted = true;

    /// <summary>Clears the staged profile, for any ending in which the account was not kept.</summary>
    internal async Task DiscardAsync()
    {
        if (_promoted)
        {
            return;
        }

        try
        {
            if (_view.CoreWebView2 is { } core)
            {
                await core.Profile.ClearBrowsingDataAsync();
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"staged profile could not be cleared: {error.Message}");
        }
    }

    internal void Close()
    {
        _closing = true;
        _polling?.Cancel();
        try
        {
            _window.Close();
        }
        catch (Exception)
        {
            // Already closed by the user.
        }
    }

    private void StartPolling(CoreWebView2 core)
    {
        _polling?.Cancel();
        var polling = new CancellationTokenSource();
        _polling = polling;
        _ = PollAsync(core, polling.Token);
    }

    private async Task PollAsync(CoreWebView2 core, CancellationToken cancel)
    {
        var deadline = DateTime.UtcNow + CompletionTimeout;
        while (!cancel.IsCancellationRequested && !_finished)
        {
            if (DateTime.UtcNow >= deadline)
            {
                BridgeLog.Write("sign-in timed out");
                Finish(null);
                return;
            }

            var page = core.Source;
            if (ShellSupport.IsLoginCompletionOrigin(page))
            {
                try
                {
                    var metadata = await WebProfiles.EvaluateAsync(core, CompletionScript);
                    if (!cancel.IsCancellationRequested && !string.IsNullOrEmpty(metadata)
                        && ShellSupport.IsLoginCompletionOrigin(core.Source)
                        && ShellSupport.LoginResult(_accountId, _profileId, metadata, core.Source) is { } summary
                        && System.Text.Json.JsonSerializer.Deserialize<AccountLoginSummary>(summary) is { } cleaned)
                    {
                        BridgeLog.Write("sign-in completed");
                        Finish(new AccountLoginResult(_accountId, _profileId, cleaned));
                        return;
                    }
                }
                catch (Exception error)
                {
                    BridgeLog.Write($"sign-in probe failed: {error.Message}");
                }
            }

            try
            {
                await Task.Delay(500, cancel);
            }
            catch (TaskCanceledException)
            {
                return;
            }
        }
    }

    private void Finish(AccountLoginResult? result)
    {
        if (_finished)
        {
            return;
        }

        _finished = true;
        _polling?.Cancel();
        _outcome.TrySetResult(result);
    }
}
