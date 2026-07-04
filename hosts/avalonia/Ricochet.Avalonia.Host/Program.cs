using Avalonia;

namespace Ricochet.Avalonia.Host;

public static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        try
        {
            var options = HostOptions.Parse(args);
            HostRuntime.Options = options;

            if (options.ValidateOnly)
            {
                var envelope = UiEnvelope.Load(options.DocumentPath);
                new UiRenderer(_ => { }).RenderWindow(envelope.Document);
                var title = envelope.Document.StringProp("title") ?? envelope.Document.Id;
                Console.WriteLine($"Ricochet Avalonia host validated {title}");
                return 0;
            }

            BuildAvaloniaApp().StartWithClassicDesktopLifetime(Array.Empty<string>());
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("Ricochet Avalonia host failed:");
            Console.Error.WriteLine(ex);
            return 1;
        }
    }

    public static AppBuilder BuildAvaloniaApp()
    {
        return AppBuilder.Configure<App>().UsePlatformDetect();
    }
}

public sealed class HostOptions
{
    public string DocumentPath { get; private init; } = "";
    public string? EventsPath { get; private init; }
    public string? ResponsesPath { get; private init; }
    public bool ValidateOnly { get; private init; }

    public static HostOptions Parse(string[] args)
    {
        var documentPath = "";
        string? eventsPath = null;
        string? responsesPath = null;
        var validateOnly = false;

        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--document":
                    documentPath = ReadValue(args, ref i, "--document");
                    break;
                case "--events":
                    eventsPath = ReadValue(args, ref i, "--events");
                    break;
                case "--responses":
                    responsesPath = ReadValue(args, ref i, "--responses");
                    break;
                case "--validate-only":
                    validateOnly = true;
                    break;
                case "--help":
                case "-h":
                    throw new InvalidOperationException(
                        "usage: Ricochet.Avalonia.Host --document PATH [--events PATH] [--responses PATH] [--validate-only]");
                default:
                    throw new InvalidOperationException($"unknown argument: {args[i]}");
            }
        }

        if (string.IsNullOrWhiteSpace(documentPath))
        {
            throw new InvalidOperationException("--document PATH is required");
        }

        if (!File.Exists(documentPath))
        {
            throw new FileNotFoundException("document JSON was not found", documentPath);
        }

        return new HostOptions
        {
            DocumentPath = documentPath,
            EventsPath = eventsPath,
            ResponsesPath = responsesPath,
            ValidateOnly = validateOnly,
        };
    }

    private static string ReadValue(string[] args, ref int index, string name)
    {
        if (index + 1 >= args.Length)
        {
            throw new InvalidOperationException($"{name} requires a value");
        }

        index++;
        return args[index];
    }
}

internal static class HostRuntime
{
    public static HostOptions? Options { get; set; }
}
