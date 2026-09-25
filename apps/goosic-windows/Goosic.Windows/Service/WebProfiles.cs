using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Web.WebView2.Core;

namespace Goosic.Windows.Service;

/// <summary>
/// The WebView2 profiles: one for browsing as a guest, one per signed-in account.
/// </summary>
/// <remarks>
/// <para>
/// This is the Windows form of the WebKit data stores macOS keys by profile identifier. Every
/// WebView2 in the process shares one environment rooted under the user's Goosic data, and each
/// is opened into a named profile inside it, so an account's cookies live in that profile's own
/// storage and nowhere else. Nothing in this class reads a cookie; it only decides which jar a
/// surface is opened into.
/// </para>
/// <para>
/// A profile's name is derived from the <c>webkitProfileId</c> Rust stores for the account -- a
/// UUID generated before the sign-in window opened, never from anything the provider said -- so
/// the same account always reopens the same storage, and two accounts cannot share one.
/// </para>
/// </remarks>
internal static class WebProfiles
{
    internal const string GuestProfile = "guest";

    private static readonly SemaphoreSlim Gate = new(1, 1);
    private static CoreWebView2Environment? _environment;

    /// <summary>The profile name for an account's web profile id, or the guest profile.</summary>
    internal static string NameFor(string? webProfileId) =>
        Guid.TryParse(webProfileId, out var id) ? "account-" + id.ToString("N") : GuestProfile;

    /// <summary>The shared environment, created on first use.</summary>
    internal static async Task<CoreWebView2Environment> EnvironmentAsync()
    {
        if (_environment is not null)
        {
            return _environment;
        }

        await Gate.WaitAsync().ConfigureAwait(true);
        try
        {
            if (_environment is null)
            {
                // Under the user's own data rather than beside the executable, which is the
                // unpackaged default and the wrong place once the app is installed read-only.
                var root = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "Goosic", "WebView2", "profiles");
                Directory.CreateDirectory(root);
                _environment = await CoreWebView2Environment.CreateWithOptionsAsync(null, root, null);
            }

            return _environment;
        }
        finally
        {
            Gate.Release();
        }
    }

    /// <summary>Options that open a surface into <paramref name="profileName"/>.</summary>
    internal static CoreWebView2ControllerOptions OptionsFor(CoreWebView2Environment environment, string profileName)
    {
        var options = environment.CreateCoreWebView2ControllerOptions();
        options.ProfileName = profileName;
        options.IsInPrivateModeEnabled = false;
        return options;
    }

    /// <summary>Whether a URL is on the official YouTube Music host or one of its subdomains.</summary>
    internal static bool IsOfficialNavigation(string value) =>
        Uri.TryCreate(value, UriKind.Absolute, out var target)
        && target.Scheme == Uri.UriSchemeHttps
        && (target.Host == ShellSupport.AllowedHost
            || target.Host.EndsWith("." + ShellSupport.AllowedHost, StringComparison.Ordinal));

    /// <summary>
    /// Runs an async expression in the page's main world and returns its string result.
    /// </summary>
    /// <remarks>
    /// <c>ExecuteScriptAsync</c> cannot await a promise, and both the sign-in marker and the
    /// personal catalog have to wait on the page. The DevTools protocol's evaluate can, and it
    /// returns the value rather than a serialised promise.
    /// </remarks>
    internal static async Task<string?> EvaluateAsync(CoreWebView2 core, string expression)
    {
        var request = new System.Text.Json.Nodes.JsonObject
        {
            ["expression"] = expression,
            ["awaitPromise"] = true,
            ["returnByValue"] = true,
        };
        var raw = await core.CallDevToolsProtocolMethodAsync("Runtime.evaluate", request.ToJsonString());
        var answer = System.Text.Json.Nodes.JsonNode.Parse(raw);
        if (answer?["exceptionDetails"] is { } failure)
        {
            var message = failure["exception"]?["description"]?.GetValue<string>()
                ?? failure["text"]?.GetValue<string>()
                ?? "the page script failed";
            throw new InvalidOperationException(message.Split('\n')[0]);
        }

        var result = answer?["result"];
        return result?["type"]?.GetValue<string>() == "string" ? result["value"]?.GetValue<string>() : null;
    }
}
