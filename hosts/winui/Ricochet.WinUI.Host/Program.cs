using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using WinRT;

namespace Ricochet.WinUI.Host;

public static class Program
{
    [DllImport("Microsoft.ui.xaml.dll")]
    private static extern void XamlCheckProcessRequirements();

    [STAThread]
    public static int Main(string[] args)
    {
        try
        {
            var options = HostOptions.Parse(args);
            HostRuntime.Options = options;

            if (options.ValidateOnly)
            {
                UiEnvelope.Load(options.DocumentPath);
                return 0;
            }

            XamlCheckProcessRequirements();
            ComWrappersSupport.InitializeComWrappers();
            Application.Start(_ =>
            {
                var context = new DispatcherQueueSynchronizationContext(
                    DispatcherQueue.GetForCurrentThread());
                SynchronizationContext.SetSynchronizationContext(context);
                new App();
            });
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("Ricochet WinUI host failed:");
            Console.Error.WriteLine(ex);
            return 1;
        }
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
                        "usage: Ricochet.WinUI.Host --document PATH [--events PATH] [--responses PATH] [--validate-only]");
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
