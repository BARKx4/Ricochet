using Microsoft.UI.Xaml;

namespace Ricochet.WinUI.Host;

public sealed partial class MainWindow : Window
{
    private readonly HostProtocol protocol;
    private readonly UiRenderer renderer;

    public MainWindow(HostOptions options, UiEnvelope envelope)
    {
        InitializeComponent();
        protocol = new HostProtocol(options, DispatcherQueue);
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
