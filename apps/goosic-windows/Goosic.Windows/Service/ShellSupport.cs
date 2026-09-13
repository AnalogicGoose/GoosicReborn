using System;
using System.Runtime.InteropServices;

namespace Goosic.Windows.Service;

/// <summary>
/// The platform-neutral rules, reached through <c>goosic-shell-support-ffi</c>.
/// </summary>
/// <remarks>
/// These are not reimplemented here on purpose. <c>SHELL_CONTRACT.md</c> is explicit that a rule
/// with three copies has three answers as soon as one of them is edited, and what crosses this
/// boundary is the security-sensitive half: the JavaScript injected into the official page, and
/// the checks that decide whether an event from it may be believed. A C# restatement of those
/// would be a second place to get them subtly wrong.
///
/// Every string the native side returns is owned by this process and freed here.
/// </remarks>
internal static class ShellSupport
{
    private const string Library = "goosic_shell_support_ffi";

    [DllImport(Library, EntryPoint = "goosic_string_free")]
    private static extern void StringFree(IntPtr value);

    [DllImport(Library, EntryPoint = "goosic_bridge_allowed_host")]
    private static extern IntPtr AllowedHostRaw();

    [DllImport(Library, EntryPoint = "goosic_bridge_handler_name")]
    private static extern IntPtr HandlerNameRaw();

    [DllImport(Library, EntryPoint = "goosic_bridge_event_version")]
    private static extern long EventVersionRaw();

    [DllImport(Library, EntryPoint = "goosic_bridge_max_body_bytes")]
    private static extern nuint MaxBodyBytesRaw();

    [DllImport(Library, EntryPoint = "goosic_bridge_is_valid_video_id")]
    [return: MarshalAs(UnmanagedType.I1)]
    private static extern bool IsValidVideoIdRaw([MarshalAs(UnmanagedType.LPUTF8Str)] string videoId);

    [DllImport(Library, EntryPoint = "goosic_bridge_observer_script")]
    private static extern IntPtr ObserverScriptRaw(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string token,
        ulong generation,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string videoId);

    [DllImport(Library, EntryPoint = "goosic_bridge_media_session_guard_script")]
    private static extern IntPtr MediaSessionGuardScriptRaw();

    [DllImport(Library, EntryPoint = "goosic_bridge_js_string_literal")]
    private static extern IntPtr JsStringLiteralRaw([MarshalAs(UnmanagedType.LPUTF8Str)] string value);

    /// <summary>Takes ownership of a native string and frees it.</summary>
    private static string Consume(IntPtr value)
    {
        if (value == IntPtr.Zero)
        {
            throw new InvalidOperationException("goosic-shell-support returned no value");
        }

        try
        {
            return Marshal.PtrToStringUTF8(value) ?? "";
        }
        finally
        {
            StringFree(value);
        }
    }

    /// <summary>The only host the official playback surface may load.</summary>
    internal static string AllowedHost { get; } = Consume(AllowedHostRaw());

    /// <summary>The name the page posts bridge messages to.</summary>
    internal static string HandlerName { get; } = Consume(HandlerNameRaw());

    /// <summary>The bridge event version this build speaks.</summary>
    internal static long EventVersion { get; } = EventVersionRaw();

    /// <summary>The largest bridge message body that will be read.</summary>
    internal static int MaxBodyBytes { get; } = checked((int)MaxBodyBytesRaw());

    /// <summary>Whether a value is shaped like a YouTube video id.</summary>
    internal static bool IsValidVideoId(string videoId) => IsValidVideoIdRaw(videoId);

    /// <summary>The per-load page observer, for one lease generation and one requested track.</summary>
    internal static string ObserverScript(string token, ulong generation, string videoId) =>
        Consume(ObserverScriptRaw(token, generation, videoId));

    /// <summary>The script that stops the page installing its own media-session handlers.</summary>
    internal static string MediaSessionGuardScript() => Consume(MediaSessionGuardScriptRaw());

    /// <summary>Encodes a value as a JavaScript string literal.</summary>
    internal static string JsStringLiteral(string value) => Consume(JsStringLiteralRaw(value));
}
