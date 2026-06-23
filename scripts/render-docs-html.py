#!/usr/bin/env python3
"""Render public Markdown docs to static HTML pages.

This intentionally small renderer covers the Markdown shapes used by Ricochet's
public docs. It does not replace a full Markdown engine; it gives the repository
a repeatable no-install path for keeping human-facing docs available as HTML.
"""

from __future__ import annotations

import argparse
import html
import os
import re
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DOCS_ROOT = REPO_ROOT / "docs"


INTERNAL_DOCS = {
    "feature-map.md",
}


def is_internal_doc(path: Path) -> bool:
    rel = path.relative_to(DOCS_ROOT).as_posix()
    return rel in INTERNAL_DOCS or rel.startswith("superpowers/")


def slugify(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text)
    text = html.unescape(text).lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    return text.strip("-") or "section"


def split_target(target: str) -> tuple[str, str]:
    for marker in ("#", "?"):
        if marker in target:
            base, suffix = target.split(marker, 1)
            return base, marker + suffix
    return target, ""


def rewrite_href(target: str) -> str:
    if re.match(r"^(?:[a-z][a-z0-9+.-]*:|#)", target, re.IGNORECASE):
        return target
    base, suffix = split_target(target)
    if base.endswith(".md"):
        base = base[:-3] + ".html"
    return base + suffix


def escape_visible(text: str) -> str:
    escaped = html.escape(text, quote=False)
    return (
        escaped.replace("{%", "{&#37;")
        .replace("%}", "&#37;}")
        .replace("{{", "{&#123;")
        .replace("}}", "&#125;}")
    )


def render_inline(text: str) -> str:
    code_spans: list[str] = []
    html_spans: list[str] = []

    def hold_code(match: re.Match[str]) -> str:
        code_spans.append(f"<code>{escape_visible(match.group(2))}</code>")
        return f"\x00CODE{len(code_spans) - 1}\x00"

    text = re.sub(r"(`+)(.*?)(\1)", hold_code, text)

    def link(match: re.Match[str]) -> str:
        label = render_inline(match.group(1))
        href = html.escape(rewrite_href(match.group(2).strip()), quote=True)
        html_spans.append(f'<a href="{href}">{label}</a>')
        return f"\x00HTML{len(html_spans) - 1}\x00"

    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", link, text)
    escaped = escape_visible(text)
    escaped = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", escaped)
    escaped = re.sub(r"(?<!\*)\*([^*\n]+)\*(?!\*)", r"<em>\1</em>", escaped)

    for index, html_markup in enumerate(html_spans):
        escaped = escaped.replace(f"\x00HTML{index}\x00", html_markup)
    for index, code_html in enumerate(code_spans):
        escaped = escaped.replace(f"\x00CODE{index}\x00", code_html)
    return escaped


def is_table_separator(line: str) -> bool:
    cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
    return bool(cells) and all(re.fullmatch(r":?-{3,}:?", cell or "") for cell in cells)


def parse_table_row(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


@dataclass
class RenderedDoc:
    body: str
    title: str
    toc: list[tuple[str, str, int]]


class MarkdownRenderer:
    def __init__(self, source: str, fallback_title: str) -> None:
        self.lines = source.replace("\r\n", "\n").split("\n")
        self.fallback_title = fallback_title
        self.out: list[str] = []
        self.title = fallback_title
        self.toc: list[tuple[str, str, int]] = []
        self.paragraph: list[str] = []
        self.list_stack: list[str] = []
        self.slug_counts: dict[str, int] = {}

    def close_paragraph(self) -> None:
        if self.paragraph:
            text = " ".join(part.strip() for part in self.paragraph).strip()
            self.out.append(f"<p>{render_inline(text)}</p>")
            self.paragraph.clear()

    def close_lists(self) -> None:
        while self.list_stack:
            self.out.append(f"</{self.list_stack.pop()}>")

    def close_blocks(self) -> None:
        self.close_paragraph()
        self.close_lists()

    def unique_id(self, text: str) -> str:
        base = slugify(text)
        count = self.slug_counts.get(base, 0)
        self.slug_counts[base] = count + 1
        return base if count == 0 else f"{base}-{count + 1}"

    def render_table(self, start: int) -> int:
        rows: list[list[str]] = []
        index = start
        while index < len(self.lines) and "|" in self.lines[index].strip():
            rows.append(parse_table_row(self.lines[index]))
            index += 1
        if len(rows) < 2 or not is_table_separator(self.lines[start + 1]):
            self.paragraph.append(self.lines[start])
            return start + 1

        self.close_blocks()
        header = rows[0]
        body_rows = rows[2:]
        self.out.append("<table>")
        self.out.append("<thead><tr>")
        for cell in header:
            self.out.append(f"<th>{render_inline(cell)}</th>")
        self.out.append("</tr></thead>")
        if body_rows:
            self.out.append("<tbody>")
            for row in body_rows:
                self.out.append("<tr>")
                for cell in row:
                    self.out.append(f"<td>{render_inline(cell)}</td>")
                self.out.append("</tr>")
            self.out.append("</tbody>")
        self.out.append("</table>")
        return index

    def render_list_item(self, line: str) -> bool:
        unordered = re.match(r"^\s*[-*+]\s+(.+)$", line)
        ordered = re.match(r"^\s*\d+\.\s+(.+)$", line)
        if not unordered and not ordered:
            return False
        self.close_paragraph()
        kind = "ul" if unordered else "ol"
        text = (unordered or ordered).group(1)
        if not self.list_stack or self.list_stack[-1] != kind:
            self.close_lists()
            self.out.append(f"<{kind}>")
            self.list_stack.append(kind)
        self.out.append(f"<li>{render_inline(text)}</li>")
        return True

    def render_list_continuation(self, line: str) -> bool:
        if not self.list_stack or not self.out or not self.out[-1].startswith("<li>"):
            return False
        if not line.startswith(("  ", "\t")):
            return False
        text = line.strip()
        if not text:
            return False
        self.out[-1] = self.out[-1][:-5] + " " + render_inline(text) + "</li>"
        return True

    def render(self) -> RenderedDoc:
        index = 0
        while index < len(self.lines):
            line = self.lines[index]
            stripped = line.strip()

            if stripped.startswith("```"):
                self.close_blocks()
                language = stripped.strip("`").strip()
                index += 1
                code: list[str] = []
                while index < len(self.lines) and not self.lines[index].strip().startswith("```"):
                    code.append(self.lines[index])
                    index += 1
                class_attr = f' class="language-{html.escape(language, quote=True)}"' if language else ""
                code_html = escape_visible("\n".join(code))
                self.out.append(f'<pre class="code-block"><code{class_attr}>{code_html}</code></pre>')
                index += 1
                continue

            if not stripped:
                self.close_blocks()
                index += 1
                continue

            if stripped in ("---", "***", "___"):
                self.close_blocks()
                self.out.append("<hr>")
                index += 1
                continue

            if "|" in stripped and index + 1 < len(self.lines) and is_table_separator(self.lines[index + 1]):
                index = self.render_table(index)
                continue

            heading = re.match(r"^(#{1,6})\s+(.+)$", stripped)
            if heading:
                self.close_blocks()
                level = len(heading.group(1))
                text = heading.group(2).strip()
                heading_id = self.unique_id(text)
                if level == 1 and self.title == self.fallback_title:
                    self.title = re.sub(r"<[^>]+>", "", text)
                if level <= 3 and level > 1:
                    self.toc.append((render_inline(text), heading_id, level))
                if level > 1:
                    self.out.append(f'<h{level} id="{heading_id}">{render_inline(text)}</h{level}>')
                index += 1
                continue

            if stripped.startswith(">"):
                self.close_blocks()
                quote_lines = []
                while index < len(self.lines) and self.lines[index].strip().startswith(">"):
                    quote_lines.append(self.lines[index].strip().lstrip(">").strip())
                    index += 1
                self.out.append(f"<blockquote><p>{render_inline(' '.join(quote_lines))}</p></blockquote>")
                continue

            if self.render_list_item(line):
                index += 1
                continue

            if self.render_list_continuation(line):
                index += 1
                continue

            if self.list_stack:
                self.close_lists()
            self.paragraph.append(line)
            index += 1

        self.close_blocks()
        return RenderedDoc("\n".join(self.out), self.title, self.toc)


def title_from_filename(path: Path) -> str:
    stem = "Index" if path.stem.lower() == "readme" else path.stem
    return stem.replace("-", " ").replace("_", " ").title()


def rel_url(from_file: Path, to_file: Path) -> str:
    return os.path.relpath(to_file, start=from_file.parent).replace(os.sep, "/")


def build_page(source_md: Path, rendered: RenderedDoc) -> str:
    html_path = source_md.with_suffix(".html")
    stylesheet = rel_url(html_path, DOCS_ROOT / "reference" / "styles.css")
    app_js = rel_url(html_path, DOCS_ROOT / "reference" / "app.js")
    home = rel_url(html_path, DOCS_ROOT / "reference" / "index.html") + "#top"
    learn = rel_url(html_path, DOCS_ROOT / "reference" / "learn" / "index.html")
    guides = rel_url(html_path, DOCS_ROOT / "reference" / "guides" / "index.html")

    toc_items = "\n".join(
        f'          <a href="#{heading_id}">{label}</a>' for label, heading_id, level in rendered.toc if level >= 2
    )
    if not toc_items:
        toc_items = f'          <a href="#{slugify(rendered.title)}">Top</a>'

    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{escape_visible(rendered.title)} | Ricochet Docs</title>
    <link rel="stylesheet" href="{stylesheet}">
  </head>
  <body class="guide-page">
    <header class="topbar">
      <a class="brand" href="{home}" aria-label="Ricochet Reference home">
        <span class="brand-mark">rco</span>
        <span>Ricochet Reference</span>
      </a>
      <nav class="nav-links" aria-label="Reference sections">
        <a href="{home}">Reference</a>
        <a href="{learn}">Learn</a>
        <a href="{guides}">Guides</a>
      </nav>
    </header>

    <main id="top">
      <section class="hero section-band">
        <div class="hero-copy">
          <p class="eyebrow">Ricochet docs</p>
          <h1>{render_inline(rendered.title)}</h1>
          <p class="lede">Production-facing documentation for Ricochet developers.</p>
        </div>
      </section>

      <section class="section-band">
        <div class="guide-layout">
          <aside class="guide-toc" aria-label="Page contents">
            <strong>On this page</strong>
{toc_items}
          </aside>
          <article class="guide-content">
{rendered.body}
          </article>
        </div>
      </section>
    </main>

    <footer class="footer">
      <span>Ricochet Docs</span>
      <a href="{guides}">Back to Guides</a>
    </footer>
    <script src="{app_js}"></script>
  </body>
</html>
"""


def render_file(path: Path, force: bool) -> bool:
    if is_internal_doc(path):
        return False
    html_path = path.with_suffix(".html")
    if html_path.exists() and not force:
        return False
    source = path.read_text(encoding="utf-8")
    rendered = MarkdownRenderer(source, title_from_filename(path)).render()
    html_path.write_text(build_page(path, rendered), encoding="utf-8", newline="\n")
    return True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--force", action="store_true", help="overwrite existing HTML siblings")
    parser.add_argument(
        "--refresh-public",
        action="store_true",
        help="overwrite generated public HTML siblings while preserving the custom Learn manual pages",
    )
    args = parser.parse_args()

    rendered: list[str] = []
    skipped_internal = 0
    skipped_existing = 0
    for path in sorted(DOCS_ROOT.rglob("*.md")):
        rel = path.relative_to(DOCS_ROOT).as_posix()
        if is_internal_doc(path):
            skipped_internal += 1
            continue
        can_refresh = args.refresh_public and not rel.startswith("learn/")
        if path.with_suffix(".html").exists() and not args.force and not can_refresh:
            skipped_existing += 1
            continue
        if render_file(path, args.force or can_refresh):
            rendered.append(rel)

    print(f"Rendered {len(rendered)} Markdown files to HTML.")
    for rel in rendered:
        print(f"  {rel}")
    print(f"Skipped {skipped_existing} files with existing HTML siblings.")
    print(f"Skipped {skipped_internal} internal Markdown files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
