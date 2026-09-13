using System;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Microsoft.UI.Xaml.Controls;
using Microsoft.Web.WebView2.Core;

namespace Goosic.Windows.Service;

/// <summary>What the page reported, once it has been believed.</summary>
internal sealed record BridgeEvent
{
    [JsonPropertyName("videoId")] public string VideoId { get; init; } = "";
    [JsonPropertyName("sequence")] public ulong Sequence { get; init; }
    [JsonPropertyName("state")] public string State { get; init; } = "";
    [JsonPropertyName("currentTime")] public double CurrentTime { get; init; }
    [JsonPropertyName("duration")] public double Duration { get; init; }
    [JsonPropertyName("isAdvertisement")] public bool IsAdvertisement { get; init; }
    [JsonPropertyName("volume")] public double Volume { get; init; }
    [JsonPropertyName("muted")] public bool Muted { get; init; }
}

/// <summary>The verdict the rules library returned for one message.</summary>
internal sealed record BridgeVerdict
{
    [JsonPropertyName("accepted")] public bool Accepted { get; init; }
    [JsonPropertyName("reason")] public string? Reason { get; init; }
    [JsonPropertyName("event")] public BridgeEvent? Event { get; init; }
}

/// <summary>
/// Plays a track in the official web player, inside a WebView2.
/// </summary>
/// <remarks>
/// This is the Windows counterpart to the macOS WKWebView host, and it carries the same
/// responsibility: the page is a renderer, never an authority. Rust owns the lease, so the host
/// claims one before anything loads and reports what the page confirms rather than what the
/// shell asked for.
///
/// Nothing the page says is trusted on its own. Every message is handed to
/// <c>goosic-shell-support</c>, which decides whether it came from the current document, the
/// active lease generation, the requested track, and a sequence that advanced -- and the answer
/// is applied here without second-guessing it. Advertisements are reported as markers and never
/// bypassed.
/// </remarks>
internal sealed class OfficialPlaybackHost
{
    private readonly WebView2 _view;
    private readonly GoosicServiceClient _client;
    private string _token = "";
    private string _videoId = "";
    private ulong _generation;
    private ulong _lastSequence;
    private bool _ready;
    private bool _observerPending;

    internal OfficialPlaybackHost(WebView2 view, GoosicServiceClient client)
    {
        _view = view;
        _client = client;
    }

    /// <summary>Raised when the page reports something the rules accepted.</summary>
    internal event Action<BridgeEvent>? Sampled;

    /// <summary>Raised with anything worth saying on screen.</summary>
    internal event Action<string>? Status;

    /// <summary>Whether this host currently owns a loaded official document.</summary>
    internal bool HasLoadedTrack => _ready && _videoId.Length > 0;

    /// <summary>
    /// Claims the lease and loads a track.
    /// </summary>
    /// <remarks>
    /// The claim comes first. A renderer that started producing sound before Rust agreed would
    /// have escaped the authority, which is the one thing every shell has to get right.
    /// </remarks>
    internal async Task PlayAsync(string videoId)
    {
        BridgeLog.Write("play requested");
        if (!ShellSupport.IsValidVideoId(videoId))
        {
            BridgeLog.Write("play refused: not a video id");
            Status?.Invoke("That is not a playable track.");
            return;
        }

        try
        {
            // The authority is asked what it currently holds rather than told. `claim` takes the
            // generation the caller believes is active and refuses anything else, which is how a
            // shell working from a stale idea of the lease is caught instead of quietly winning.
            var snapshot = await _client.RequestAsync("state.get").ConfigureAwait(true);
            var owner = snapshot?["state"]?["owner"]?.GetValue<string>() ?? "none";
            var generation = snapshot?["state"]?["generation"]?.GetValue<ulong>() ?? 0;
            BridgeLog.Write($"authority holds owner={owner} generation={generation}");

            // One owner at a time: switching tracks means giving the lease back before taking it
            // again, and the release is what quiesces the renderer that currently holds it.
            if (owner != "none")
            {
                var released = await _client.RequestAsync("playback.release", new System.Text.Json.Nodes.JsonObject
                {
                    ["owner"] = owner,
                    ["generation"] = generation,
                }).ConfigureAwait(true);
                generation = released?["state"]?["generation"]?.GetValue<ulong>() ?? generation + 1;
            }

            var claim = await _client.RequestAsync("playback.claim", new System.Text.Json.Nodes.JsonObject
            {
                ["owner"] = "officialWebView",
                ["generation"] = generation,
            }).ConfigureAwait(true);

            // The generation the observer is stamped with is the one Rust just issued, so an
            // event from the previous lease cannot be mistaken for this one.
            _generation = claim?["state"]?["generation"]?.GetValue<ulong>() ?? generation + 1;
        }
        catch (ServiceRefusedException refused)
        {
            BridgeLog.Write($"claim refused: {refused.Code} {refused.Message}");
            // Rust refused the transition. That is an answer, not a failure to be worked around.
            Status?.Invoke($"Rust refused the claim: {refused.Message}");
            return;
        }
        catch (ServiceUnavailableException unavailable)
        {
            Status?.Invoke(unavailable.Message);
            return;
        }

        // A fresh token per load, so a message from the document being replaced is recognisable
        // as coming from a superseded page rather than being mistaken for the new one.
        _token = Guid.NewGuid().ToString("n");
        _videoId = videoId;
        _lastSequence = 0;

        BridgeLog.Write($"lease claimed generation={_generation}; preparing WebView2");
        await EnsureReadyAsync().ConfigureAwait(true);
        BridgeLog.Write("WebView2 ready");

        // The observer for this load is installed once the page has finished loading; see
        // InstallObserverAsync for why it is not a document-created script.
        _observerPending = true;
        BridgeLog.Write($"claimed generation={_generation}; loading requested track");
        _view.CoreWebView2.Navigate($"https://{ShellSupport.AllowedHost}/watch?v={Uri.EscapeDataString(videoId)}");
    }

    private async Task EnsureReadyAsync()
    {
        if (_ready)
        {
            return;
        }

        BridgeLog.Write($"initialising WebView2 (control loaded={_view.IsLoaded})");

        // The profile lives under the user's own Goosic data, not beside the executable. The
        // default for an unpackaged app is a folder next to the .exe, which is the wrong place for
        // a profile once the app is installed somewhere the user cannot write, and is invisible
        // to whoever later has to clear it.
        var profile = System.IO.Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Goosic", "WebView2", "guest");
        System.IO.Directory.CreateDirectory(profile);

        CoreWebView2Environment environment;
        try
        {
            environment = await CoreWebView2Environment.CreateWithOptionsAsync(null, profile, null);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"WebView2 environment failed: {error.Message}");
            Status?.Invoke($"The web player could not start: {error.Message}");
            throw;
        }

        BridgeLog.Write($"WebView2 environment created, runtime {environment.BrowserVersionString}");

        // Bounded, because the failure this guards against is a wait that never ends: an
        // initialisation that cannot complete otherwise leaves the shell silently claiming a
        // lease for a renderer that will never exist.
        var initialise = _view.EnsureCoreWebView2Async(environment).AsTask();
        if (await Task.WhenAny(initialise, Task.Delay(TimeSpan.FromSeconds(20))) != initialise)
        {
            BridgeLog.Write("WebView2 initialisation timed out");
            Status?.Invoke("The web player did not start in time.");
            throw new TimeoutException("WebView2 initialisation timed out");
        }

        await initialise;
        BridgeLog.Write($"WebView2 initialised, browser {_view.CoreWebView2.Environment.BrowserVersionString}");
        var core = _view.CoreWebView2;

        // The page must not install its own media-session handlers: the system transport belongs
        // to this shell, and a page that answered the media keys would be acting without Rust.
        await core.AddScriptToExecuteOnDocumentCreatedAsync(ShellSupport.MediaSessionGuardScript());

        // The bridge shim carries no per-load state, so it is registered once for every document.
        await core.AddScriptToExecuteOnDocumentCreatedAsync(WebView2BridgeShimScript());

        core.WebMessageReceived += OnWebMessage;

        // The click that selected a track has necessarily awaited Rust's lease claim before the
        // navigation starts, so Chromium no longer considers it a user gesture. Grant autoplay
        // only to the official origin, only for this request, and never write the decision into
        // the WebView profile. Nothing else obtains a permission through this shell.
        core.PermissionRequested += (_, args) =>
        {
            if (args.PermissionKind == CoreWebView2PermissionKind.Autoplay
                && IsOfficialOrigin(args.Uri))
            {
                args.State = CoreWebView2PermissionState.Allow;
                args.SavesInProfile = false;
                args.Handled = true;
            }
        };

        // Loading the official player is the slowest part of starting a track, and a shell that
        // says nothing during it looks broken rather than busy.
        core.NavigationCompleted += async (_, args) =>
        {
            BridgeLog.Write($"navigation completed success={args.IsSuccess} status={args.WebErrorStatus} "
                + $"at {BridgeLog.Describe(core.Source)}");
            if (!args.IsSuccess)
            {
                Status?.Invoke($"The official player did not load ({args.WebErrorStatus}).");
                return;
            }

            await ProbePageAsync(core);
            await InstallObserverAsync(core);
        };

        // Only the official host may run in here. A navigation anywhere else is cancelled rather
        // than followed, so a redirect cannot turn this into a general browser.
        core.NavigationStarting += (_, args) =>
        {
            var allowed = IsOfficialNavigation(args.Uri);
            BridgeLog.Write($"navigation starting {BridgeLog.Describe(args.Uri)} allowed={allowed}");
            if (!allowed)
            {
                args.Cancel = true;
            }
        };

        _ready = true;
    }

    /// <summary>
    /// Records what the loaded page can actually do, so a silent player can be diagnosed.
    /// </summary>
    /// <remarks>
    /// Each field answers one way the chain can break: the shim not installed, the WebView2
    /// message port absent, no media element yet, or a media element that exists but is paused.
    /// </remarks>
    private async Task ProbePageAsync(CoreWebView2 core)
    {
        try
        {
            var result = await core.ExecuteScriptAsync("""
                (() => {
                  const media = document.querySelector('audio,video');
                  return JSON.stringify({
                    shim: !!window.webkit?.messageHandlers,
                    port: typeof window.chrome?.webview?.postMessage,
                    media: !!media,
                    paused: media ? media.paused : null,
                    readyState: media ? media.readyState : null,
                    title: document.title,
                    href: location.pathname + (new URLSearchParams(location.search).has('v') ? '?v=present' : '?v=absent'),
                  });
                })();
                """);
            BridgeLog.Write($"page probe {result}");
        }
        catch (Exception error)
        {
            BridgeLog.Write($"page probe failed: {error.Message}");
        }
    }

    /// <summary>
    /// Installs the shared observer for the load that just finished, exactly once.
    /// </summary>
    /// <remarks>
    /// Registered as a document-created script, the observer never reported a single sample on
    /// WebView2: the shim beside it was present, the message port worked, a media element existed,
    /// and nothing was posted. The identical script executed once the page had loaded began
    /// reporting at once and the track played -- verified against the running app, with the
    /// bridge log showing accepted samples through a pre-roll advertisement.
    ///
    /// Running after load cannot duplicate reports: the observer samples on a timer and on media
    /// events, so starting it after the page is ready loses nothing but a moment of silence, and
    /// a load that fires NavigationCompleted twice is guarded twice -- once here by the pending
    /// flag, and once in the page by the token, which also refuses a second copy if the page
    /// re-runs its own navigation within the same document.
    /// </remarks>
    private async Task InstallObserverAsync(CoreWebView2 core)
    {
        if (!_observerPending)
        {
            return;
        }

        _observerPending = false;
        var token = ShellSupport.JsStringLiteral(_token);
        var observer = ShellSupport.ObserverScript(_token, _generation, _videoId);
        try
        {
            var outcome = await core.ExecuteScriptAsync(
                "(() => { if (window.__goosicObserverToken === " + token + ") return 'already'; "
                + "window.__goosicObserverToken = " + token + "; "
                + "try { " + observer + " return 'installed'; } catch (error) { return 'threw: ' + error; } })()");
            BridgeLog.Write($"observer {outcome}");
        }
        catch (Exception error)
        {
            BridgeLog.Write($"observer install failed: {error.Message}");
            Status?.Invoke($"Could not attach to the official player: {error.Message}");
        }
    }

    /// <summary>The narrowly adapted message port that the shared WebKit observer requires.</summary>
    private static string WebView2BridgeShimScript()
    {
        var handler = ShellSupport.JsStringLiteral(ShellSupport.HandlerName);
        return $$"""
            (() => {
              const name = {{handler}};
              if (window.webkit?.messageHandlers?.[name]) return;
              window.webkit = window.webkit || {};
              window.webkit.messageHandlers = window.webkit.messageHandlers || {};
              window.webkit.messageHandlers[name] = {
                postMessage: (payload) => window.chrome.webview.postMessage(JSON.stringify(payload)),
              };
            })();
            """;
    }

    /// <summary>Whether a URL is one of the official player's controlled subdomains.</summary>
    private static bool IsOfficialNavigation(string value) =>
        Uri.TryCreate(value, UriKind.Absolute, out var target)
        && target.Scheme == Uri.UriSchemeHttps
        && (target.Host == ShellSupport.AllowedHost
            || target.Host.EndsWith("." + ShellSupport.AllowedHost, StringComparison.Ordinal));

    /// <summary>Autoplay is narrower than navigation: only the player origin is trusted.</summary>
    private static bool IsOfficialOrigin(string value) =>
        Uri.TryCreate(value, UriKind.Absolute, out var target)
        && target.Scheme == Uri.UriSchemeHttps
        && target.Host == ShellSupport.AllowedHost;

    /// <summary>Requests a pause or resume from the page that already holds Rust's lease.</summary>
    /// <remarks>
    /// This does not manufacture playback state. The next observer sample is still what changes
    /// the shell's now-playing UI and what Rust receives, so a rejected or ignored page request
    /// cannot be presented as a successful transition.
    /// </remarks>
    internal async Task TogglePauseAsync()
    {
        if (!HasLoadedTrack)
        {
            Status?.Invoke("Choose a track to begin.");
            return;
        }

        try
        {
            await _view.CoreWebView2.ExecuteScriptAsync("""
                (() => {
                  const media = document.querySelector('audio,video');
                  if (!media) return 'no-media';
                  if (media.paused) { void media.play(); return 'play-requested'; }
                  media.pause(); return 'pause-requested';
                })();
                """);
        }
        catch (Exception error)
        {
            Status?.Invoke($"Could not control the official player: {error.Message}");
        }
    }

    /// <summary>Requests a bounded seek; the next bridge sample remains the source of truth.</summary>
    internal async Task SeekAsync(double position)
    {
        if (!HasLoadedTrack || !double.IsFinite(position) || position < 0)
        {
            return;
        }

        try
        {
            var requested = position.ToString(System.Globalization.CultureInfo.InvariantCulture);
            await _view.CoreWebView2.ExecuteScriptAsync($$"""
                (() => {
                  const media = document.querySelector('audio,video');
                  if (!media || !Number.isFinite(media.duration) || media.duration <= 0) return 'not-seekable';
                  media.currentTime = Math.min(Math.max({{requested}}, 0), media.duration);
                  return 'seek-requested';
                })();
                """);
        }
        catch (Exception error)
        {
            Status?.Invoke($"Could not seek in the official player: {error.Message}");
        }
    }

    /// <summary>Requests a clamped page volume without treating that request as confirmed state.</summary>
    internal async Task SetVolumeAsync(double volume)
    {
        if (!HasLoadedTrack || !double.IsFinite(volume)) return;
        var requested = Math.Clamp(volume, 0, 1).ToString(System.Globalization.CultureInfo.InvariantCulture);
        try
        {
            await _view.CoreWebView2.ExecuteScriptAsync($$"""
                (() => {
                  const media = document.querySelector('audio,video');
                  if (!media) return 'no-media';
                  media.volume = {{requested}}; media.muted = false; return 'volume-requested';
                })();
                """);
        }
        catch (Exception error)
        {
            Status?.Invoke($"Could not set official-player volume: {error.Message}");
        }
    }

    /// <summary>Requests mute inversion; bridge samples publish the result.</summary>
    internal async Task ToggleMutedAsync()
    {
        if (!HasLoadedTrack) return;
        try
        {
            await _view.CoreWebView2.ExecuteScriptAsync("""
                (() => { const media = document.querySelector('audio,video');
                  if (!media) return 'no-media'; media.muted = !media.muted; return 'mute-requested'; })();
                """);
        }
        catch (Exception error)
        {
            Status?.Invoke($"Could not change official-player mute: {error.Message}");
        }
    }

    private void OnWebMessage(CoreWebView2 sender, CoreWebView2WebMessageReceivedEventArgs args)
    {
        BridgeLog.Write($"message arrived from {BridgeLog.Describe(args.Source)}");
        string body;
        try
        {
            body = args.TryGetWebMessageAsString();
        }
        catch (ArgumentException)
        {
            // Not a string message, so not one of ours.
            BridgeLog.Write("message ignored: not a string");
            return;
        }

        if (body.Length > ShellSupport.MaxBodyBytes)
        {
            BridgeLog.Write($"message ignored: {body.Length} bytes is over the limit");
            return;
        }

        var verdictJson = ShellSupport.ValidateEvent(body, _token, _generation, _videoId, _lastSequence);
        var verdict = JsonSerializer.Deserialize<BridgeVerdict>(verdictJson, ServiceProtocol.Json);
        if (verdict is null)
        {
            return;
        }

        if (!verdict.Accepted || verdict.Event is null)
        {
            // Kept quiet on screen: a superseded document reports for a moment after every load,
            // and saying so each time would be noise rather than information.
            BridgeLog.Write($"message rejected: {verdict.Reason}");
            return;
        }

        BridgeLog.Write($"sample accepted seq={verdict.Event.Sequence} state={verdict.Event.State} "
            + $"t={verdict.Event.CurrentTime:F1}/{verdict.Event.Duration:F1} ad={verdict.Event.IsAdvertisement}");
        _lastSequence = verdict.Event.Sequence;
        Sampled?.Invoke(verdict.Event);
        _ = ReportSampleAsync(verdict.Event);
    }

    /// <summary>Tells Rust what the page confirmed.</summary>
    /// <remarks>
    /// An advertisement is reported as a marker. It is information, never something to skip:
    /// bypassing it is one of the things this project does not do.
    /// </remarks>
    private async Task ReportSampleAsync(BridgeEvent sample)
    {
        try
        {
            var payload = new System.Text.Json.Nodes.JsonObject
            {
                ["owner"] = "officialWebView",
                ["generation"] = _generation,
                ["sequence"] = sample.Sequence,
            };
            if (sample.IsAdvertisement)
            {
                payload["marker"] = "advertisement";
            }

            await _client.RequestAsync("playback.sample", payload).ConfigureAwait(true);
        }
        catch (ServiceRefusedException refused)
        {
            Status?.Invoke($"Rust rejected a sample: {refused.Message}");
        }
        catch (ServiceUnavailableException)
        {
            // The transport reports its own loss; nothing useful to add per sample.
        }
    }
}
