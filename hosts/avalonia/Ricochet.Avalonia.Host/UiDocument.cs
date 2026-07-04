using System.Text.Json;
using System.Text.Json.Serialization;

namespace Ricochet.Avalonia.Host;

public sealed class UiEnvelope
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        ReadCommentHandling = JsonCommentHandling.Skip,
    };

    [JsonPropertyName("schema_version")]
    public int SchemaVersion { get; set; } = 1;

    [JsonPropertyName("backend")]
    public string Backend { get; set; } = "avalonia";

    [JsonPropertyName("state")]
    public JsonElement State { get; set; }

    [JsonPropertyName("document")]
    public UiNode Document { get; set; } = new();

    [JsonExtensionData]
    public Dictionary<string, JsonElement> Extra { get; set; } = new();

    public static UiEnvelope Load(string path)
    {
        var json = File.ReadAllText(path);
        return FromJson(json, path);
    }

    public static UiEnvelope FromJson(string json, string sourceName)
    {
        using var parsed = JsonDocument.Parse(json);
        var root = parsed.RootElement;
        var envelope = root.TryGetProperty("document", out _)
            ? JsonSerializer.Deserialize<UiEnvelope>(json, JsonOptions)
            : new UiEnvelope
            {
                Document = JsonSerializer.Deserialize<UiNode>(root.GetRawText(), JsonOptions)
                    ?? throw new InvalidOperationException($"Ricochet UI document was empty in {sourceName}"),
            };

        if (envelope is null)
        {
            throw new InvalidOperationException($"Ricochet UI envelope was empty in {sourceName}");
        }

        envelope.Document.ValidateRoot(sourceName);
        return envelope;
    }
}

public sealed class UiNode
{
    [JsonPropertyName("schema_version")]
    public int SchemaVersion { get; set; } = 1;

    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("type")]
    public string Type { get; set; } = "";

    [JsonPropertyName("props")]
    public Dictionary<string, JsonElement> Props { get; set; } = new();

    [JsonPropertyName("children")]
    public List<UiNode> Children { get; set; } = new();

    [JsonPropertyName("events")]
    public List<string> Events { get; set; } = new();

    [JsonPropertyName("native_options")]
    public Dictionary<string, JsonElement> NativeOptions { get; set; } = new();

    [JsonExtensionData]
    public Dictionary<string, JsonElement> Extra { get; set; } = new();

    public void ValidateRoot(string sourceName)
    {
        Validate(sourceName);
        if (Type != "window")
        {
            throw new InvalidOperationException($"Ricochet UI root in {sourceName} must be a window, got {Type}");
        }
    }

    private void Validate(string sourceName)
    {
        if (string.IsNullOrWhiteSpace(Id))
        {
            throw new InvalidOperationException($"Ricochet UI node in {sourceName} is missing id");
        }

        if (string.IsNullOrWhiteSpace(Type))
        {
            throw new InvalidOperationException($"Ricochet UI node {Id} in {sourceName} is missing type");
        }

        foreach (var child in Children)
        {
            child.Validate(sourceName);
        }
    }

    public string? StringProp(string name)
    {
        if (!Props.TryGetValue(name, out var value))
        {
            return null;
        }

        return UiJson.Label(value);
    }

    public bool BoolProp(string name, bool fallback = false)
    {
        if (!Props.TryGetValue(name, out var value))
        {
            return fallback;
        }

        return value.ValueKind switch
        {
            JsonValueKind.True => true,
            JsonValueKind.False => false,
            JsonValueKind.String => bool.TryParse(value.GetString(), out var parsed) ? parsed : fallback,
            _ => fallback,
        };
    }

    public int IntProp(string name, int fallback = 0)
    {
        if (!Props.TryGetValue(name, out var value))
        {
            return fallback;
        }

        if (value.ValueKind == JsonValueKind.Number && value.TryGetInt32(out var parsed))
        {
            return parsed;
        }

        return value.ValueKind == JsonValueKind.String
            && int.TryParse(value.GetString(), out var stringParsed)
                ? stringParsed
                : fallback;
    }

    public IEnumerable<JsonElement> ArrayProp(string name)
    {
        if (!Props.TryGetValue(name, out var value) || value.ValueKind != JsonValueKind.Array)
        {
            return Array.Empty<JsonElement>();
        }

        return value.EnumerateArray().ToArray();
    }
}

public sealed class UiEvent
{
    [JsonPropertyName("schema_version")]
    public int SchemaVersion { get; init; } = 1;

    [JsonPropertyName("type")]
    public string Type { get; init; } = "";

    [JsonPropertyName("id")]
    public string Id { get; init; } = "";

    [JsonPropertyName("value")]
    public object? Value { get; init; }

    [JsonPropertyName("backend")]
    public string Backend { get; init; } = "avalonia";

    [JsonPropertyName("native")]
    public Dictionary<string, object?> Native { get; init; } = new();
}

internal static class UiJson
{
    public static string Label(JsonElement value)
    {
        return value.ValueKind switch
        {
            JsonValueKind.Null => "",
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            JsonValueKind.Number => value.GetRawText(),
            JsonValueKind.String => value.GetString() ?? "",
            JsonValueKind.Array => string.Join(", ", value.EnumerateArray().Select(Label)),
            JsonValueKind.Object => ObjectLabel(value),
            _ => value.GetRawText(),
        };
    }

    public static string ObjectString(JsonElement value, string name)
    {
        return value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var property)
            ? Label(property)
            : "";
    }

    public static IEnumerable<JsonElement> ObjectArray(JsonElement value, string name)
    {
        return value.ValueKind == JsonValueKind.Object
            && value.TryGetProperty(name, out var property)
            && property.ValueKind == JsonValueKind.Array
                ? property.EnumerateArray().ToArray()
                : Array.Empty<JsonElement>();
    }

    private static string ObjectLabel(JsonElement value)
    {
        foreach (var propertyName in new[] { "label", "title", "id" })
        {
            if (value.TryGetProperty(propertyName, out var property))
            {
                return Label(property);
            }
        }

        return value.GetRawText();
    }
}
