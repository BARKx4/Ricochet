using Avalonia.Controls;
using Avalonia.Threading;

namespace Ricochet.Avalonia.Host;

public sealed class MainWindow : Window
{
    private readonly HostProtocol protocol;
    private readonly UiRenderer renderer;

    public MainWindow(HostOptions options, UiEnvelope envelope)
    {
        Width = 960;
        Height = 720;
        MinWidth = 720;
        MinHeight = 480;

        protocol = new HostProtocol(options, action => Dispatcher.UIThread.Post(action));
        renderer = new UiRenderer(protocol.Emit);
        protocol.ResponseReceived += RenderEnvelope;
        Closed += (_, _) => protocol.Dispose();
        RenderEnvelope(envelope);
        protocol.Start();
    }

    private void RenderEnvelope(UiEnvelope envelope)
    {
        Title = envelope.Document.StringProp("title") ?? "Ricochet";
        Content = renderer.RenderWindow(envelope.Document);
    }
}
