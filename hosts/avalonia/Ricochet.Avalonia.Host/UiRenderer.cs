using System.Text.Json;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;

namespace Ricochet.Avalonia.Host;

internal sealed class UiRenderer
{
    private readonly Action<UiEvent> emit;

    public UiRenderer(Action<UiEvent> emit)
    {
        this.emit = emit;
    }

    public Control RenderWindow(UiNode window)
    {
        return new ScrollViewer
        {
            Padding = new Thickness(16),
            Content = RenderChildren(window.Children, Orientation.Vertical),
        };
    }

    private Control RenderNode(UiNode node)
    {
        return node.Type switch
        {
            "window" => RenderWindow(node),
            "text" => Text(node.StringProp("text") ?? ""),
            "heading" => Heading(node.StringProp("text") ?? node.Id, node.IntProp("level", 2)),
            "button" => Button(node),
            "text_input" => TextInput(node, acceptsReturn: false),
            "multiline_text_input" => TextInput(node, acceptsReturn: true),
            "checkbox" => CheckBox(node, toggleStyle: false),
            "toggle" => CheckBox(node, toggleStyle: true),
            "select" => Select(node),
            "stack" => Stack(node),
            "grid" => LayoutGrid(node),
            "split_pane" => SplitPane(node),
            "scroll_view" => ScrollView(node),
            "group" => Group(node),
            "spacer" => new Border { Height = 12 },
            "list" => List(node),
            "tree" => Tree(node),
            "data_grid" => DataGrid(node),
            "rich_text" => RichText(node, editable: false),
            "rich_text_input" => RichText(node, editable: true),
            "menu_bar" or "command_bar" or "context_menu" => CommandBar(node),
            _ => Unsupported(node),
        };
    }

    private Control RenderChildren(IEnumerable<UiNode> children, Orientation orientation)
    {
        var panel = new StackPanel
        {
            Orientation = orientation,
            Spacing = 8,
        };

        foreach (var child in children)
        {
            panel.Children.Add(RenderNode(child));
        }

        return panel;
    }

    private static TextBlock Text(string text)
    {
        return new TextBlock
        {
            Text = text,
            TextWrapping = TextWrapping.Wrap,
            VerticalAlignment = VerticalAlignment.Center,
        };
    }

    private static TextBlock Heading(string text, int level)
    {
        var fontSize = level <= 1 ? 28 : level == 2 ? 22 : 18;
        return new TextBlock
        {
            Text = text,
            FontSize = fontSize,
            FontWeight = FontWeight.SemiBold,
            Margin = new Thickness(0, 4, 0, 2),
            TextWrapping = TextWrapping.Wrap,
        };
    }

    private Control Button(UiNode node)
    {
        var button = new Button
        {
            Content = node.StringProp("label") ?? node.Id,
            HorizontalAlignment = HorizontalAlignment.Left,
            MinWidth = 96,
        };
        button.Click += (_, _) => Emit("click", node.Id, null);
        return button;
    }

    private Control TextInput(UiNode node, bool acceptsReturn)
    {
        var label = node.StringProp("label") ?? node.Id;
        var input = new TextBox
        {
            Text = node.StringProp("value") ?? "",
            PlaceholderText = label,
            AcceptsReturn = acceptsReturn,
            MinWidth = 220,
            MinHeight = acceptsReturn ? 96 : 0,
        };
        input.LostFocus += (_, _) => Emit("change", node.Id, input.Text ?? "");
        return Labeled(label, input);
    }

    private Control CheckBox(UiNode node, bool toggleStyle)
    {
        var checkbox = new CheckBox
        {
            Content = toggleStyle
                ? $"{node.StringProp("label") ?? node.Id} (toggle)"
                : node.StringProp("label") ?? node.Id,
            IsChecked = node.BoolProp("checked"),
        };
        checkbox.Click += (_, _) => Emit("change", node.Id, checkbox.IsChecked == true);
        return checkbox;
    }

    private Control Select(UiNode node)
    {
        var label = node.StringProp("label") ?? node.Id;
        var options = node.ArrayProp("options").Select(UiJson.Label).ToList();
        var value = node.StringProp("value") ?? "";
        var select = new ComboBox
        {
            ItemsSource = options,
            SelectedItem = options.Contains(value) ? value : options.FirstOrDefault(),
            MinWidth = 220,
        };
        select.SelectionChanged += (_, _) => Emit("change", node.Id, select.SelectedItem?.ToString() ?? "");
        return Labeled(label, select);
    }

    private Control Stack(UiNode node)
    {
        var orientation = node.StringProp("orientation") == "horizontal"
            ? Orientation.Horizontal
            : Orientation.Vertical;
        return RenderChildren(node.Children, orientation);
    }

    private Control LayoutGrid(UiNode node)
    {
        var columns = Math.Max(1, node.IntProp("columns", 1));
        var grid = new Grid
        {
            ColumnSpacing = 8,
            RowSpacing = 8,
        };

        for (var column = 0; column < columns; column++)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        }

        for (var i = 0; i < node.Children.Count; i++)
        {
            var row = i / columns;
            if (grid.RowDefinitions.Count <= row)
            {
                grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            }

            var child = RenderNode(node.Children[i]);
            Grid.SetColumn(child, i % columns);
            Grid.SetRow(child, row);
            grid.Children.Add(child);
        }

        return grid;
    }

    private Control SplitPane(UiNode node)
    {
        var horizontal = node.StringProp("orientation") != "vertical";
        var grid = new Grid
        {
            ColumnSpacing = horizontal ? 12 : 0,
            RowSpacing = horizontal ? 0 : 12,
        };

        if (horizontal)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(2, GridUnitType.Star) });
        }
        else
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
            grid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(2, GridUnitType.Star) });
        }

        for (var i = 0; i < Math.Min(2, node.Children.Count); i++)
        {
            var child = RenderNode(node.Children[i]);
            if (horizontal)
            {
                Grid.SetColumn(child, i);
            }
            else
            {
                Grid.SetRow(child, i);
            }

            grid.Children.Add(child);
        }

        return grid;
    }

    private Control ScrollView(UiNode node)
    {
        return new ScrollViewer
        {
            Content = RenderChildren(node.Children, Orientation.Vertical),
            MaxHeight = 360,
        };
    }

    private Control Group(UiNode node)
    {
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(Heading(node.StringProp("title") ?? node.Id, 3));
        foreach (var child in node.Children)
        {
            panel.Children.Add(RenderNode(child));
        }

        return new Border
        {
            BorderBrush = Brushes.Gray,
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(10),
            Child = panel,
        };
    }

    private Control List(UiNode node)
    {
        var items = node.ArrayProp("items")
            .Select(item => new UiChoice(UiJson.ObjectString(item, "id"), UiJson.Label(item)))
            .ToList();
        var selectedIds = node.ArrayProp("selected_ids").Select(UiJson.Label).ToHashSet();
        var selected = items.FirstOrDefault(item => selectedIds.Contains(item.Id));
        var list = new ListBox
        {
            ItemsSource = items,
            SelectedItem = selected,
            MinHeight = 120,
        };
        list.SelectionChanged += (_, _) =>
        {
            if (list.SelectedItem is UiChoice choice)
            {
                Emit("select", node.Id, choice.Id);
            }
        };
        return list;
    }

    private Control Tree(UiNode node)
    {
        var expanded = node.ArrayProp("expanded_ids").Select(UiJson.Label).ToHashSet();
        var selected = node.ArrayProp("selected_ids").Select(UiJson.Label).ToHashSet();
        var roots = node.ArrayProp("nodes")
            .Select(treeNode => TreeItem(node.Id, treeNode, expanded, selected))
            .ToList();
        var tree = new TreeView
        {
            ItemsSource = roots,
            MinHeight = 160,
        };
        tree.SelectionChanged += (_, _) =>
        {
            if (tree.SelectedItem is TreeViewItem { Tag: string id })
            {
                Emit("select", node.Id, id);
            }
        };
        return tree;
    }

    private TreeViewItem TreeItem(
        string treeId,
        JsonElement treeNode,
        ISet<string> expanded,
        ISet<string> selected)
    {
        var id = UiJson.ObjectString(treeNode, "id");
        var label = UiJson.ObjectString(treeNode, "label");
        var item = new TreeViewItem
        {
            Header = string.IsNullOrWhiteSpace(label) ? id : label,
            Tag = id,
            IsExpanded = expanded.Contains(id),
            IsSelected = selected.Contains(id),
            ItemsSource = UiJson.ObjectArray(treeNode, "children")
                .Select(child => TreeItem(treeId, child, expanded, selected))
                .ToList(),
        };
        item.DoubleTapped += (_, args) =>
        {
            args.Handled = true;
            Emit("activate", treeId, id);
        };
        return item;
    }

    private Control DataGrid(UiNode node)
    {
        var columns = node.ArrayProp("columns").ToList();
        var rows = node.ArrayProp("rows").ToList();
        var grid = new Grid
        {
            ColumnSpacing = 1,
            RowSpacing = 1,
        };

        for (var column = 0; column < columns.Count; column++)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        }

        AddGridRow(grid, 0, columns.Select(column =>
            UiJson.ObjectString(column, "title") is { Length: > 0 } title
                ? title
                : UiJson.ObjectString(column, "id")), strong: true);

        for (var rowIndex = 0; rowIndex < rows.Count; rowIndex++)
        {
            var row = rows[rowIndex];
            var cells = row.TryGetProperty("cells", out var cellMap) && cellMap.ValueKind == JsonValueKind.Object
                ? columns.Select(column =>
                {
                    var columnId = UiJson.ObjectString(column, "id");
                    return cellMap.TryGetProperty(columnId, out var cell) ? UiJson.Label(cell) : "";
                })
                : Enumerable.Repeat("", columns.Count);
            AddGridRow(grid, rowIndex + 1, cells, strong: false);

            var rowId = UiJson.ObjectString(row, "id");
            if (!string.IsNullOrWhiteSpace(rowId))
            {
                foreach (var child in grid.Children.OfType<Border>().Where(border => Grid.GetRow(border) == rowIndex + 1))
                {
                    child.PointerPressed += (_, _) => Emit("select", node.Id, rowId);
                    child.DoubleTapped += (_, _) => Emit("activate", node.Id, rowId);
                }
            }
        }

        return new Border
        {
            BorderBrush = Brushes.Gray,
            BorderThickness = new Thickness(1),
            Child = grid,
        };
    }

    private static void AddGridRow(Grid grid, int rowIndex, IEnumerable<string> cells, bool strong)
    {
        if (grid.RowDefinitions.Count <= rowIndex)
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }

        var columnIndex = 0;
        foreach (var cell in cells)
        {
            var border = new Border
            {
                Padding = new Thickness(6, 4),
                Background = strong ? Brushes.LightGray : Brushes.Transparent,
                Child = new TextBlock
                {
                    Text = cell,
                    FontWeight = strong ? FontWeight.SemiBold : FontWeight.Normal,
                    TextWrapping = TextWrapping.Wrap,
                },
            };
            Grid.SetColumn(border, columnIndex);
            Grid.SetRow(border, rowIndex);
            grid.Children.Add(border);
            columnIndex++;
        }
    }

    private Control RichText(UiNode node, bool editable)
    {
        var label = node.StringProp("label");
        var text = RichTextString(node);
        Control control = editable
            ? new TextBox
            {
                Text = text,
                AcceptsReturn = true,
                MinHeight = 120,
            }
            : new TextBlock
            {
                Text = text,
                TextWrapping = TextWrapping.Wrap,
            };
        return string.IsNullOrWhiteSpace(label) ? control : Labeled(label, control);
    }

    private static string RichTextString(UiNode node)
    {
        if (!node.Props.TryGetValue("document", out var document)
            || !document.TryGetProperty("blocks", out var blocks)
            || blocks.ValueKind != JsonValueKind.Array)
        {
            return "";
        }

        var lines = new List<string>();
        foreach (var block in blocks.EnumerateArray())
        {
            if (!block.TryGetProperty("spans", out var spans) || spans.ValueKind != JsonValueKind.Array)
            {
                continue;
            }

            lines.Add(string.Concat(spans.EnumerateArray().Select(span => UiJson.ObjectString(span, "text"))));
        }

        return string.Join(Environment.NewLine + Environment.NewLine, lines.Where(line => !string.IsNullOrWhiteSpace(line)));
    }

    private Control CommandBar(UiNode node)
    {
        var panel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };

        foreach (var item in node.ArrayProp("items"))
        {
            var id = UiJson.ObjectString(item, "id");
            var label = UiJson.ObjectString(item, "label");
            var button = new Button
            {
                Content = string.IsNullOrWhiteSpace(label) ? id : label,
                MinWidth = 96,
            };
            button.Click += (_, _) => Emit("click", id, null);
            panel.Children.Add(button);
        }

        return panel;
    }

    private static Control Unsupported(UiNode node)
    {
        return new Border
        {
            BorderBrush = Brushes.Gray,
            BorderThickness = new Thickness(1),
            Padding = new Thickness(8),
            Child = Text($"Unsupported node {node.Id} ({node.Type})"),
        };
    }

    private static Control Labeled(string label, Control control)
    {
        var panel = new StackPanel { Spacing = 4 };
        panel.Children.Add(new TextBlock
        {
            Text = label,
            FontWeight = FontWeight.SemiBold,
        });
        panel.Children.Add(control);
        return panel;
    }

    private void Emit(string type, string id, object? value)
    {
        emit(new UiEvent
        {
            Type = type,
            Id = id,
            Value = value,
            Backend = "avalonia",
            Native = new Dictionary<string, object?>(),
        });
    }

    private sealed record UiChoice(string Id, string Label)
    {
        public override string ToString() => string.IsNullOrWhiteSpace(Label) ? Id : Label;
    }
}
