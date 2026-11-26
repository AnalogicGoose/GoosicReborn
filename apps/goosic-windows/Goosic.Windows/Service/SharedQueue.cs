using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text.Json.Nodes;

namespace Goosic.Windows.Service;

/// <summary>
/// The listening queue, decided by <c>QueueEngine</c> in <c>goosic-shell-support</c>.
/// </summary>
/// <remarks>
/// Windows and macOS each used to keep their own queue rules, and they drifted: shuffle, the end
/// of a list, autoplay and where a chosen song starts all behaved differently on the two. The
/// rules now live once, in Rust, and this type is only the channel to them: a command goes in as
/// JSON, and what comes back is what to do and what the queue now is. It holds no rule of its own.
/// </remarks>
internal sealed class SharedQueue
{
    private const string Library = "goosic_shell_support_ffi";

    private sealed class QueueHandle : SafeHandle
    {
        public QueueHandle() : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        protected override bool ReleaseHandle()
        {
            QueueFree(handle);
            return true;
        }
    }

    [DllImport(Library, EntryPoint = "goosic_queue_new")]
    private static extern QueueHandle QueueNew();

    [DllImport(Library, EntryPoint = "goosic_queue_free")]
    private static extern void QueueFree(IntPtr queue);

    [DllImport(Library, EntryPoint = "goosic_queue_handle")]
    private static extern IntPtr QueueHandleRaw(QueueHandle queue, [MarshalAs(UnmanagedType.LPUTF8Str)] string command);

    [DllImport(Library, EntryPoint = "goosic_rules_call")]
    private static extern IntPtr RulesCallRaw(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string input);

    [DllImport(Library, EntryPoint = "goosic_string_free")]
    private static extern void StringFree(IntPtr value);

    private readonly QueueHandle _queue = QueueNew();

    /// <summary>Applies one command and returns what the engine decided.</summary>
    /// <exception cref="InvalidOperationException">The engine could not read the command, which is a bug here.</exception>
    internal QueueOutcome Handle(JsonObject command)
    {
        var answer = Consume(QueueHandleRaw(_queue, command.ToJsonString()));
        if (answer["ok"] is JsonObject ok)
        {
            return QueueOutcome.Parse(ok);
        }

        throw new InvalidOperationException("queue command refused: " + (answer["error"]?.GetValue<string>() ?? "no answer"));
    }

    /// <summary>Answers a stateless rule by name, or null when the rule refused the arguments.</summary>
    internal static JsonNode? Rule(string name, JsonObject arguments) =>
        Consume(RulesCallRaw(name, arguments.ToJsonString()))["ok"];

    /// <summary>The sleep timer's menu label.</summary>
    internal static string SleepTimerLabel(TimeSpan? left, bool endOfSong)
    {
        var arguments = new JsonObject { ["endOfSong"] = endOfSong };
        if (left is { } remaining)
        {
            arguments["remainingSeconds"] = remaining.TotalSeconds;
        }

        return Rule("sleepTimer", arguments)?.GetValue<string>() ?? "Sleep timer";
    }

    /// <summary>"100 songs", or "100+ songs" while more are still to come.</summary>
    internal static string SongCount(int count, bool hasMore) =>
        Rule("songCount", new JsonObject { ["count"] = count, ["hasMore"] = hasMore })?.GetValue<string>()
        ?? $"{count} songs";

    /// <summary>Whether a song chosen on <paramref name="surface"/> starts its list in order or a station.</summary>
    internal static string LaunchKind(string surface) =>
        Rule("launchKind", new JsonObject { ["surface"] = surface })?.GetValue<string>() ?? "station";

    private static JsonObject Consume(IntPtr value)
    {
        if (value == IntPtr.Zero)
        {
            throw new InvalidOperationException("goosic-shell-support returned no value");
        }

        try
        {
            return JsonNode.Parse(Marshal.PtrToStringUTF8(value) ?? "{}") as JsonObject ?? new JsonObject();
        }
        finally
        {
            StringFree(value);
        }
    }

    /// <summary>A song as the engine needs it: what to load and what to call it.</summary>
    internal static JsonObject Track(string videoId, string title) =>
        new() { ["videoId"] = videoId, ["title"] = title };
}

/// <summary>What the shell should do now, in order.</summary>
internal abstract record QueueEffect
{
    internal static QueueEffect Parse(JsonObject effect) => effect["type"]?.GetValue<string>() switch
    {
        "play" => new PlayEffect(effect["entry"]!.GetValue<ulong>()),
        "seekToStart" => new SeekToStartEffect(),
        "fetchRadio" => new FetchRadioEffect(
            effect["request"]!.GetValue<ulong>(),
            effect["seedVideoId"]!.GetValue<string>(),
            effect["continuation"]?.GetValue<string>(),
            effect["accountId"]?.GetValue<string>()),
        "stopForSleep" => new StopForSleepEffect(),
        "savePreferences" => new SavePreferencesEffect(
            effect["shuffle"]!.GetValue<bool>(), effect["repeat"]!.GetValue<string>()),
        "notice" => new NoticeEffect(effect["text"]!.GetValue<string>()),
        _ => new UnknownEffect(),
    };
}

internal sealed record PlayEffect(ulong Entry) : QueueEffect;

internal sealed record SeekToStartEffect : QueueEffect;

internal sealed record FetchRadioEffect(ulong Request, string SeedVideoId, string? Continuation, string? AccountId) : QueueEffect;

internal sealed record StopForSleepEffect : QueueEffect;

internal sealed record SavePreferencesEffect(bool Shuffle, string Repeat) : QueueEffect;

internal sealed record NoticeEffect(string Text) : QueueEffect;

internal sealed record UnknownEffect : QueueEffect;

/// <summary>The queue as the shell draws it.</summary>
internal sealed record QueueSnapshot(
    IReadOnlyList<ulong> Entries,
    int? Current,
    string? SourceTitle,
    string? SourceKey,
    bool Shuffle,
    string Repeat,
    bool Autoplay,
    bool SleepAtEnd,
    bool Radio,
    bool RadioLoading,
    int UpNextCount,
    string UpNextText,
    bool CanClear,
    double? UndoDeadline,
    ulong Revision)
{
    internal static QueueSnapshot Parse(JsonObject snapshot)
    {
        var entries = new List<ulong>();
        foreach (var entry in snapshot["entries"] as JsonArray ?? new JsonArray())
        {
            entries.Add(entry!.GetValue<ulong>());
        }

        return new QueueSnapshot(
            entries,
            snapshot["current"]?.GetValue<int>(),
            snapshot["sourceTitle"]?.GetValue<string>(),
            snapshot["sourceKey"]?.GetValue<string>(),
            snapshot["shuffle"]?.GetValue<bool>() ?? false,
            snapshot["repeat"]?.GetValue<string>() ?? "off",
            snapshot["autoplay"]?.GetValue<bool>() ?? true,
            snapshot["sleepAtEnd"]?.GetValue<bool>() ?? false,
            snapshot["radio"]?.GetValue<bool>() ?? false,
            snapshot["radioLoading"]?.GetValue<bool>() ?? false,
            snapshot["upNextCount"]?.GetValue<int>() ?? 0,
            snapshot["upNextText"]?.GetValue<string>() ?? "",
            snapshot["canClear"]?.GetValue<bool>() ?? false,
            snapshot["undoDeadline"]?.GetValue<double>(),
            snapshot["revision"]?.GetValue<ulong>() ?? 0);
    }
}

/// <summary>An entry the engine created from the command's track at position <see cref="Input"/>.</summary>
internal readonly record struct QueueAdded(int Input, ulong Entry);

/// <summary>What one command produced.</summary>
internal sealed record QueueOutcome(IReadOnlyList<QueueEffect> Effects, IReadOnlyList<QueueAdded> Added, QueueSnapshot Snapshot)
{
    internal static QueueOutcome Parse(JsonObject outcome)
    {
        var effects = new List<QueueEffect>();
        foreach (var effect in outcome["effects"] as JsonArray ?? new JsonArray())
        {
            if (effect is JsonObject value)
            {
                effects.Add(QueueEffect.Parse(value));
            }
        }

        var added = new List<QueueAdded>();
        foreach (var item in outcome["added"] as JsonArray ?? new JsonArray())
        {
            if (item is JsonObject value)
            {
                added.Add(new QueueAdded(value["input"]!.GetValue<int>(), value["entry"]!.GetValue<ulong>()));
            }
        }

        return new QueueOutcome(effects, added, QueueSnapshot.Parse(outcome["snapshot"] as JsonObject ?? new JsonObject()));
    }
}
