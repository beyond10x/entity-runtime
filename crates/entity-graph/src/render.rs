//! Turning a laid-out graph into bytes, in the formats a person actually asks for.
//!
//! Every emitter here is a pure function of [`Graph`] and [`Layout`], so the same definition
//! produces the same bytes on every machine. Nothing measures a font, reads a clock or consults an
//! environment: a box is as wide as its label has characters, times a constant.
//!
//! Escaping is the emitter's job and not the caller's (R-95). A state called `he said "no"` has to
//! produce a valid document in every format here, or the drawing becomes a way to inject syntax
//! into whatever reads it.

use std::fmt::Write as _;

use crate::graph::{Emphasis, Graph, GraphKind};
use crate::layout::Layout;

/// Character cell width, in SVG user units. Nothing here measures a font — a proportional font
/// would make the drawing depend on which one was installed.
const CELL: usize = 9;
/// Padding inside a box, each side.
const PAD: usize = 12;
/// Height of a box.
const BOX_HEIGHT: usize = 34;
/// Gap between layers.
const LAYER_GAP: usize = 90;
/// Gap between rows within a layer.
const ROW_GAP: usize = 26;
/// Margin around the whole drawing.
const MARGIN: usize = 20;

/// `from --label--> to`, one per line, under a header naming what is drawn.
///
/// The oldest format this repository has and the one a person reads in a terminal without asking
/// for anything. Kept byte-identical to what `entity graph` printed before this crate existed.
#[must_use]
pub fn text(graph: &Graph) -> String {
    let mut out = String::new();
    match graph
        .nodes
        .iter()
        .find(|node| node.emphasis == Emphasis::Entry)
    {
        Some(entry) => {
            let _ = writeln!(out, "{}: initial {}", graph.title, entry.id);
        }
        None => {
            let _ = writeln!(out, "{}", graph.title);
        }
    }
    for edge in &graph.edges {
        let _ = writeln!(out, "{} --{}--> {}", edge.from, edge.label, edge.to);
    }
    out
}

/// Graphviz DOT.
///
/// Not rendered here — emitted for whoever already has `dot` and wants a picture this crate does
/// not draw. Left to right, entry double-bordered, one labelled edge per transition.
#[must_use]
pub fn dot(graph: &Graph) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "digraph {} {{", quote(&graph.title));
    let _ = writeln!(out, "  rankdir=LR;");
    let _ = writeln!(out, "  node [shape=box];");
    for node in &graph.nodes {
        let shape = match node.emphasis {
            Emphasis::Entry => " peripheries=2",
            Emphasis::Terminal => " style=filled fillcolor=\"#eeeeee\"",
            Emphasis::Plain => "",
        };
        let _ = writeln!(
            out,
            "  {} [label={}{shape}];",
            quote(&node.id),
            quote(&node.label)
        );
    }
    for edge in &graph.edges {
        let _ = writeln!(
            out,
            "  {} -> {} [label={}];",
            quote(&edge.from),
            quote(&edge.to),
            quote(&edge.label)
        );
    }
    let _ = writeln!(out, "}}");
    out
}

/// Mermaid source suitable for Markdown renderers and the Mermaid CLI.
///
/// Lifecycles use `stateDiagram-v2`; reference graphs use `flowchart LR`. Opaque node ids keep a
/// definition name from becoming Mermaid syntax, while labels retain the name a person wrote.
#[must_use]
pub fn mermaid(graph: &Graph) -> String {
    match graph.kind {
        GraphKind::Lifecycle => mermaid_lifecycle(graph),
        GraphKind::References => mermaid_references(graph),
    }
}

fn mermaid_lifecycle(graph: &Graph) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    for (at, node) in graph.nodes.iter().enumerate() {
        let _ = writeln!(out, "  state \"{}\" as n{at}", mermaid_label(&node.label));
    }
    if let Some((at, _)) = graph
        .nodes
        .iter()
        .enumerate()
        .find(|(_, node)| node.emphasis == Emphasis::Entry)
    {
        let _ = writeln!(out, "  [*] --> n{at}");
    }
    for edge in &graph.edges {
        let (Some(from), Some(to)) = (find(graph, &edge.from), find(graph, &edge.to)) else {
            continue;
        };
        let _ = writeln!(out, "  n{from} --> n{to}: {}", mermaid_label(&edge.label));
    }
    for (at, node) in graph.nodes.iter().enumerate() {
        if node.emphasis == Emphasis::Terminal {
            let _ = writeln!(out, "  n{at} --> [*]");
        }
    }
    out
}

fn mermaid_references(graph: &Graph) -> String {
    let mut out = String::from("flowchart LR\n");
    for (at, node) in graph.nodes.iter().enumerate() {
        let _ = writeln!(out, "  n{at}[\"{}\"]", mermaid_label(&node.label));
    }
    for edge in &graph.edges {
        let (Some(from), Some(to)) = (find(graph, &edge.from), find(graph, &edge.to)) else {
            continue;
        };
        let _ = writeln!(
            out,
            "  n{from} -->|{}| n{to}",
            mermaid_flowchart_label(&edge.label)
        );
    }
    let missing: Vec<String> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.emphasis == Emphasis::Terminal)
        .map(|(at, _)| format!("n{at}"))
        .collect();
    if !missing.is_empty() {
        let _ = writeln!(out, "  classDef missing stroke-dasharray: 4 3");
        let _ = writeln!(out, "  class {} missing", missing.join(","));
    }
    out
}

fn mermaid_label(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            ':' => out.push_str("&#58;"),
            '|' => out.push_str("&#124;"),
            '\n' | '\r' => out.push(' '),
            other if other.is_control() => out.push('\u{FFFD}'),
            other => out.push(other),
        }
    }
    out
}

fn mermaid_flowchart_label(value: &str) -> String {
    mermaid_label(value)
        .replace('[', "#91;")
        .replace(']', "#93;")
}

/// A standalone SVG, laid out here rather than by a tool nobody in this repository controls.
///
/// Back edges are drawn under the boxes with a dashed stroke, so a ladder that loops reads as a
/// ladder that loops rather than as a line disappearing behind a rectangle.
#[must_use]
pub fn svg(graph: &Graph, layout: &Layout) -> String {
    let boxes = places(graph, layout);
    let width = boxes
        .iter()
        .map(|place| place.x + place.width)
        .max()
        .unwrap_or(0)
        + MARGIN;
    let height = boxes
        .iter()
        .map(|place| place.y + BOX_HEIGHT)
        .max()
        .unwrap_or(0)
        + MARGIN;

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="{}">"#,
        escape(&graph.title)
    );
    let _ = writeln!(
        out,
        r#"  <style>.b{{fill:#fff;stroke:#333;stroke-width:1.5}}.e{{stroke-width:3}}.t{{fill:#f6f6f6}}
  text{{font:13px ui-monospace,SFMono-Regular,Menlo,monospace;fill:#111}}
  .l{{font-size:11px;fill:#555}}.edge{{stroke:#666;fill:none}}.back{{stroke-dasharray:4 3}}</style>"#
    );
    let _ = writeln!(
        out,
        r##"  <defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#666"/></marker></defs>"##
    );

    for (at, edge) in graph.edges.iter().enumerate() {
        let (Some(from), Some(to)) = (find(graph, &edge.from), find(graph, &edge.to)) else {
            continue;
        };
        let (start, end) = (&boxes[from], &boxes[to]);
        let back = layout.back_edges.contains(&at);
        let path = if from == to {
            // A self-loop leaves the top and comes back to it; anything else would be a line of
            // zero length that a reader cannot see and a renderer cannot put an arrowhead on.
            let top = start.y;
            let mid = start.x + start.width / 2;
            format!(
                "M{mid},{top} C{},{} {},{} {mid},{top}",
                mid - 26,
                top.saturating_sub(34),
                mid + 26,
                top.saturating_sub(34)
            )
        } else if back {
            // Down, across underneath, and up into the target's foot — so an edge running
            // right-to-left never crosses the boxes between its ends.
            let below = start.y.max(end.y) + BOX_HEIGHT + 14;
            let (leave, arrive) = (start.x + start.width / 2, end.x + end.width / 2);
            format!(
                "M{leave},{} L{leave},{below} L{arrive},{below} L{arrive},{}",
                start.y + BOX_HEIGHT,
                end.y + BOX_HEIGHT
            )
        } else {
            format!(
                "M{},{} L{},{}",
                start.x + start.width,
                start.y + BOX_HEIGHT / 2,
                end.x,
                end.y + BOX_HEIGHT / 2
            )
        };
        let class = if back { "edge back" } else { "edge" };
        let _ = writeln!(
            out,
            r#"  <path class="{class}" d="{path}" marker-end="url(#a)"/>"#
        );
        let _ = writeln!(
            out,
            r#"  <text class="l" x="{}" y="{}" text-anchor="middle">{}</text>"#,
            (start.x + start.width + end.x) / 2,
            (start.y + end.y) / 2 + BOX_HEIGHT / 2 - 6,
            escape(&edge.label)
        );
    }

    for (at, node) in graph.nodes.iter().enumerate() {
        let place = &boxes[at];
        let class = match node.emphasis {
            Emphasis::Entry => "b e",
            Emphasis::Terminal => "b t",
            Emphasis::Plain => "b",
        };
        let _ = writeln!(
            out,
            r#"  <rect class="{class}" x="{}" y="{}" width="{}" height="{BOX_HEIGHT}" rx="4"/>"#,
            place.x, place.y, place.width
        );
        let _ = writeln!(
            out,
            r#"  <text x="{}" y="{}" text-anchor="middle">{}</text>"#,
            place.x + place.width / 2,
            place.y + BOX_HEIGHT / 2 + 5,
            escape(&node.label)
        );
    }
    let _ = writeln!(out, "</svg>");
    out
}

/// One self-contained page: the drawing, then the same thing as a table.
///
/// The table is not decoration. An SVG is not readable by everything that reads a page, and a
/// drawing whose content cannot be reached any other way is a drawing that excludes readers.
#[must_use]
pub fn html(graph: &Graph, layout: &Layout) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<title>{}</title>",
        escape(&graph.title)
    );
    let _ = writeln!(
        out,
        "<style>body{{font:14px ui-monospace,SFMono-Regular,Menlo,monospace;margin:2rem;color:#111}}\ntable{{border-collapse:collapse;margin-top:1.5rem}}td,th{{border:1px solid #ddd;padding:.35rem .6rem;text-align:left}}\nth{{background:#f6f6f6}}</style>"
    );
    let _ = writeln!(out, "<h1>{}</h1>", escape(&graph.title));
    let _ = out.write_str(&svg(graph, layout));
    let _ = writeln!(
        out,
        "<table>\n<tr><th>from</th><th>edge</th><th>to</th></tr>"
    );
    for edge in &graph.edges {
        let _ = writeln!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&edge.from),
            escape(&edge.label),
            escape(&edge.to)
        );
    }
    let _ = writeln!(out, "</table>\n</html>");
    out
}

/// Where one box sits, in whole user units.
struct Place {
    x: usize,
    y: usize,
    width: usize,
}

fn places(graph: &Graph, layout: &Layout) -> Vec<Place> {
    let mut places: Vec<Place> = graph
        .nodes
        .iter()
        .map(|node| Place {
            x: 0,
            y: 0,
            width: node.label.chars().count() * CELL + PAD * 2,
        })
        .collect();

    let mut x = MARGIN;
    for layer in &layout.layers {
        let widest = layer.iter().map(|at| places[*at].width).max().unwrap_or(0);
        for (row, at) in layer.iter().enumerate() {
            places[*at].x = x;
            places[*at].y = MARGIN + row * (BOX_HEIGHT + ROW_GAP);
        }
        x += widest + LAYER_GAP;
    }
    places
}

/// Which node an edge names.
///
/// The **last** node with that id, matching how [`Layout`] builds its index — a `BTreeMap`
/// collected in order, where a later duplicate replaces an earlier one. They disagreed before: the
/// layout laid out one node and the renderer drew the arrow into another, so a back edge left a box
/// and re-entered the same box. Duplicate ids are not reachable through the two constructors, but
/// `Graph`'s fields are public and a caller can build one.
fn find(graph: &Graph, id: &str) -> Option<usize> {
    graph.nodes.iter().rposition(|node| node.id == id)
}

/// A DOT string literal. A name carrying a quote or a backslash must not close the string it is
/// written into (R-95).
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        if character == '"' || character == '\\' {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}

/// XML text content and attribute values.
///
/// Five characters are escaped, and a sixth class is **replaced**: XML 1.0 permits only tab, line
/// feed and carriage return below `U+0020`, and the rest have no escape — `&#1;` is as invalid as
/// the raw byte. A state name carrying `U+0001` used to produce an SVG no parser accepts and an
/// HTML document no browser accepts, from a definition `entity validate` had passed. So an
/// unrepresentable character becomes `U+FFFD`, which is visible in the drawing and valid in the
/// document: silently dropping it would make two different names draw the same box.
///
/// Found by an independent review after 0.4.0 shipped. R-95 claimed "valid DOT, valid SVG and a
/// valid HTML page" and was pinned only for DOT.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other if is_xml_char(other) => out.push(other),
            _ => out.push('\u{FFFD}'),
        }
    }
    out
}

/// Whether XML 1.0 permits the character at all.
///
/// `Char ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`. Rust has no
/// surrogates in a `char`, so only the low controls and the two non-characters at the end of the
/// BMP need excluding.
fn is_xml_char(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r')
        || matches!(character, ' '..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}
