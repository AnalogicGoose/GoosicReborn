using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class FullPlayerLayoutTests
{
    [Theory]
    [InlineData(1400, true, FullPlayerMode.Split)]
    [InlineData(900, true, FullPlayerMode.Split)]
    [InlineData(899, true, FullPlayerMode.Lyrics)]
    [InlineData(1400, false, FullPlayerMode.Cover)]
    [InlineData(480, false, FullPlayerMode.Cover)]
    public void WidthAndToggleChooseTheArrangement(double width, bool lyrics, FullPlayerMode expected) =>
        Assert.Equal(expected, FullPlayerLayout.Mode(width, lyrics));

    [Fact]
    public void PaddingShrinksWithTheWindow()
    {
        Assert.Equal((20d, 112d, 24d), FullPlayerLayout.Padding(480));
        Assert.Equal((36d, 112d, 40d), FullPlayerLayout.Padding(700));
        Assert.Equal((72d, 56d, 72d), FullPlayerLayout.Padding(1400));
    }

    [Theory]
    [InlineData(FullPlayerMode.Split, 64)]
    [InlineData(FullPlayerMode.Lyrics, 0)]
    public void OnlySplitLyricsNeedTheirOwnClearance(FullPlayerMode mode, double margin) =>
        Assert.Equal(margin, FullPlayerLayout.LyricsTopMargin(mode));

    [Theory]
    [InlineData(true, true, true)]
    [InlineData(false, true, false)]
    [InlineData(true, false, false)]
    public void BlurNeedsTransparencyAndACover(bool effects, bool artwork, bool expected) =>
        Assert.Equal(expected, FullPlayerLayout.ShowsBlur(effects, artwork));

    [Theory]
    [InlineData(SeekKey.Back, 60, 55)]
    [InlineData(SeekKey.Forward, 60, 65)]
    [InlineData(SeekKey.PageBack, 60, 40)]
    [InlineData(SeekKey.PageForward, 60, 80)]
    [InlineData(SeekKey.Start, 60, 0)]
    [InlineData(SeekKey.End, 60, 200)]
    [InlineData(SeekKey.Back, 2, 0)]
    [InlineData(SeekKey.Forward, 198, 200)]
    public void SliderKeysSeekInsideTheTrack(SeekKey key, double position, double expected) =>
        Assert.Equal(expected, FullPlayerLayout.KeySeek(key, position, duration: 200));

    [Fact]
    public void NothingSeeksWithoutADurationOrASeekKey()
    {
        Assert.Null(FullPlayerLayout.KeySeek(SeekKey.Forward, 0, 0));
        Assert.Null(FullPlayerLayout.KeySeek(SeekKey.None, 10, 200));
    }
}
