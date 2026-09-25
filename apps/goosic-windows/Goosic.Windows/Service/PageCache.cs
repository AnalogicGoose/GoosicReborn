using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;

namespace Goosic.Windows.Service;

/// <summary>
/// The last copy of each page seen, so opening it again is instant.
/// </summary>
/// <remarks>
/// <para>
/// Apple Music and Spotify open a page they have shown before immediately and bring it up to date
/// behind it. Reborn fetched every page afresh on every visit, which cost from a fifth of a second
/// to over a second each time, and several seconds whenever the account's reader page had to be
/// started first. The shell now shows the copy kept here, asks for a fresh one, and replaces the
/// copy on screen only if the answer differs.
/// </para>
/// <para>
/// Only the first part of a page is kept; what loads as the reader scrolls is fetched each time.
/// The copies are the same small catalog shape the page was drawn from -- titles, ids, artwork
/// addresses and durations -- and never credentials. They are kept on disk per account profile
/// under <c>%LOCALAPPDATA%\Goosic\cache</c>, so the first page after a restart is instant too, and
/// a profile's copies are deleted when it signs out.
/// </para>
/// </remarks>
internal sealed class PageCache
{
    private const int MaxEntries = 64;

    private static readonly string Root = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Goosic", "cache");

    private readonly Dictionary<string, string> _memory = new(StringComparer.Ordinal);
    private readonly LinkedList<string> _order = new();

    /// <summary>The copy of <paramref name="key"/>, from memory or disk, or <c>null</c>.</summary>
    internal CatalogPage? Get(string scope, string key)
    {
        var full = scope + "/" + key;
        if (!_memory.TryGetValue(full, out var json))
        {
            try
            {
                var path = PathFor(scope, key);
                if (!File.Exists(path))
                {
                    return null;
                }

                json = File.ReadAllText(path);
                Remember(full, json);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                return null;
            }
        }

        try
        {
            return JsonSerializer.Deserialize<CatalogPage>(json, ServiceProtocol.Json);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    /// <summary>Keeps <paramref name="page"/>, and says whether it differs from the copy it replaces.</summary>
    internal bool Put(string scope, string key, CatalogPage page)
    {
        var full = scope + "/" + key;
        var json = JsonSerializer.Serialize(page, ServiceProtocol.Json);
        if (_memory.TryGetValue(full, out var previous) && previous == json)
        {
            return false;
        }

        Remember(full, json);
        try
        {
            var path = PathFor(scope, key);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path + ".partial", json);
            File.Move(path + ".partial", path, overwrite: true);
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            // The copy on disk is a convenience; the page on screen is already right.
        }

        return true;
    }

    /// <summary>Forgets every copy kept for an account, as signing out must.</summary>
    internal void Forget(string scope)
    {
        foreach (var key in new List<string>(_memory.Keys))
        {
            if (key.StartsWith(scope + "/", StringComparison.Ordinal))
            {
                _memory.Remove(key);
                _order.Remove(key);
            }
        }

        try
        {
            var folder = Path.Combine(Root, Safe(scope));
            if (Directory.Exists(folder))
            {
                Directory.Delete(folder, recursive: true);
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
    }

    private void Remember(string full, string json)
    {
        _memory[full] = json;
        _order.Remove(full);
        _order.AddFirst(full);
        while (_order.Count > MaxEntries && _order.Last is { } oldest)
        {
            _memory.Remove(oldest.Value);
            _order.RemoveLast();
        }
    }

    private static string PathFor(string scope, string key) =>
        Path.Combine(Root, Safe(scope), ArtworkLoader.CacheKey(key) + ".json");

    /// <summary>A scope is a profile id or "guest"; anything else in it becomes an underscore.</summary>
    private static string Safe(string scope)
    {
        var chars = scope.ToCharArray();
        for (var i = 0; i < chars.Length; i++)
        {
            if (!char.IsAsciiLetterOrDigit(chars[i]) && chars[i] != '-')
            {
                chars[i] = '_';
            }
        }

        return chars.Length == 0 ? "_" : new string(chars);
    }
}
