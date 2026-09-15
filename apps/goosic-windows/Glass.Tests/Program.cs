using Goosic.Windows.Glass;

static void Check(bool condition, string name)
{
    if (!condition) throw new Exception(name);
    Console.WriteLine($"PASS {name}");
}
var normal = new GlassEnvironment(false, true, true, false, false, true, "Ultra");
Check(GlassQualityPolicy.Select(normal with { HighContrast = true }) == GlassQuality.Fallback, "high contrast overrides Ultra");
Check(GlassQualityPolicy.Select(normal with { TransparencyEffects = false }) == GlassQuality.Fallback, "reduced transparency overrides Ultra");
Check(GlassQualityPolicy.Select(normal with { RendererSupported = false }) == GlassQuality.Fallback, "unsupported renderer is solid");
Check(GlassQualityPolicy.Select(normal with { Override = null, EnergySaver = true }) == GlassQuality.Low, "energy saver selects Low");
Check(GlassGeometry.SignedDistance(0, 0, 50, 20, 20, .6) < 0, "capsule centre is inside");
Check(GlassGeometry.SignedDistance(50, 20, 50, 20, 20, .6) > 0, "capsule corner is outside");
var a = new GlassBounds(0, 0, 100, 100, 12);
var b = new GlassBounds(50, 20, 100, 100, 12);
var stack = GlassGeometry.ResolveStack(new[] { a, b });
Check(stack[0].Count == 0 && stack[1].Single().X == -50, "upper surface receives lower coverage in local coordinates");
var reverse = GlassGeometry.ResolveStack(new[] { b, a });
Check(reverse[1].Single().X == 50, "reversing z order reverses overlap response");
Check(GlassGeometry.ResolveStack(new[] { a, b with { X = 200 } })[1].Count == 0, "disjoint surfaces do not accumulate tint relief");
var stats = new GlassFrameStatistics(100);
for (var i = 1; i <= 100; i++) stats.Add(i);
Check(stats.Percentile(95) == 95, "p95 uses nearest rank");
Check(double.IsFinite(GlassGeometry.PeakRefractionOffset(60, 20)), "capsule refraction stays finite");
