using System;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;

namespace Goosic.Windows.Service;

/// <summary>The protocol version this shell speaks, and the only one it accepts.</summary>
/// <remarks>
/// <c>COMPATIBILITY.md</c> declares one exact version rather than a range, because exact
/// equality is what both sides implement. Mirrors <c>goosic_protocol::PROTOCOL_VERSION</c>.
/// </remarks>
internal static class ServiceProtocol
{
    internal const string Version = "0.4.0";

    internal static readonly JsonSerializerOptions Json = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    /// <summary>How long to wait for an answer, by command.</summary>
    /// <remarks>
    /// A client rule rather than a protocol one, kept identical to
    /// <c>goosic_shell_support::timeout_for</c>. Catalog and lyrics commands reach a
    /// third-party host, so they get a wait long enough to cover the service's own upstream
    /// timeout instead of tearing the child down mid-request; a first download decodes a whole
    /// file into the WAV cache and needs far longer still.
    /// </remarks>
    internal static TimeSpan TimeoutFor(string command) => command switch
    {
        "downloads.prepare" => TimeSpan.FromSeconds(120),
        _ when command.StartsWith("catalog.", StringComparison.Ordinal)
            || command.StartsWith("lyrics.", StringComparison.Ordinal) => TimeSpan.FromSeconds(20),
        _ => TimeSpan.FromSeconds(5),
    };
}

/// <summary>One request to the service.</summary>
internal sealed record RequestEnvelope(
    [property: JsonPropertyName("protocolVersion")] string ProtocolVersion,
    [property: JsonPropertyName("requestId")] string RequestId,
    [property: JsonPropertyName("command")] string Command,
    [property: JsonPropertyName("payload")] JsonObject Payload);

/// <summary>What the service answered.</summary>
/// <remarks>
/// The payload stays a <see cref="JsonNode"/> rather than becoming a hand-written mirror of
/// every response type. <c>SHELL_CONTRACT.md</c> names that translation as the drift the
/// migration exists to stop: <c>goosic-protocol</c> is the source of truth, and a second
/// hand-maintained copy agrees with it only for as long as someone keeps it in step.
/// </remarks>
internal sealed record ResponseEnvelope(
    [property: JsonPropertyName("protocolVersion")] string ProtocolVersion,
    [property: JsonPropertyName("requestId")] string RequestId,
    [property: JsonPropertyName("ok")] bool Ok,
    [property: JsonPropertyName("payload")] JsonNode? Payload,
    [property: JsonPropertyName("error")] ErrorObject? Error);

internal sealed record ErrorObject(
    [property: JsonPropertyName("code")] string Code,
    [property: JsonPropertyName("message")] string Message);

/// <summary>The service could not be reached, or stopped being reachable.</summary>
internal sealed class ServiceUnavailableException : Exception
{
    internal ServiceUnavailableException(string message) : base(message)
    {
    }

    internal ServiceUnavailableException(string message, Exception inner) : base(message, inner)
    {
    }
}

/// <summary>The service answered, and the answer was a refusal.</summary>
/// <remarks>
/// Separate from <see cref="ServiceUnavailableException"/> because the distinction decides
/// whether the transport is still healthy: a refused request means the conversation continues,
/// while an unavailable service means there is nothing left to talk to.
/// </remarks>
internal sealed class ServiceRefusedException : Exception
{
    internal ServiceRefusedException(string code, string message) : base(message)
    {
        Code = code;
    }

    internal string Code { get; }
}
