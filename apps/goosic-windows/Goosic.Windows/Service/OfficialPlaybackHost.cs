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
    private readonly Microsoft.UI.Xaml.Controls.Panel _container;
    private WebView2 _view = new();
    private string _profileName = WebProfiles.GuestProfile;
    private readonly GoosicServiceClient _client;
    private string _token = "";
    private string _videoId = "";
    private ulong _generation;
    private ulong _lastSequence;
    private bool _ready;
    private bool _observerPending;
    private long _playRequest;
    private readonly System.Threading.SemaphoreSlim _playGate = new(1, 1);

    internal OfficialPlaybackHost(Microsoft.UI.Xaml.Controls.Panel container, GoosicServiceClient client)
    {
        _container = container;
        _client = client;
    }

    /// <summary>
    /// Opens the player into an account's profile, or the guest one, from the next track on.
    /// </summary>
    /// <remarks>
    /// A WebView2 cannot move between profiles, so the renderer is torn down and a new one is made
    /// on the next load. The caller has already given the lease back: tearing down is what silences
    /// the old renderer, and a player that kept sounding across an account change would be playing
    /// under an owner Rust no longer recognises.
    /// </remarks>
    internal void BindProfile(string? webProfileId)
    {
        var name = WebProfiles.NameFor(webProfileId);
        if (name == _profileName)
        {
            return;
        }

        _profileName = name;
        Stop();
    }

    /// <summary>Silences and discards the renderer, and refuses anything it still reports.</summary>
    internal void Stop()
    {
        _token = "";
        _videoId = "";
        _observerPending = false;
        if (_ready)
        {
            _view.CoreWebView2.WebMessageReceived -= OnWebMessage;
            _container.Children.Remove(_view);
            _view.Close();
            _view = new WebView2();
            _ready = false;
        }
    }

    /// <summary>Raised when the page reports something the rules accepted.</summary>
    internal event Action<BridgeEvent>? Sampled;

    /// <summary>
    /// Raised with the requested video id when the page moved on to a track of its own choosing.
    /// </summary>
    internal event Action<string>? PageMovedOn;

    /// <summary>Raised with the requested id when the page replaced it without playing it.</summary>
    internal event Action<string>? PageRefused;

    /// <summary>
    /// Asked, before the requested track was ever heard, whether the page's replacement is the same
    /// song under another id. Given the requested id, the page's id and the page's title.
    /// </summary>
    internal Func<string, string, string, bool>? IsSubstitute;

    private string _lastState = "";

    /// <summary>
    /// Says so when YouTube Music paused the track to ask whether anyone is still listening.
    /// </summary>
    /// <remarks>
    /// The prompt is the page's, and it is answered only by the listener pressing Play; this only
    /// explains a pause nobody asked for.
    /// </remarks>
    private async Task NoticeIdlePromptAsync()
    {
        try
        {
            var shown = await _view.CoreWebView2.ExecuteScriptAsync("""
                (() => {
                  const prompt = document.querySelector('ytmusic-you-there-renderer');
                  return !!prompt && prompt.offsetParent !== null;
                })()
                """);
            if (shown == "true")
            {
                BridgeLog.Write("the page paused to ask whether anyone is still listening");
                Status?.Invoke("YouTube Music paused to check you’re still listening. Press Play to continue.");
            }
        }
        catch (Exception error)
        {
            BridgeLog.Write($"idle prompt check failed: {error.GetType().Name}");
        }
    }

    /// <summary>Whether the requested track has been heard playing under this load.</summary>
    private bool _heardRequested;

    private int _strayReports;

    /// <summary>The volume and mute the listener last chose, or null before they chose one.</summary>
    private double? _preferredVolume;
    private bool _preferredMuted;
    private bool _lastMuted;
    private string? _volumePrimerId;
    private DateTime _lastVolumeFix = DateTime.MinValue;
    private bool _movedOnHandled;

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

        // Loads are serialised, and only the most recent request survives the wait. Two that ran
        // together would each read the lease, each release it, and each claim it -- leaving one
        // renderer holding a generation the other just released.
        var request = System.Threading.Interlocked.Increment(ref _playRequest);
        await _playGate.WaitAsync().ConfigureAwait(true);
        try
        {
            if (request != _playRequest)
            {
                BridgeLog.Write("play superseded by a newer request");
                return;
            }

            await LoadAsync(videoId).ConfigureAwait(true);
        }
        finally
        {
            _playGate.Release();
        }
    }

    private async Task LoadAsync(string videoId)
    {
        // The page being replaced keeps posting until it unloads. Its reports are refused from
        // this moment rather than after the new claim: between Rust releasing the old lease and
        // granting the new one there is no owner at all, and a report forwarded in that window is
        // rejected with "no playback owner is active".
        _token = "";
        _videoId = "";

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
        _strayReports = 0;
        _movedOnHandled = false;
        _heardRequested = false;
        _lastState = "";

        BridgeLog.Write($"lease claimed generation={_generation}; preparing WebView2");
        await EnsureReadyAsync().ConfigureAwait(true);
        BridgeLog.Write("WebView2 ready");

        // The observer for this load is installed once the page has finished loading; see
        // InstallObserverAsync for why it is not a document-created script.
        _observerPending = true;
        BridgeLog.Write($"claimed generation={_generation}; loading requested track");
        await PrimeVolumeAsync(_view.CoreWebView2);
        _view.CoreWebView2.Navigate($"https://{ShellSupport.AllowedHost}/watch?v={Uri.EscapeDataString(videoId)}");
    }

    private async Task EnsureReadyAsync()
    {
        if (_ready)
        {
            return;
        }

        BridgeLog.Write($"initialising WebView2 (control loaded={_view.IsLoaded})");

        // Every surface shares one environment, and the player opens into the active account's
        // profile -- or the guest one -- so it plays with that account's cookies and no other.
        CoreWebView2Environment environment;
        try
        {
            environment = await WebProfiles.EnvironmentAsync();
        }
        catch (Exception error)
        {
            BridgeLog.Write($"WebView2 environment failed: {error.Message}");
            Status?.Invoke($"The web player could not start: {error.Message}");
            throw;
        }

        _view.Width = 1;
        _view.Height = 1;
        _view.IsHitTestVisible = false;
        if (!_container.Children.Contains(_view))
        {
            _container.Children.Add(_view);
        }

        BridgeLog.Write($"WebView2 environment created, runtime {environment.BrowserVersionString}");

        // Bounded, because the failure this guards against is a wait that never ends: an
        // initialisation that cannot complete otherwise leaves the shell silently claiming a
        // lease for a renderer that will never exist.
        var initialise = _view.EnsureCoreWebView2Async(
            environment, WebProfiles.OptionsFor(environment, _profileName)).AsTask();
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

        // While music plays the official page installs a beforeunload prompt. In a one-pixel
        // renderer nobody can see or answer it, so the next navigation waits on it forever: the
        // page freezes, NavigationStarting never fires, and Next appears to do nothing. The shell
        // is the one asking to leave, so a leave prompt from the official origin is accepted.
        // Every other dialog is left unanswered, which WebView2 treats as dismissed -- the page
        // gets no way to ask the listener anything through this surface.
        core.Settings.AreDefaultScriptDialogsEnabled = false;
        core.ScriptDialogOpening += (_, args) =>
        {
            var leave = args.Kind == CoreWebView2ScriptDialogKind.Beforeunload && IsOfficialOrigin(args.Uri);
            BridgeLog.Write($"script dialog {args.Kind} from {BridgeLog.Describe(args.Uri)} accepted={leave}");
            if (leave)
            {
                args.Accept();
            }
        };

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
            await KeepAutomixOffAsync(core);
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

    /// <summary>
    /// Turns off the page's Automix, so a track plays to its end.
    /// </summary>
    /// <remarks>
    /// With Automix on, YouTube Music fades into a track of its own choosing some seconds before
    /// the requested one ends. The observer then sees another video, the page is paused, and the
    /// queue moves on -- having cut the song's last seconds. With it off the page plays the track
    /// through and reports its end, and Goosic's queue decides what follows, as it should. The
    /// switch sits in the page's Up Next tab, which renders after the player, so it is looked for
    /// for a while rather than once. This is the renderer's own page, in Goosic's own profile.
    /// </remarks>
    private async Task KeepAutomixOffAsync(CoreWebView2 core)
    {
        var token = _token;
        for (var attempt = 0; attempt < 40 && token == _token; attempt++)
        {
            string outcome;
            try
            {
                outcome = await core.ExecuteScriptAsync("""
                    (() => {
                      const toggle = document.getElementById('automix');
                      if (!toggle || typeof toggle.checked !== 'boolean') return 'absent';
                      if (!toggle.checked) return 'off';
                      toggle.click();
                      return toggle.checked ? 'still on' : 'turned off';
                    })()
                    """);
            }
            catch (Exception error)
            {
                BridgeLog.Write($"automix check failed: {error.GetType().Name}");
                return;
            }

            if (outcome != "\"absent\"")
            {
                BridgeLog.Write($"automix {outcome}");
                return;
            }

            await Task.Delay(500);
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
                  if (media.paused) {
                    // A resume the listener asked for also answers YouTube Music's
                    // "still listening?" prompt, which otherwise pauses the track again.
                    const prompt = document.querySelector('ytmusic-you-there-renderer');
                    prompt?.querySelector('button, yt-button-renderer, tp-yt-paper-button')?.click();
                    void media.play();
                    return 'play-requested';
                  }
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
                  const target = Math.min(Math.max({{requested}}, 0), media.duration);
                  const player = document.getElementById('movie_player');
                  if (player && typeof player.seekTo === 'function') { player.seekTo(target, true); return 'seek-requested'; }
                  media.currentTime = target;
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
    /// <summary>
    /// Sets the volume, and remembers it as the listener's choice.
    /// </summary>
    /// <remarks>
    /// Every track is a new page, and a new page starts at whatever volume YouTube Music keeps for
    /// itself -- so the level the listener chose was lost on some track changes. The choice is kept
    /// here and put back whenever a report shows the page has drifted from it.
    /// </remarks>
    internal async Task SetVolumeAsync(double volume)
    {
        if (!double.IsFinite(volume))
        {
            return;
        }

        _preferredVolume = Math.Clamp(volume, 0, 1);
        if (_preferredVolume > 0)
        {
            _preferredMuted = false;
        }

        await ApplyPreferredVolumeAsync();
    }

    /// <summary>
    /// Makes the next page start at the chosen volume instead of correcting it once it is playing.
    /// </summary>
    /// <remarks>
    /// Correcting after the first report left each new track at 100 for about a second before it
    /// dropped to the listener's level. Setting the level once as a media element loaded was not
    /// enough either: the player applies its own volume afterwards. This is the macOS shell's
    /// <c>OfficialVolumeBootstrap</c>: installed before the document runs, it holds every media
    /// element's volume and mute setters and its <c>play</c> to the listener's level, so whatever
    /// the page sets, the first sound is already at that level. The host changes the level through
    /// <c>window.goosicSetVolumePreference</c>. An advertisement's volume is left to the page.
    /// </remarks>
    private async Task PrimeVolumeAsync(CoreWebView2 core)
    {
        if (_preferredVolume is not { } volume)
        {
            return;
        }

        if (_volumePrimerId is not null)
        {
            core.RemoveScriptToExecuteOnDocumentCreated(_volumePrimerId);
            _volumePrimerId = null;
        }

        var level = volume.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var muted = _preferredMuted ? "true" : "false";
        try
        {
            _volumePrimerId = await core.AddScriptToExecuteOnDocumentCreatedAsync($$"""
                (() => {
                  if (location.hostname !== 'music.youtube.com') return;
                  let preferred = {{level}}, intendedMute = {{muted}};
                  // The player's own saved level, so its first choice is already the right one.
                  try {
                    const now = Date.now();
                    const data = JSON.stringify({ volume: Math.round(preferred * 100), muted: intendedMute });
                    localStorage.setItem('yt-player-volume', JSON.stringify({ data, expiration: now + 2592000000, creation: now }));
                    sessionStorage.setItem('yt-player-volume', JSON.stringify({ data, creation: now }));
                  } catch (_) {}
                  const proto = HTMLMediaElement.prototype;
                  const volume = Object.getOwnPropertyDescriptor(proto, 'volume');
                  const muted = Object.getOwnPropertyDescriptor(proto, 'muted');
                  const originalPlay = proto.play;
                  const advertisement = () => !!document.querySelector('.ad-showing, .ad-interrupting');
                  // Changes only what differs, and never mutes on the way. Muting, setting the
                  // level and unmuting cut a playing song to silence and back on every step of
                  // the slider, which was heard as crackling while the volume moved. Muting
                  // goes first and unmuting last, so the old level is never heard at the new state.
                  function apply(media) {
                    if (advertisement()) return;
                    const isMuted = muted.get.call(media);
                    if (intendedMute && !isMuted) muted.set.call(media, true);
                    if (volume.get.call(media) !== preferred) volume.set.call(media, preferred);
                    if (!intendedMute && isMuted) muted.set.call(media, false);
                  }
                  Object.defineProperty(proto, 'volume', {
                    ...volume,
                    set(value) {
                      if (advertisement()) { volume.set.call(this, value); return; }
                      apply(this);
                    }
                  });
                  Object.defineProperty(proto, 'muted', {
                    ...muted,
                    set(value) {
                      if (advertisement()) { muted.set.call(this, value); return; }
                      apply(this);
                    }
                  });
                  proto.play = function(...args) { apply(this); return originalPlay.apply(this, args); };
                  const applyAll = () => document.querySelectorAll('video, audio').forEach(apply);
                  window.goosicSetVolumePreference = (value, mute) => {
                    if (Number.isFinite(value)) preferred = Math.min(1, Math.max(0, value));
                    if (typeof mute === 'boolean') intendedMute = mute;
                    if (!advertisement()) applyAll();
                  };
                  for (const name of ['loadstart', 'loadedmetadata', 'play', 'volumechange']) {
                    document.addEventListener(name, (event) => {
                      if (event.target instanceof HTMLMediaElement) apply(event.target);
                    }, true);
                  }
                  new MutationObserver(applyAll).observe(document, {
                    subtree: true, childList: true, attributes: true, attributeFilter: ['class', 'src']
                  });
                  applyAll();
                })();
                """);
        }
        catch (Exception error)
        {
            BridgeLog.Write($"volume primer failed: {error.Message}");
        }
    }

    /// <summary>Adopts a remembered volume without touching the page, for the first track to start at.</summary>
    internal void RestoreVolume(double volume, bool muted)
    {
        if (double.IsFinite(volume))
        {
            _preferredVolume = Math.Clamp(volume, 0, 1);
            _preferredMuted = muted;
        }
    }

    internal bool PreferredMuted => _preferredMuted;

    private bool _volumeApplying;
    private bool _volumeApplyPending;

    /// <summary>Puts the chosen volume and mute on the page, one script at a time.</summary>
    /// <remarks>
    /// A slider drag raises dozens of changes a second. Each used to queue its own page script, so
    /// the page worked through stale levels after the thumb had stopped. While one is running,
    /// later changes only mark the preference dirty, and the latest is sent once it finishes.
    /// </remarks>
    private async Task ApplyPreferredVolumeAsync()
    {
        if (_volumeApplying)
        {
            _volumeApplyPending = true;
            return;
        }

        _volumeApplying = true;
        try
        {
            do
            {
                _volumeApplyPending = false;
                await SendPreferredVolumeAsync();
            }
            while (_volumeApplyPending);
        }
        finally
        {
            _volumeApplying = false;
        }
    }

    private async Task SendPreferredVolumeAsync()
    {
        if (!HasLoadedTrack || _preferredVolume is not { } volume)
        {
            return;
        }

        var level = volume.ToString(System.Globalization.CultureInfo.InvariantCulture);
        var muted = _preferredMuted ? "true" : "false";
        try
        {
            await _view.CoreWebView2.ExecuteScriptAsync($$"""
                (() => {
                  const level = {{level}}, muted = {{muted}};
                  // The primer holds the media element to its preference; tell it the new one
                  // first, or it would put the old level straight back.
                  if (typeof window.goosicSetVolumePreference === 'function') {
                    window.goosicSetVolumePreference(level, muted);
                  }
                  const player = document.getElementById('movie_player');
                  if (player && typeof player.setVolume === 'function') {
                    player.setVolume(Math.round(level * 100));
                    if (muted) player.mute(); else player.unMute();
                    return 'volume-requested';
                  }
                  const media = document.querySelector('audio,video');
                  if (!media) return 'no-media';
                  media.volume = level; media.muted = muted; return 'volume-requested';
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
        _preferredVolume ??= 1;
        _preferredMuted = !_lastMuted;
        await ApplyPreferredVolumeAsync();
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
            NoticeStrayTrack(verdict.Reason);
            return;
        }

        _strayReports = 0;
        if (verdict.Event is { State: "playing", Duration: > 0, IsAdvertisement: false })
        {
            _heardRequested = true;
        }

        if (verdict.Event.State == "paused" && _lastState == "playing")
        {
            _ = NoticeIdlePromptAsync();
        }

        _lastState = verdict.Event.State;

        BridgeLog.Write($"sample accepted seq={verdict.Event.Sequence} state={verdict.Event.State} "
            + $"t={verdict.Event.CurrentTime:F1}/{verdict.Event.Duration:F1} ad={verdict.Event.IsAdvertisement} vol={verdict.Event.Volume:F2}");
        _lastSequence = verdict.Event.Sequence;
        _lastMuted = verdict.Event.Muted;
        KeepPreferredVolume(verdict.Event);
        Sampled?.Invoke(verdict.Event);
        _ = ReportSampleAsync(verdict.Event);
    }

    /// <summary>
    /// Stops the page when it has started a track nobody asked for, and says the requested one is over.
    /// </summary>
    /// <remarks>
    /// When a track ends, YouTube Music's own page moves straight on to a track from its own
    /// automix, so the observer never reports the requested one as ended -- it reports a different
    /// video. Those reports are refused, correctly, but refusing them left the page playing its own
    /// choice without a lease that covered it, and the queue never learned the song had finished.
    ///
    /// The rules only say "describes another video" after the token and generation have both
    /// matched, so the report is from this document under this lease. Two in a row is the page
    /// having moved on rather than a single report racing a navigation. The page is paused at once
    /// -- the track Rust did not approve must not keep sounding -- and the queue decides what plays.
    /// </remarks>
    private void NoticeStrayTrack(string? reason)
    {
        const string prefix = "it describes ";
        if (_movedOnHandled || _videoId.Length == 0 || reason is null
            || !reason.StartsWith(prefix, StringComparison.Ordinal))
        {
            return;
        }

        var described = reason[prefix.Length..].Split(',')[0];
        if (!ShellSupport.IsValidVideoId(described) || described == _videoId || ++_strayReports < 2)
        {
            return;
        }

        _movedOnHandled = true;
        if (!_heardRequested)
        {
            // Nothing of the requested track has played, so this cannot be its end. It is either
            // the same song under another id, or the page giving up on an unplayable one.
            _ = ResolveEarlySwitchAsync(_videoId, described);
            return;
        }

        StopMovedOnPage();
    }

    private async Task ResolveEarlySwitchAsync(string requested, string described)
    {
        var title = "";
        try
        {
            var raw = await _view.CoreWebView2.ExecuteScriptAsync("""
                (() => {
                  const data = document.getElementById('movie_player')?.getVideoData?.();
                  return data && data.video_id ? String(data.title || '') : '';
                })();
                """);
            title = JsonSerializer.Deserialize<string>(raw) ?? "";
        }
        catch (Exception error)
        {
            BridgeLog.Write($"could not read the page's title: {error.GetType().Name}");
        }

        // The page may have been replaced while the title was read.
        if (requested != _videoId)
        {
            return;
        }

        if (IsSubstitute?.Invoke(requested, described, title) == true)
        {
            BridgeLog.Write("the page swapped the requested song for another version of it; keeping it");
            _videoId = described;
            _strayReports = 0;
            _movedOnHandled = false;
            return;
        }

        BridgeLog.Write($"the page replaced a track that never played with {described} (\"{title}\"); treating it as unplayable");
        PageRefused?.Invoke(requested);
        StopMovedOnPage();
    }

    private void StopMovedOnPage()
    {
        BridgeLog.Write("the page moved on to its own track; pausing it and advancing the queue");
        _ = _view.CoreWebView2?.ExecuteScriptAsync("""
            (() => {
              const player = document.getElementById('movie_player');
              if (player && typeof player.pauseVideo === 'function') player.pauseVideo();
              document.querySelectorAll('audio,video').forEach((media) => media.pause());
            })();
            """);
        PageMovedOn?.Invoke(_videoId);
    }

    /// <summary>Restores the chosen volume when a report shows the page has moved away from it.</summary>
    private void KeepPreferredVolume(BridgeEvent sample)
    {
        // An advertisement's volume is left to the page, as on macOS: the listener's level is put
        // back once the track itself is playing.
        if (sample.IsAdvertisement
            || _preferredVolume is not { } volume
            || (Math.Abs(sample.Volume - volume) < 0.02 && sample.Muted == _preferredMuted)
            || DateTime.UtcNow - _lastVolumeFix < TimeSpan.FromMilliseconds(900))
        {
            return;
        }

        _lastVolumeFix = DateTime.UtcNow;
        _ = ApplyPreferredVolumeAsync();
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
