using System;
using System.Collections.Generic;
using System.Linq;

namespace Goosic.Windows.Glass;

/// <summary>A rolling window of timings with the numbers the HUD and the benchmark report.</summary>
public sealed class GlassFrameStatistics(int capacity = 240)
{
    private readonly Queue<double> _samples = new();

    public int Count => _samples.Count;

    public void Add(double milliseconds)
    {
        if (double.IsNaN(milliseconds) || milliseconds < 0)
        {
            return;
        }

        _samples.Enqueue(milliseconds);
        while (_samples.Count > capacity)
        {
            _samples.Dequeue();
        }
    }

    public void Clear() => _samples.Clear();

    public double Average => _samples.Count == 0 ? 0 : _samples.Average();

    public double Max => _samples.Count == 0 ? 0 : _samples.Max();

    /// <summary>Nearest-rank percentile, so a p95 is always a frame that really happened.</summary>
    public double Percentile(double percentile)
    {
        if (_samples.Count == 0)
        {
            return 0;
        }

        var ordered = _samples.Order().ToArray();
        var rank = (int)Math.Ceiling(Math.Clamp(percentile, 0, 100) / 100 * ordered.Length);
        return ordered[Math.Clamp(rank - 1, 0, ordered.Length - 1)];
    }

    public double FramesPerSecond => Average > 0 ? 1000 / Average : 0;
}
