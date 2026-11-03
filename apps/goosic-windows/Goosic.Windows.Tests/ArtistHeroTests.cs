using System;
using System.Linq;
using Goosic.Windows.Presentation;
using Xunit;

namespace Goosic.Windows.Tests;

/// <summary>The same cases as <c>goosic-shell-support</c>'s <c>hero</c> tests and <c>ArtistHeroTests.swift</c>.</summary>
public sealed class ArtistHeroTests
{
    private static byte[] Bitmap(int width, int height, Func<int, byte[]> pixel) =>
        Enumerable.Range(0, width * height).SelectMany(index => pixel(index / width).Append((byte)255)).ToArray();

    [Fact]
    public void TheHeroFollowsTheBannerBetweenAFloorAndACeiling()
    {
        Assert.Equal(500.0, ArtistHero.Height(1200));
        Assert.Equal(ArtistHero.MinHeight, ArtistHero.Height(600));
        Assert.Equal(ArtistHero.MaxHeight, ArtistHero.Height(2400));
    }

    [Fact]
    public void ADarkBannerKeepsItsOwnColour()
    {
        var navy = Bitmap(4, 4, _ => [20, 30, 60]);
        Assert.Equal(new byte[] { 20, 30, 60 }, ArtistHero.Tint(navy, 4, 4));
    }

    [Fact]
    public void ABrightBannerIsDarkenedUntilWhiteTextReadsOverIt()
    {
        var white = Bitmap(4, 4, _ => [255, 255, 255]);
        Assert.Equal(new byte[] { 118, 118, 118 }, ArtistHero.Tint(white, 4, 4));
    }

    [Fact]
    public void OnlyTheFootOfTheBannerIsSampled()
    {
        var banner = Bitmap(4, 8, row => row < 6 ? [255, 255, 255] : [0, 0, 0]);
        Assert.Equal(new byte[] { 0, 0, 0 }, ArtistHero.Tint(banner, 4, 8));
    }

    [Fact]
    public void ABitmapShorterThanItClaimsHasNoTint()
    {
        Assert.Null(ArtistHero.Tint(new byte[12], 2, 2));
        Assert.Null(ArtistHero.Tint([], 0, 0));
    }

    [Fact]
    public void TheBannerBlursByHalfWayAndHasGoneWithTheHero()
    {
        Assert.Equal(0.0, ArtistHero.Progress(-40, 500));
        Assert.Equal(0.5, ArtistHero.Progress(250, 500));
        Assert.Equal(1.0, ArtistHero.Progress(900, 500));

        Assert.Equal(0.0, ArtistHero.Blur(0, transparencyEffects: true));
        Assert.Equal(ArtistHero.MaxBlur, ArtistHero.Blur(0.5, transparencyEffects: true));
        Assert.Equal(0.0, ArtistHero.Blur(1, transparencyEffects: false));

        Assert.Equal(1.0, ArtistHero.PictureOpacity(0.3));
        Assert.Equal(0.5, ArtistHero.PictureOpacity(0.6), 6);
        Assert.Equal(0.0, ArtistHero.PictureOpacity(0.9));
    }
}
