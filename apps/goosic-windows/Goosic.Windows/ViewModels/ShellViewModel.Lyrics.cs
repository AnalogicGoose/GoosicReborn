using System;
using System.Collections.Generic;
using System.Linq;
using Goosic.Windows.Presentation;
using Goosic.Windows.Service;
using Microsoft.UI.Dispatching;

namespace Goosic.Windows.ViewModels;

/// <summary>Which lyric line is current, kept on time between the page's confirmations.</summary>
public sealed partial class ShellViewModel
{
    private readonly LyricsFollower _lyricFollower = new();
    private DispatcherQueueTimer? _lyricTimer;
    private bool _lyricsFrozen = true;
    private LyricsTiming.Interlude? _lyricsIntroduction;

    /// <summary>
    /// Raised with the index of the lyric line drawn as current, or -1 before the first one,
    /// whenever that line or its rest changes.
    /// </summary>
    internal event Action<int>? CurrentLyricChanged;

    internal event Action? LyricsLoaded;

    internal event Action? LyricClockChanged;

    /// <summary>The line drawn as current, or -1 when there is none.</summary>
    internal int CurrentLyricIndex => _lyricFollower.Index;

    /// <summary>The current line has reached its supplied end and the next has not begun.</summary>
    internal bool LyricsResting => _lyricFollower.Resting;

    /// <summary>The clock the lyric views draw by. It only ever draws; it never moves playback.</summary>
    internal LyricsFollower LyricFollower => _lyricFollower;

    /// <summary>The introduction's dots, when the first line begins four seconds or more in.</summary>
    internal LyricsTiming.Interlude? LyricsIntroduction => _lyricsIntroduction;

    public Microsoft.UI.Xaml.Visibility LyricsIntroductionVisibility =>
        _lyricsIntroduction is null ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;

    /// <summary>
    /// Takes the position the page confirmed and redraws the current line if it changed.
    /// </summary>
    /// <remarks>
    /// Only for synced lyrics. Plain lyrics carry no times, and highlighting one of them would
    /// claim a position the document never stated. The position is the one shown, so a pending
    /// seek's target counts, and nothing runs on while the page is paused or in an advertisement.
    /// </remarks>
    private void FollowLyrics(bool frozen)
    {
        _lyricsFrozen = frozen;
        if (!_lyricFollower.Synced || Lyrics.Count == 0)
        {
            return;
        }

        if (_lyricFollower.Confirm(PlaybackPosition, frozen, DateTimeOffset.Now))
        {
            PresentLyricLine();
        }

        LyricClockChanged?.Invoke();
        ScheduleLyricChange();
    }

    private void FreezeLyrics()
    {
        _lyricsFrozen = true;
        _lyricTimer?.Stop();
        _lyricFollower.Freeze(DateTimeOffset.Now);
        LyricClockChanged?.Invoke();
    }

    /// <summary>Waits for the next line start or rest due before the next confirmation.</summary>
    private void ScheduleLyricChange()
    {
        _lyricTimer?.Stop();
        if (_lyricFollower.NextChange is not { } due)
        {
            return;
        }

        if (_lyricTimer is null)
        {
            if (DispatcherQueue.GetForCurrentThread() is not { } queue)
            {
                return;
            }

            _lyricTimer = queue.CreateTimer();
            _lyricTimer.IsRepeating = false;
            _lyricTimer.Tick += (_, _) =>
            {
                if (_lyricFollower.Advance(DateTimeOffset.Now))
                {
                    PresentLyricLine();
                }

                ScheduleLyricChange();
            };
        }

        var wait = due - DateTimeOffset.Now;
        _lyricTimer.Interval = wait > TimeSpan.FromMilliseconds(1) ? wait : TimeSpan.FromMilliseconds(1);
        _lyricTimer.Start();
    }

    private void PresentLyricLine()
    {
        var index = _lyricFollower.Index;
        var resting = _lyricFollower.Resting;
        foreach (var line in Lyrics)
        {
            line.IsCurrent = LyricsAppearance.Lit(line.Index, index, resting);
        }

        CurrentLyricChanged?.Invoke(index);
    }

    /// <summary>Forgets the lyrics and the line they were on, as for a new track.</summary>
    private void ResetLyricFollowing()
    {
        _lyricTimer?.Stop();
        Lyrics.Clear();
        _lyricFollower.Clear();
        SetLyricsIntroduction(null);
    }

    /// <summary>Shows a found document and starts following it from the position already confirmed.</summary>
    private void ShowLyrics(LyricsDocument document)
    {
        var timed = document.Lines
            .Select(line => new TimedLine(line.AtMilliseconds, line.EndMilliseconds, line.Text,
                line.Words.Select(word => new TimedWord(word.AtMilliseconds, word.EndMilliseconds, word.Text)).ToArray()))
            .ToArray();
        for (var index = 0; index < timed.Length; index++)
        {
            Lyrics.Add(new LyricLineViewModel(timed, index, document.Synced));
        }

        _lyricFollower.Load(timed, document.Synced);
        SetLyricsIntroduction(LyricsTiming.Introduction(timed, document.Synced));
        LyricsLoaded?.Invoke();
        FollowLyrics(_lyricsFrozen);
    }

    private void SetLyricsIntroduction(LyricsTiming.Interlude? introduction)
    {
        _lyricsIntroduction = introduction;
        OnPropertyChanged(nameof(LyricsIntroductionVisibility));
    }
}

/// <summary>One line of lyrics, and whether it is the one being sung.</summary>
/// <remarks>Never synthesized by the shell: every line, word and time comes from the provider.</remarks>
public sealed class LyricLineViewModel : System.ComponentModel.INotifyPropertyChanged
{
    private bool _isCurrent;

    internal LyricLineViewModel(IReadOnlyList<TimedLine> lines, int index, bool synced)
    {
        Timed = lines[index];
        Index = index;
        Synced = synced;
        EmptyInterlude = LyricsTiming.EmptyLine(lines, synced, index);
        InterludeAfter = LyricsTiming.InterludeAfter(lines, synced, index);
    }

    public event System.ComponentModel.PropertyChangedEventHandler? PropertyChanged;

    /// <summary>The line as the timing rules see it.</summary>
    public TimedLine Timed { get; }

    /// <summary>The line's place in its document, which is what its distance from the sung line is measured in.</summary>
    public int Index { get; }

    /// <summary>Whether the document follows the song; unsynced lines have no current line.</summary>
    public bool Synced { get; }

    public string Text => Timed.Text;

    /// <summary>What is drawn: an empty line is a musical note, unless it stands for an interlude.</summary>
    public string Shown => EmptyInterlude is not null ? "" : Text.Length == 0 ? "♪" : Text;

    public long AtMilliseconds => Timed.AtMs;

    /// <summary>The stretch an empty line stands for, drawn as dots in its place.</summary>
    internal LyricsTiming.Interlude? EmptyInterlude { get; }

    /// <summary>The rest after this line, drawn as dots beneath it.</summary>
    internal LyricsTiming.Interlude? InterludeAfter { get; }

    public Microsoft.UI.Xaml.Visibility EmptyDotsVisibility =>
        EmptyInterlude is null ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;

    public Microsoft.UI.Xaml.Visibility AfterDotsVisibility =>
        InterludeAfter is null ? Microsoft.UI.Xaml.Visibility.Collapsed : Microsoft.UI.Xaml.Visibility.Visible;

    /// <summary>Whether this line is being sung: current, and not at rest.</summary>
    public bool IsCurrent
    {
        get => _isCurrent;
        internal set
        {
            if (_isCurrent == value)
            {
                return;
            }

            _isCurrent = value;
            PropertyChanged?.Invoke(this, new System.ComponentModel.PropertyChangedEventArgs(nameof(IsCurrent)));
        }
    }
}
