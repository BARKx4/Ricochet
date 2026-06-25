using System.Text.Json;
using Microsoft.UI.Dispatching;

namespace Ricochet.WinUI.Host;

internal sealed class HostProtocol : IDisposable
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private readonly HostOptions options;
    private readonly DispatcherQueue dispatcher;
    private readonly CancellationTokenSource cancellation = new();
    private long responseOffset;

    public HostProtocol(HostOptions options, DispatcherQueue dispatcher)
    {
        this.options = options;
        this.dispatcher = dispatcher;
    }

    public event Action<UiEnvelope>? ResponseReceived;

    public void Start()
    {
        if (!string.IsNullOrWhiteSpace(options.EventsPath))
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(options.EventsPath))!);
            File.WriteAllText(options.EventsPath, "");
        }

        if (!string.IsNullOrWhiteSpace(options.ResponsesPath))
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(options.ResponsesPath))!);
            File.WriteAllText(options.ResponsesPath, "");
            _ = Task.Run(PollResponsesAsync);
        }
    }

    public void Emit(UiEvent evt)
    {
        if (string.IsNullOrWhiteSpace(options.EventsPath))
        {
            return;
        }

        var json = JsonSerializer.Serialize(evt, JsonOptions);
        File.AppendAllText(options.EventsPath, json + Environment.NewLine);
    }

    private async Task PollResponsesAsync()
    {
        while (!cancellation.IsCancellationRequested)
        {
            try
            {
                foreach (var line in ReadNewLines(options.ResponsesPath!))
                {
                    if (string.IsNullOrWhiteSpace(line))
                    {
                        continue;
                    }

                    var envelope = UiEnvelope.FromJson(line, options.ResponsesPath!);
                    dispatcher.TryEnqueue(() => ResponseReceived?.Invoke(envelope));
                }
            }
            catch (Exception ex)
            {
                dispatcher.TryEnqueue(() =>
                    ResponseReceived?.Invoke(UiEnvelope.FromJson(
                        $$"""
                        {
                          "backend": "winui",
                          "document": {
                            "schema_version": 1,
                            "id": "host_protocol_error",
                            "type": "window",
                            "props": { "title": "Ricochet WinUI Host Error" },
                            "children": [
                              {
                                "schema_version": 1,
                                "id": "host_protocol_error_text",
                                "type": "text",
                                "props": { "text": {{JsonSerializer.Serialize(ex.Message)}} },
                                "children": [],
                                "events": [],
                                "native_options": {}
                              }
                            ],
                            "events": [],
                            "native_options": {}
                          }
                        }
                        """,
                        "host protocol error")));
            }

            await Task.Delay(40, cancellation.Token).ConfigureAwait(false);
        }
    }

    private IEnumerable<string> ReadNewLines(string path)
    {
        if (!File.Exists(path))
        {
            return Array.Empty<string>();
        }

        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        stream.Seek(responseOffset, SeekOrigin.Begin);
        using var reader = new StreamReader(stream);
        var lines = new List<string>();
        while (reader.ReadLine() is { } line)
        {
            lines.Add(line);
        }

        responseOffset = stream.Position;
        return lines;
    }

    public void Dispose()
    {
        cancellation.Cancel();
        cancellation.Dispose();
    }
}
