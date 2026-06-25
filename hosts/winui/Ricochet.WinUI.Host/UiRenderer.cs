using System.Text.Json;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;

namespace Ricochet.WinUI.Host;

public sealed class UiRenderer
{
    private readonly Action<UiEvent> emit;

    public UiRenderer(Action<UiEvent> emit)
    {
        this.emit = emit;
    }

    public UIElement RenderWindow(UiNode window)
    {
        var root = new ScrollViewer
        {
            Content = RenderChildrenAsStack(window.Children, Orientation.Vertical),
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            Padding = new Thickness(16),
        };
        return root;
    }

    private FrameworkElement RenderNode(UiNode node)
    {
        return node.Type switch
        {
            "text" => new TextBlock { Text = node.StringProp("text") ?? "", TextWrapping = TextWrapping.Wrap },
            "heading" => RenderHeading(node),
            "button" => RenderButton(node),
            "text_input" => RenderTextBox(node, multiline: false),
            "multiline_text_input" => RenderTextBox(node, multiline: true),
            "checkbox" => RenderCheckBox(node),
            "toggle" => RenderToggle(node),
            "select" => RenderSelect(node),
            "stack" => RenderStack(node),
            "grid" => RenderGrid(node),
            "split_pane" => RenderSplitPane(node),
            "scroll_view" => RenderScrollView(node),
            "group" => RenderGroup(node),
            "spacer" => new Border { Height = 12 },
            "list" => RenderList(node),
            "tree" => RenderTree(node),
            "data_grid" => RenderDataGrid(node),
            "rich_text" => RenderRichText(node, editable: false),
            "rich_text_input" => RenderRichText(node, editable: true),
            "menu_bar" => RenderCommandItems(node, "items"),
            "command_bar" => RenderCommandItems(node, "items"),
            "context_menu" => RenderCommandItems(node, "items"),
            _ => Diagnostic($"Unsupported Ricochet UI node type: {node.Type}", node.Id),
        };
    }

    private FrameworkElement RenderHeading(UiNode node)
    {
        var level = Math.Clamp(node.IntProp("level", 1), 1, 6);
        return new TextBlock
        {
            Text = node.StringProp("text") ?? "",
            FontSize = level switch
            {
                1 => 28,
                2 => 24,
                3 => 20,
                _ => 16,
            },
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Margin = new Thickness(0, 0, 0, 8),
            TextWrapping = TextWrapping.Wrap,
        };
    }

    private FrameworkElement RenderButton(UiNode node)
    {
        var button = new Button
        {
            Content = node.StringProp("label") ?? node.Id,
            HorizontalAlignment = HorizontalAlignment.Left,
            Margin = new Thickness(0, 4, 0, 4),
        };
        button.Click += (_, _) => emit(new UiEvent { Type = "click", Id = node.Id, Value = null });
        return button;
    }

    private FrameworkElement RenderTextBox(UiNode node, bool multiline)
    {
        var box = new TextBox
        {
            Header = node.StringProp("label"),
            Text = node.StringProp("value") ?? "",
            AcceptsReturn = multiline,
            TextWrapping = multiline ? TextWrapping.Wrap : TextWrapping.NoWrap,
            MinWidth = 240,
            Height = multiline ? 120 : double.NaN,
            Margin = new Thickness(0, 4, 0, 4),
        };
        box.TextChanged += (_, _) => emit(new UiEvent { Type = "change", Id = node.Id, Value = box.Text });
        return box;
    }

    private FrameworkElement RenderCheckBox(UiNode node)
    {
        var box = new CheckBox
        {
            Content = node.StringProp("label") ?? node.Id,
            IsChecked = node.BoolProp("checked"),
            Margin = new Thickness(0, 4, 0, 4),
        };
        box.Checked += (_, _) => emit(new UiEvent { Type = "change", Id = node.Id, Value = true });
        box.Unchecked += (_, _) => emit(new UiEvent { Type = "change", Id = node.Id, Value = false });
        return box;
    }

    private FrameworkElement RenderToggle(UiNode node)
    {
        var toggle = new ToggleSwitch
        {
            Header = node.StringProp("label") ?? node.Id,
            IsOn = node.BoolProp("checked"),
            Margin = new Thickness(0, 4, 0, 4),
        };
        toggle.Toggled += (_, _) => emit(new UiEvent { Type = "change", Id = node.Id, Value = toggle.IsOn });
        return toggle;
    }

    private FrameworkElement RenderSelect(UiNode node)
    {
        var combo = new ComboBox
        {
            Header = node.StringProp("label"),
            MinWidth = 200,
            Margin = new Thickness(0, 4, 0, 4),
        };
        var value = node.StringProp("value");
        foreach (var option in node.ArrayProp("options"))
        {
            var optionText = JsonElementLabel(option);
            combo.Items.Add(optionText);
            if (optionText == value)
            {
                combo.SelectedItem = optionText;
            }
        }

        combo.SelectionChanged += (_, _) =>
            emit(new UiEvent { Type = "change", Id = node.Id, Value = combo.SelectedItem?.ToString() });
        return combo;
    }

    private FrameworkElement RenderStack(UiNode node)
    {
        var orientation = node.StringProp("orientation") == "horizontal"
            ? Orientation.Horizontal
            : Orientation.Vertical;
        return RenderChildrenAsStack(node.Children, orientation);
    }

    private FrameworkElement RenderGrid(UiNode node)
    {
        var grid = new Grid { RowSpacing = 8, ColumnSpacing = 8 };
        var rowCount = Math.Max(1, node.ArrayProp("rows").Count());
        var columnCount = Math.Max(1, node.ArrayProp("columns").Count());
        for (var row = 0; row < rowCount; row++)
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }

        for (var column = 0; column < columnCount; column++)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        }

        var index = 0;
        foreach (var child in node.Children)
        {
            var element = RenderNode(child);
            Grid.SetRow(element, index / columnCount);
            Grid.SetColumn(element, index % columnCount);
            grid.Children.Add(element);
            index++;
        }

        return grid;
    }

    private FrameworkElement RenderSplitPane(UiNode node)
    {
        var orientation = node.StringProp("orientation") == "horizontal"
            ? Orientation.Horizontal
            : Orientation.Vertical;
        return RenderChildrenAsStack(node.Children, orientation);
    }

    private FrameworkElement RenderScrollView(UiNode node)
    {
        return new ScrollViewer
        {
            Content = RenderChildrenAsStack(node.Children, Orientation.Vertical),
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
    }

    private FrameworkElement RenderGroup(UiNode node)
    {
        return new Expander
        {
            Header = node.StringProp("title") ?? node.Id,
            IsExpanded = true,
            Content = RenderChildrenAsStack(node.Children, Orientation.Vertical),
            Margin = new Thickness(0, 8, 0, 8),
        };
    }

    private FrameworkElement RenderList(UiNode node)
    {
        var list = new ListView { Margin = new Thickness(0, 4, 0, 4) };
        foreach (var item in node.ArrayProp("items"))
        {
            list.Items.Add(JsonElementLabel(item));
        }

        list.SelectionChanged += (_, _) =>
            emit(new UiEvent { Type = "select", Id = node.Id, Value = list.SelectedItem?.ToString() });
        return list;
    }

    private FrameworkElement RenderTree(UiNode node)
    {
        var tree = new TreeView();
        foreach (var root in node.ArrayProp("nodes"))
        {
            tree.RootNodes.Add(RenderTreeNode(root));
        }

        tree.SelectionChanged += (_, _) =>
            emit(new UiEvent { Type = "select", Id = node.Id, Value = tree.SelectedItem?.ToString() });
        return tree;
    }

    private TreeViewNode RenderTreeNode(JsonElement json)
    {
        var id = JsonPropertyString(json, "id") ?? "";
        var label = JsonPropertyString(json, "label") ?? id;
        var node = new TreeViewNode { Content = label };
        if (json.TryGetProperty("children", out var children) && children.ValueKind == JsonValueKind.Array)
        {
            foreach (var child in children.EnumerateArray())
            {
                node.Children.Add(RenderTreeNode(child));
            }
        }

        return node;
    }

    private FrameworkElement RenderDataGrid(UiNode node)
    {
        var panel = new StackPanel { Spacing = 4 };
        var columns = node.ArrayProp("columns").ToArray();
        var header = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var column in columns)
        {
            header.Children.Add(new TextBlock
            {
                Text = JsonPropertyString(column, "title") ?? JsonPropertyString(column, "id") ?? "",
                FontWeight = FontWeights.SemiBold,
                MinWidth = 120,
            });
        }

        panel.Children.Add(header);
        foreach (var row in node.ArrayProp("rows"))
        {
            var rowPanel = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            var rowId = JsonPropertyString(row, "id") ?? "";
            if (row.TryGetProperty("cells", out var cells) && cells.ValueKind == JsonValueKind.Object)
            {
                foreach (var column in columns)
                {
                    var columnId = JsonPropertyString(column, "id") ?? "";
                    var cell = cells.TryGetProperty(columnId, out var cellValue)
                        ? JsonElementLabel(cellValue)
                        : "";
                    rowPanel.Children.Add(new TextBlock { Text = cell, MinWidth = 120 });
                }
            }

            var item = new Button
            {
                Content = rowPanel,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Left,
            };
            item.Click += (_, _) => emit(new UiEvent { Type = "activate", Id = node.Id, Value = rowId });
            panel.Children.Add(item);
        }

        return panel;
    }

    private FrameworkElement RenderRichText(UiNode node, bool editable)
    {
        var text = PlainTextFromRichDocument(node);
        if (editable)
        {
            var editor = new RichEditBox
            {
                Header = node.StringProp("label"),
                MinHeight = 120,
                Margin = new Thickness(0, 4, 0, 4),
            };
            editor.Document.SetText(TextSetOptions.None, text);
            editor.TextChanged += (_, _) => emit(new UiEvent { Type = "change", Id = node.Id, Value = null });
            return editor;
        }

        var block = new RichTextBlock { TextWrapping = TextWrapping.Wrap };
        block.Blocks.Add(new Paragraph { Inlines = { new Run { Text = text } } });
        return block;
    }

    private FrameworkElement RenderCommandItems(UiNode node, string propName)
    {
        var bar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        foreach (var item in node.ArrayProp(propName))
        {
            var id = JsonPropertyString(item, "id") ?? node.Id;
            var label = JsonPropertyString(item, "label") ?? id;
            var button = new Button { Content = label };
            button.Click += (_, _) => emit(new UiEvent { Type = "click", Id = id, Value = null });
            bar.Children.Add(button);
        }

        return bar;
    }

    private StackPanel RenderChildrenAsStack(IEnumerable<UiNode> children, Orientation orientation)
    {
        var panel = new StackPanel { Orientation = orientation, Spacing = 8 };
        foreach (var child in children)
        {
            panel.Children.Add(RenderNode(child));
        }

        return panel;
    }

    private static FrameworkElement Diagnostic(string message, string id)
    {
        return new InfoBar
        {
            Title = $"Ricochet UI: {id}",
            Message = message,
            Severity = InfoBarSeverity.Warning,
            IsOpen = true,
            Margin = new Thickness(0, 4, 0, 4),
        };
    }

    private static string PlainTextFromRichDocument(UiNode node)
    {
        if (!node.Props.TryGetValue("document", out var document)
            || !document.TryGetProperty("blocks", out var blocks)
            || blocks.ValueKind != JsonValueKind.Array)
        {
            return "";
        }

        var paragraphs = new List<string>();
        foreach (var block in blocks.EnumerateArray())
        {
            if (!block.TryGetProperty("spans", out var spans) || spans.ValueKind != JsonValueKind.Array)
            {
                continue;
            }

            paragraphs.Add(string.Concat(spans.EnumerateArray().Select(span => JsonPropertyString(span, "text") ?? "")));
        }

        return string.Join(Environment.NewLine, paragraphs);
    }

    private static string JsonElementLabel(JsonElement value)
    {
        return value.ValueKind switch
        {
            JsonValueKind.String => value.GetString() ?? "",
            JsonValueKind.Number => value.GetRawText(),
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            JsonValueKind.Null => "",
            JsonValueKind.Object when JsonPropertyString(value, "label") is { } label => label,
            JsonValueKind.Object when JsonPropertyString(value, "title") is { } title => title,
            JsonValueKind.Object when JsonPropertyString(value, "id") is { } id => id,
            _ => value.GetRawText(),
        };
    }

    private static string? JsonPropertyString(JsonElement element, string name)
    {
        if (element.ValueKind != JsonValueKind.Object || !element.TryGetProperty(name, out var value))
        {
            return null;
        }

        return value.ValueKind switch
        {
            JsonValueKind.String => value.GetString(),
            JsonValueKind.Number => value.GetRawText(),
            JsonValueKind.True => "true",
            JsonValueKind.False => "false",
            JsonValueKind.Null => null,
            _ => value.GetRawText(),
        };
    }
}
