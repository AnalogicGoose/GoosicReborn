using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Tasks;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;

namespace Goosic.Windows.Service;

/// <summary>
/// Reads and changes the signed-in account's library inside that account's own WebView2 profile.
/// </summary>
/// <remarks>
/// <para>
/// The Windows counterpart of <c>PersonalCatalogHost+macOS.swift</c>. The program is the same
/// <c>PersonalCatalog.js</c>, ported from the previous Goosic under GPL-3.0 and linked into this
/// build from the Swift resources rather than copied, so there is one reader for both shells. It
/// runs on a YouTube Music page the account already has cookies for; the session cookie it hashes
/// for authorisation is read inside the document and never leaves it. What comes back is the same
/// small catalog shape the anonymous service returns -- titles, ids, artwork, durations.
/// </para>
/// <para>
/// Nothing here goes through the service protocol. Catalog reads there are anonymous by
/// construction, and an account-scoped read is exactly what they must never become.
/// </para>
/// <para>
/// One page per account, loaded once and kept, because loading YouTube Music's application takes
/// long enough that paying it per request would make every library screen slow. It is muted: it
/// is a reader, never a player.
/// </para>
/// <para>
/// Between reads it costs as little as a loaded page can: it asks WebView2 for its low memory
/// target, is suspended once nothing has been asked of it for a while, and is closed altogether
/// after a longer quiet spell, to be loaded again by the next read.
/// </para>
/// </remarks>
internal sealed class PersonalCatalogHost
{
    private static readonly TimeSpan RequestTimeout = TimeSpan.FromSeconds(45);
    private static readonly TimeSpan SuspendAfter = TimeSpan.FromSeconds(20);
    private static readonly TimeSpan CloseAfter = TimeSpan.FromMinutes(5);
    private static string? _program;

    private readonly Panel _container;
    private WebView2? _view;
    private string? _profileName;
    private TaskCompletionSource<CoreWebView2>? _page;
    private readonly Dictionary<string, Task<JsonNode?>> _inFlight = new();
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _suspendTimer;
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer _closeTimer;
    private int _running;

    internal PersonalCatalogHost(Panel container)
    {
        _container = container;
        var queue = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        _suspendTimer = queue.CreateTimer();
        _suspendTimer.Interval = SuspendAfter;
        _suspendTimer.IsRepeating = false;
        _suspendTimer.Tick += async (_, _) => await SuspendAsync();
        _closeTimer = queue.CreateTimer();
        _closeTimer.Interval = CloseAfter;
        _closeTimer.IsRepeating = false;
        _closeTimer.Tick += (_, _) => CloseIdlePage();
    }

    internal bool IsSignedIn => _profileName is not null;

    /// <summary>Binds to an account's profile, or to none. Anything in flight for the old one fails.</summary>
    internal void Bind(string? webProfileId)
    {
        var name = webProfileId is null ? null : WebProfiles.NameFor(webProfileId);
        if (name == _profileName)
        {
            return;
        }

        _profileName = name;
        _page?.TrySetException(new InvalidOperationException("The active account changed while the library was loading."));
        _page = null;
        _inFlight.Clear();
        _suspendTimer.Stop();
        _closeTimer.Stop();
        DropView();

        if (name is not null)
        {
            // Warmed now, so the first library read after signing in does not pay for the load.
            _ = PageAsync();
        }
    }

    /// <summary>A browse page for the account: Home, a library section, or a playlist.</summary>
    /// <param name="shape">"tracks", "shelves", or "auto" to decide from the browse id.</param>
    internal Task<CatalogPage> BrowseAsync(string browseId, string title, string? continuation, string shape) =>
        RunPageAsync("browse", new JsonArray(browseId, title, continuation, shape));

    /// <summary>An Up Next radio as the account hears it.</summary>
    internal Task<CatalogPage> RadioAsync(string seedVideoId, string? continuation) =>
        RunPageAsync("radio", new JsonArray(seedVideoId, continuation));

    /// <summary>
    /// Applies one named change to the account.
    /// </summary>
    /// <remarks>
    /// The shell names an operation; it never composes an authenticated request. An identical
    /// change already on its way is joined rather than repeated, so a second press of Like cannot
    /// race the first into the opposite rating.
    /// </remarks>
    internal async Task<JsonNode?> MutateAsync(string operation, JsonObject arguments)
    {
        var key = operation + "?" + arguments.ToJsonString();
        if (_inFlight.TryGetValue(key, out var joined))
        {
            return await joined;
        }

        var task = RunAsync("mutate", new JsonArray(operation, arguments));
        _inFlight[key] = task;
        try
        {
            return await task;
        }
        finally
        {
            _inFlight.Remove(key);
        }
    }

    private async Task<CatalogPage> RunPageAsync(string function, JsonArray arguments)
    {
        var node = await RunAsync(function, arguments);
        return node.Deserialize<CatalogPage>(ServiceProtocol.Json)
            ?? throw new InvalidOperationException("YouTube Music returned an unreadable personal library.");
    }

    private async Task<JsonNode?> RunAsync(string function, JsonArray arguments)
    {
        if (_profileName is null)
        {
            throw new InvalidOperationException("Sign in to load your personal library.");
        }

        var program = Program();
        var profile = _profileName;
        _running++;
        _suspendTimer.Stop();
        _closeTimer.Stop();
        string? json;
        try
        {
            var core = await WithTimeout(PageAsync());
            if (core.IsSuspended)
            {
                core.Resume();
            }

            // Arguments cross as one JSON literal, spread into the call, so no value is ever
            // spliced into the program as source text.
            var expression = "(async () => {\n" + program + "\nconst __args = " + arguments.ToJsonString()
                + ";\nreturn await GoosicPersonalCatalog." + function + "(...__args);\n})()";
            json = await WithTimeout(WebProfiles.EvaluateAsync(core, expression));
        }
        finally
        {
            if (--_running == 0 && _page is not null)
            {
                _suspendTimer.Start();
                _closeTimer.Start();
            }
        }

        if (profile != _profileName)
        {
            throw new InvalidOperationException("The active account changed while the library was loading.");
        }

        return json is null ? null : JsonNode.Parse(json);
    }

    private static async Task<T> WithTimeout<T>(Task<T> task)
    {
        if (await Task.WhenAny(task, Task.Delay(RequestTimeout)) != task)
        {
            throw new TimeoutException("YouTube Music did not answer in time. Try again.");
        }

        return await task;
    }

    private static string Program()
    {
        if (_program is null)
        {
            var path = Path.Combine(AppContext.BaseDirectory, "Resources", "PersonalCatalog.js");
            _program = File.Exists(path)
                ? File.ReadAllText(path)
                : throw new InvalidOperationException("The personal catalog program is missing from the app.");
        }

        return _program;
    }

    private Task<CoreWebView2> PageAsync()
    {
        if (_page is not null)
        {
            return _page.Task;
        }

        var page = new TaskCompletionSource<CoreWebView2>();
        _page = page;
        _ = LoadPageAsync(page, _profileName!);
        return page.Task;
    }

    private async Task LoadPageAsync(TaskCompletionSource<CoreWebView2> page, string profileName)
    {
        try
        {
            var environment = await WebProfiles.EnvironmentAsync();
            var view = new WebView2 { Width = 1, Height = 1, IsHitTestVisible = false, Opacity = 0 };
            _container.Children.Add(view);
            _view = view;
            await view.EnsureCoreWebView2Async(environment, WebProfiles.OptionsFor(environment, profileName));
            if (!ReferenceEquals(page, _page))
            {
                return;
            }

            var core = view.CoreWebView2;
            core.IsMuted = true;
            // A reader never needs its caches warm; the next read is a fetch, not a repaint.
            core.MemoryUsageTargetLevel = CoreWebView2MemoryUsageTargetLevel.Low;
            core.Settings.AreDefaultScriptDialogsEnabled = false;
            core.Settings.IsWebMessageEnabled = false;
            core.NewWindowRequested += (_, args) => args.Handled = true;
            core.NavigationStarting += (_, args) =>
            {
                if (!WebProfiles.IsOfficialNavigation(args.Uri))
                {
                    args.Cancel = true;
                }
            };

            var loaded = new TaskCompletionSource<bool>();
            core.NavigationCompleted += (_, args) => loaded.TrySetResult(args.IsSuccess);
            core.Navigate("https://music.youtube.com/");
            if (!await loaded.Task)
            {
                throw new InvalidOperationException("YouTube Music did not load for this account.");
            }

            BridgeLog.Write("personal catalog page ready");
            // Hidden rather than transparent, which is what lets WebView2 suspend it when idle.
            view.Visibility = Microsoft.UI.Xaml.Visibility.Collapsed;
            page.TrySetResult(core);
            if (_running == 0)
            {
                _suspendTimer.Start();
                _closeTimer.Start();
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"personal catalog page failed: {error.Message}");
            if (ReferenceEquals(page, _page))
            {
                _page = null;
            }

            page.TrySetException(error);
        }
    }

    private async Task SuspendAsync()
    {
        if (_running > 0 || _view?.CoreWebView2 is not { IsSuspended: false } core)
        {
            return;
        }

        try
        {
            await core.TrySuspendAsync();
        }
        catch (Exception error)
        {
            BridgeLog.Write($"personal catalog page could not be suspended: {error.Message}");
        }
    }

    /// <summary>Unloads the reader after a long quiet spell; the next read loads it again.</summary>
    private void CloseIdlePage()
    {
        if (_running > 0 || _page is not { Task.IsCompletedSuccessfully: true })
        {
            return;
        }

        _page = null;
        DropView();
        BridgeLog.Write("personal catalog page closed while idle");
    }

    private void DropView()
    {
        if (_view is not null)
        {
            _container.Children.Remove(_view);
            _view.Close();
            _view = null;
        }
    }

    /// <summary>Clears everything the bound account's profile holds, for signing out.</summary>
    internal async Task ClearProfileAsync()
    {
        try
        {
            var core = await WithTimeout(PageAsync());
            await core.Profile.ClearBrowsingDataAsync();
        }
        catch (Exception error)
        {
            BridgeLog.Write($"profile could not be cleared: {error.Message}");
        }
    }
}
