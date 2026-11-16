# Windows lyrics motion port: handoff (November 16, 2026)

## Goal

Port the macOS lyrics motion (commit `d747f8f` on `feature/macos/lyrics-motion`) to Windows, then
ship a release containing both. The spec is `docs/LYRICS_TIMING.md` and
`docs/LYRICS_MOTION_HANDOFF.md` on that macOS branch.

## State of the PRs

- **#154** `feature/lyrics-line-end` → `development`: the shared protocol change (optional
  `LyricsLine.end_ms`). `make test-rust` passes, 317 of 317. Auto-merge is disabled in the repo,
  so merge it by hand once CI is green.
- **#155** `feature/macos/lyrics-motion` → `platform/macos`: the macOS presentation. Merge it by
  hand after #154.
- When development syncs down to `platform/linux`, the GTK shell fails to compile until
  `end_ms: None` is added in `apps/goosic-linux/src/lyrics.rs:174` and `src/shell.rs:3446`.

## Windows work: branch `feature/windows/lyrics-motion` from `platform/windows`, uncommitted

Done, with tests passing (324 of 324 locally on macOS, using a scratch net9.0 project that leaves
out `ShellKeyboard`):

- `Presentation/LyricsTiming.cs`: a port of `LyricsTiming.swift` covering sweeps, rests,
  interludes, reanchor, keepsPresented and ActiveIndex.
- `Presentation/LyricsFollower.cs`: the interpolated clock and the scheduler that changes lines
  on time (`Confirm`, `Advance`, `NextChange`, `Freeze`, `PositionMs`).
- `Presentation/LyricsMotion.cs`: a 50 ms delay per row, a critically damped spring with a
  0.4 s period, padding, the anchor target, and a cover scale of 0.74 while paused.
- `Presentation/LyricsAppearance.cs`: the new numbers for upcoming, past and resting lines.
  Scale and lift are removed, and the signature now takes `resting` and no longer takes
  `immersive`.
- Tests: `LyricsAppearanceTests`, `LyricsTimingTests`, `LyricsFollowerTests` and
  `LyricsMotionTests`.
- `Service/CatalogModels.cs`: the DTO now carries `endMs` and `words`.
- `ViewModels/ShellViewModel.Lyrics.cs` (new): the follower, a `DispatcherQueueTimer`,
  `FollowLyrics(frozen)`, `FreezeLyrics`, `ShowLyrics` and `ResetLyricFollowing`. It also holds
  the new `LyricLineViewModel(lines, index, synced)` with `Timed`, `Shown`, `EmptyInterlude`,
  `InterludeAfter`, `EmptyDotsVisibility` and `AfterDotsVisibility`. The old one was removed from
  `ShellViewModel.cs`.
- `Views/LyricsEffects.cs`: rewritten, but **never compiled**. It handles the per-character sweep
  runs, interlude dots, per-row spring `Follow(animate)`, `Pad()` and `Reset()`. Its new
  constructor is
  `(ScrollViewer, StackPanel content, ItemsControl items, FrameworkElement? intro, Button resume, bool immersive, LyricsFollower, Func<bool> animated, Func<bool> plain, Brush? accent)`.

## Completion update

Items 1–5 below are implemented for 0.4.1. The native port is PR #158;
GTK fixture compatibility is PR #157. #154 and #155 are merged. Shared documentation,
Rust drawing rules and version defaults land on feature/release-0-4-1 from development.
341 Windows presentation tests passed locally. Windows CI is the WinUI compilation gate;
no live Windows visual check has occurred. GTK motion is tracked in parity issue #159.
Release publication and Sparkle feed signing remain the owner's steps.

## Original checklist

1. **XAML** (`MainWindow.xaml`, full player around line 2268, panel around line 1952). Wrap each
   `ItemsControl` in a `StackPanel` with no padding: `FullPlayerLyricsContent` and
   `LyricsContent`. Its first child is the intro dots, a horizontal `StackPanel` with three
   `Ellipse` elements of size fontSize×0.3 and spacing size×0.6, `Opacity="0"`, and
   `Visibility="{x:Bind Model.LyricsIntroductionVisibility, Mode=OneWay}"`. Remove the
   `ItemsControl` padding. In each item template the host `Grid` gets two rows:
   - The `TextBlock` with `Text="{x:Bind Shown}"`.
   - A `StackPanel Tag="empty"` with three ellipses in row 0, using `EmptyDotsVisibility`.
   - A `StackPanel Tag="after"` with three ellipses in row 1, using `AfterDotsVisibility`, with
     height equal to the font size (30 immersive, 22 panel) and `Opacity="0"`.

   Ellipse fill: white in the full player, `GoosicAccentBrush` in the panel.
2. **`MainWindow.Lyrics.cs`**:
   - Construct both `LyricsEffects` with the new constructor, passing `Model.LyricFollower`.
   - Replace the body of `FollowLyricOnScreen(index)` with `Refresh()`, then `Follow(true)` on
     whichever view is visible.
   - Call `Reset()` on both views from `FollowLyricsAgain` and when lyrics load.
   - Remove the `StartBringIntoView` code.
3. **`MainWindow.FullPlayer.cs`**: shrink the cover to `LyricsMotion.PausedCoverScale` while
   paused. Animate the composition `Scale` of the cover `Border` inside `FullPlayerCoverButton`,
   centred, with a spring of damping about 0.85 and a period of about 0.5 s, triggered on
   `Model.IsPlaying` PropertyChanged. With animations off, set it instantly.
4. Check for anything else that used `LyricsAppearance.Line(...)` or `LyricLineLook`'s 4-arg
   constructor, or `new LyricLineViewModel(` (grep).
5. **Docs** (same change): in `docs/LYRICS_TIMING.md`, record that Windows now draws the sweep,
   rests, dots and per-row motion. Also record the Windows differences: no word lift, held-word
   swell or glow, because XAML text runs cannot transform; the sweep is per character rather than
   a soft-edged gradient; and the backdrop already used a black veil. Update `HANDOFF.md`.
6. Build on Windows CI, fix compile errors, and open the PR into `platform/windows` with a Parity
   section. Then do the release, following `docs/RELEASING.md`:
   - Write `docs/RELEASE_0.5.0.md`, Apple-style as AGENTS.md describes (version is the owner's
     call: 0.4.1 or 0.5.0).
   - Run `sh tools/bump-version.sh <v>` on a `feature/` branch from development.
   - Promote `platform/macos` and `platform/windows` into development.
   - Open the deploy PR from development into main and merge it.
   - Tag `v<x.y.z>` and inspect the draft release before publishing.
   - The Sparkle feed signing is the owner's step on the release Mac.

## Risk to check live

The per-row motion relies on `ChangeView(disableAnimation: true)` and the composition
`Translation` landing in the same frame. If a line change flashes for one frame, position the
column by offsets instead. Nothing here has been seen running on Windows.
