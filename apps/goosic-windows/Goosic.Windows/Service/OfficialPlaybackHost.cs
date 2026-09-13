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
    private string? _observerId;

    internal OfficialPlaybackHost(WebView2 view, GoosicServiceClient client)
    {
        _view = view;
        _client = client;
    }

    /// <summary>Raised when the page reports something the rules accepted.</summary>
    internal event Action<BridgeEvent>? Sampled;

    /// <summary>Raised with anything worth saying on screen.</summary>
    internal event Action<string>? Status;

    /// <summary>
    /// Claims the lease and loads a track.
    /// </summary>
    /// <remarks>
    /// The claim comes first. A renderer that started producing sound before Rust agreed would
    /// have escaped the authority, which is the one thing every shell has to get right.
    /// </remarks>
    internal async Task PlayAsync(string videoId)
    {
        if (!ShellSupport.IsValidVideoId(videoId))
        {
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

        await EnsureReadyAsync().ConfigureAwait(true);

        // One observer at a time. Each carries the token and generation of its own load, so
        // leaving the previous one installed would have every page reporting once per track ever
        // played, each rejected for coming from a superseded document.
        if (_observerId is { } previous)
        {
            _view.CoreWebView2.RemoveScriptToExecuteOnDocumentCreated(previous);
        }

        var script = ShellSupport.ObserverScript(_token, _generation, videoId);
        _observerId = await _view.CoreWebView2.AddScriptToExecuteOnDocumentCreatedAsync(script);
        _view.CoreWebView2.Navigate($"https://{ShellSupport.AllowedHost}/watch?v={Uri.EscapeDataString(videoId)}");
    }

    private async Task EnsureReadyAsync()
    {
        if (_ready)
        {
            return;
        }

        await _view.EnsureCoreWebView2Async();
        var core = _view.CoreWebView2;

        // The page must not install its own media-session handlers: the system transport belongs
        // to this shell, and a page that answered the media keys would be acting without Rust.
        await core.AddScriptToExecuteOnDocumentCreatedAsync(ShellSupport.MediaSessionGuardScript());

        // The shared observer posts through `window.webkit.messageHandlers`, because WebKit is
        // where it was first needed. WebView2 has no such object, so the script would return on
        // its first line and report nothing at all.
        //
        // The adaptation belongs here rather than in the observer: making the shared script
        // choose an engine at runtime would edit security-sensitive generated JavaScript for a
        // difference only this platform has. The shim forwards and does nothing else -- the
        // payload is unchanged, and it is stringified because WebView2 delivers an object as
        // JSON while the host reads a string.
        var handler = ShellSupport.JsStringLiteral(ShellSupport.HandlerName);
        await core.AddScriptToExecuteOnDocumentCreatedAsync($$"""
            (() => {
              const name = {{handler}};
              if (window.webkit?.messageHandlers?.[name]) return;
              window.webkit = window.webkit || {};
              window.webkit.messageHandlers = window.webkit.messageHandlers || {};
              window.webkit.messageHandlers[name] = {
                postMessage: (payload) => window.chrome.webview.postMessage(JSON.stringify(payload)),
              };
            })();
            """);

        core.WebMessageReceived += OnWebMessage;

        // Loading the official player is the slowest part of starting a track, and a shell that
        // says nothing during it looks broken rather than busy.
        core.NavigationCompleted += (_, args) =>
        {
            if (!args.IsSuccess)
            {
                Status?.Invoke($"The official player did not load ({args.WebErrorStatus}).");
            }
        };

        // Only the official host may run in here. A navigation anywhere else is cancelled rather
        // than followed, so a redirect cannot turn this into a general browser.
        core.NavigationStarting += (_, args) =>
        {
            if (!Uri.TryCreate(args.Uri, UriKind.Absolute, out var target)
                || target.Scheme != Uri.UriSchemeHttps
                || !(target.Host == ShellSupport.AllowedHost
                     || target.Host.EndsWith("." + ShellSupport.AllowedHost, StringComparison.Ordinal)))
            {
                args.Cancel = true;
            }
        };

        _ready = true;
    }

    private void OnWebMessage(CoreWebView2 sender, CoreWebView2WebMessageReceivedEventArgs args)
    {
        string body;
        try
        {
            body = args.TryGetWebMessageAsString();
        }
        catch (ArgumentException)
        {
            // Not a string message, so not one of ours.
            return;
        }

        if (body.Length > ShellSupport.MaxBodyBytes)
        {
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
            System.Diagnostics.Debug.WriteLine($"[bridge] rejected: {verdict.Reason}");
            return;
        }

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
