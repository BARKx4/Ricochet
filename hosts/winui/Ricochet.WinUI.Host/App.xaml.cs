using Microsoft.UI.Xaml;

namespace Ricochet.WinUI.Host;

public partial class App : Application
{
    private Window? window;

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var options = HostRuntime.Options
            ?? throw new InvalidOperationException("WinUI host options were not initialized");
        var envelope = UiEnvelope.Load(options.DocumentPath);
        window = new MainWindow(options, envelope);
        window.Activate();
    }
}
