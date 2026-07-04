using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;

namespace Ricochet.Avalonia.Host;

public sealed class App : Application
{
    public override void Initialize()
    {
        RequestedThemeVariant = ThemeVariant.Default;
        Styles.Add(new FluentTheme());
    }

    public override void OnFrameworkInitializationCompleted()
    {
        var options = HostRuntime.Options
            ?? throw new InvalidOperationException("Avalonia host options were not initialized");
        var envelope = UiEnvelope.Load(options.DocumentPath);

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.MainWindow = new MainWindow(options, envelope);
        }

        base.OnFrameworkInitializationCompleted();
    }
}
