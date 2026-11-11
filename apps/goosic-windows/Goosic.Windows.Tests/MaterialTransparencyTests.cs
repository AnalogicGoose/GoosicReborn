using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

public sealed class MaterialTransparencyTests
{
    [Theory]
    [InlineData(0.78)]
    [InlineData(0.6)]
    [InlineData(0.55)]
    public void TheDefaultIsTheMaterialAsDesigned(double designed) =>
        Assert.Equal(designed, MaterialTransparency.Opacity(designed, MaterialTransparency.Default), 6);

    [Fact]
    public void NoTransparencyIsSolid() =>
        Assert.Equal(1.0, MaterialTransparency.Opacity(0.78, MaterialTransparency.Minimum), 6);

    [Fact]
    public void FullTransparencyStillKeepsSomeTint()
    {
        var clearest = MaterialTransparency.Opacity(0.78, MaterialTransparency.Maximum);
        Assert.Equal(0.78 * MaterialTransparency.ClearestFraction, clearest, 6);
        Assert.True(clearest > 0, "labels must never sit over bare album art");
    }

    [Fact]
    public void MoreTransparencyNeverMakesTheTintDenser()
    {
        var previous = double.MaxValue;
        for (var t = MaterialTransparency.Minimum; t <= MaterialTransparency.Maximum; t++)
        {
            var opacity = MaterialTransparency.Opacity(0.6, t);
            Assert.True(opacity <= previous, $"opacity rose at {t}");
            previous = opacity;
        }
    }

    [Theory]
    [InlineData(null, MaterialTransparency.Default)]
    [InlineData(-20, MaterialTransparency.Minimum)]
    [InlineData(250, MaterialTransparency.Maximum)]
    [InlineData(70, 70)]
    public void StoredValuesAreBroughtBackIntoRange(int? stored, int expected) =>
        Assert.Equal(expected, MaterialTransparency.Sanitize(stored));

    [Theory]
    [InlineData(0, "Solid")]
    [InlineData(50, "Default")]
    [InlineData(80, "80%")]
    public void TheValueReadsAsWords(int transparency, string expected) =>
        Assert.Equal(expected, MaterialTransparency.Label(transparency));
}
