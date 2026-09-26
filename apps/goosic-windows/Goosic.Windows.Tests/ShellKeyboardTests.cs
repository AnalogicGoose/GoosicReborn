using System;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public class ShellKeyboardTests
{
    [Fact]
    public void NoTwoShortcutsShareAChord()
    {
        var chords = ShellKeyboard.Bindings.Select(b => (b.Key, b.Modifiers)).ToList();
        Assert.Equal(chords.Count, chords.Distinct().Count());
    }

    [Fact]
    public void EveryCommandHasAShortcut()
    {
        var bound = ShellKeyboard.Bindings.Select(b => b.Command).ToHashSet();
        Assert.All(System.Enum.GetValues<ShellCommand>(), command => Assert.Contains(command, bound));
    }

    [Theory]
    [InlineData(SpaceTarget.Other, true)]
    [InlineData(SpaceTarget.TextInput, false)]
    [InlineData(SpaceTarget.Control, false)]
    public void SpaceOnlyTogglesWhereItMeansNothingElse(SpaceTarget focus, bool toggles) =>
        Assert.Equal(toggles, ShellKeyboard.SpaceTogglesPlayback(focus));

    [Theory]
    [InlineData(5, 200, -10, 0)]
    [InlineData(195, 200, 10, 200)]
    [InlineData(50, 200, 10, 60)]
    public void SeekingStaysInsideTheTrack(double position, double duration, double delta, double expected) =>
        Assert.Equal(expected, ShellKeyboard.SeekTarget(position, duration, delta, seekable: true));

    [Fact]
    public void NothingSeeksWithoutASeekableTrack()
    {
        Assert.Null(ShellKeyboard.SeekTarget(0, 200, 10, seekable: false));
        Assert.Null(ShellKeyboard.SeekTarget(0, 0, 10, seekable: true));
    }

    [Theory]
    [InlineData(0.98, 0.05, 1.0)]
    [InlineData(0.0001, -0.05, 0.0)]
    [InlineData(0.125, 0.05, 0.166375)]
    public void VolumeStaysInRange(double volume, double delta, double expected) =>
        Assert.Equal(expected, ShellKeyboard.NudgedVolume(volume, delta), precision: 10);

    [Theory]
    [InlineData(0.0, 0.0)]
    [InlineData(0.5, 0.125)]
    [InlineData(1.0, 1.0)]
    [InlineData(1.5, 1.0)]
    public void SliderFollowsAnAudioTaper(double position, double gain)
    {
        Assert.Equal(gain, VolumeTaper.ToGain(position), precision: 10);
        Assert.Equal(Math.Clamp(position, 0, 1), VolumeTaper.ToPosition(gain), precision: 10);
    }
}
