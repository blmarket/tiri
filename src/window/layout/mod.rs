//! Window layouts. A layout is a pure function of the window's pane list:
//! each pane's place in the list and its layout metadata, such as its strip
//! width, give the rectangles. The window stores only which layout it uses,
//! a `LayoutKind`, whose functions are the only layout code that touches
//! panes: they dispatch to the layout's `Layout` implementation, which reads
//! each pane's metadata and calls the layout's own pure functions. A layout's
//! metadata type is private to its module.

mod presets;
mod scrolling;
mod tiling;

use crate::shared::layout::{layout_geometry, Direction};
use crate::shared::pane::{window_pane, PANE_MINIMUM};
use std::any::Any;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::rc::Rc;

/// What a user can ask a layout to do to one pane. Layouts that can't do it
/// refuse.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayoutAction {
    /// Switch a pane between its two widths: in the strip, half and full.
    ToggleWidth,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutKind {
    #[default]
    Scrolling,
    Tiling,
    MainVertical,
    MainCentered,
    MainHorizontal,
    Grid,
}

/// One layout's behaviour. A layout keeps no state of its own, so its
/// functions take no receiver and `LayoutKind` picks the implementation
/// statically. The defaults are those of a layout that fills the window with
/// a shape, as tmux's layouts do.
trait Layout {
    /// The shape of `count` panes, at least one.
    fn shape(count: usize, size: (u32, u32)) -> LayoutNode;

    /// The panes' rectangles in list order: by default the shape, filling the
    /// window.
    fn arrange(panes: &[Rc<UnsafeCell<window_pane>>], size: (u32, u32)) -> Vec<layout_geometry> {
        let tree = (!panes.is_empty()).then(|| Self::shape(panes.len(), size));
        place(tree, panes.len(), size)
    }

    /// Whether the layout takes the pane list in this window, or why not: by
    /// default while every pane stays inside the window.
    fn admits(
        panes: &[Rc<UnsafeCell<window_pane>>],
        size: (u32, u32),
    ) -> Result<(), &'static CStr> {
        fits(
            &Self::arrange(panes, size),
            size,
            c"no space for a new pane: the panes would not fit in the window",
        )
    }

    /// The area a view can scroll across: by default the window itself.
    /// Builds no rectangles.
    fn logical_size(
        _panes: impl Iterator<Item = Rc<UnsafeCell<window_pane>>>,
        size: (u32, u32),
    ) -> (u32, u32) {
        size
    }

    /// A pane's metadata after `action`, or why this layout can't do it: by
    /// default it can't.
    fn apply(
        action: LayoutAction,
        _meta: Option<&dyn Any>,
    ) -> Result<Option<Box<dyn Any>>, &'static CStr> {
        match action {
            LayoutAction::ToggleWidth => Err(c"this layout can't change a pane's width"),
        }
    }

    /// Whether a separator with a pane on each side splits its colour, half
    /// in each pane's border style, to show which side is active. By default
    /// only with two panes, as tmux does: every pane is in view, so two panes
    /// share one separator, and more show the active one by the outline of
    /// its borders.
    fn splits_separator_colours(panes: usize) -> bool {
        panes == 2
    }
}

/// `$body` with `$L` naming the `Layout` implementation of `$kind`.
macro_rules! with_layout {
    ($kind:expr, $L:ident => $body:expr) => {
        match $kind {
            LayoutKind::Scrolling => {
                type $L = scrolling::Scrolling;
                $body
            }
            LayoutKind::Tiling => {
                type $L = tiling::Tiling;
                $body
            }
            LayoutKind::MainVertical => {
                type $L = presets::MainVertical;
                $body
            }
            LayoutKind::MainCentered => {
                type $L = presets::MainCentered;
                $body
            }
            LayoutKind::MainHorizontal => {
                type $L = presets::MainHorizontal;
                $body
            }
            LayoutKind::Grid => {
                type $L = presets::Grid;
                $body
            }
        }
    };
}

impl LayoutKind {
    const ALL: [LayoutKind; 6] = [
        LayoutKind::Scrolling,
        LayoutKind::Tiling,
        LayoutKind::MainVertical,
        LayoutKind::MainCentered,
        LayoutKind::MainHorizontal,
        LayoutKind::Grid,
    ];

    pub fn name(self) -> &'static CStr {
        match self {
            LayoutKind::Scrolling => c"scrolling",
            LayoutKind::Tiling => c"tiling",
            LayoutKind::MainVertical => c"main-vertical",
            LayoutKind::MainCentered => c"main-centered",
            LayoutKind::MainHorizontal => c"main-horizontal",
            LayoutKind::Grid => c"grid",
        }
    }

    pub fn from_name(name: &CStr) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The layout after this one, wrapping around.
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|&kind| kind == self)
            .expect("listed layout");
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// The panes as a split tree, for the layout string; none without panes.
    pub(super) fn tree(
        self,
        panes: &[Rc<UnsafeCell<window_pane>>],
        size: (u32, u32),
    ) -> Option<LayoutNode> {
        (!panes.is_empty()).then(|| with_layout!(self, L => L::shape(panes.len(), size)))
    }

    /// The panes' rectangles in list order.
    pub(super) fn arrange(
        self,
        panes: &[Rc<UnsafeCell<window_pane>>],
        size: (u32, u32),
    ) -> Vec<layout_geometry> {
        with_layout!(self, L => L::arrange(panes, size))
    }

    /// Whether this layout takes the pane list in this window, or why not.
    /// A window may still hold a list its layout would not take, after a
    /// resize; only a change to the list is refused.
    pub(super) fn admits(
        self,
        panes: &[Rc<UnsafeCell<window_pane>>],
        size: (u32, u32),
    ) -> Result<(), &'static CStr> {
        with_layout!(self, L => L::admits(panes, size))
    }

    /// The area a view can scroll across: the strip's length by the window
    /// height, or the window itself in the other layouts. Builds no
    /// rectangles.
    pub(super) fn logical_size(
        self,
        panes: impl Iterator<Item = Rc<UnsafeCell<window_pane>>>,
        size: (u32, u32),
    ) -> (u32, u32) {
        with_layout!(self, L => L::logical_size(panes, size))
    }

    /// A pane's metadata after `action`, or why this layout can't do it.
    pub(super) fn apply(
        self,
        action: LayoutAction,
        meta: Option<&dyn Any>,
    ) -> Result<Option<Box<dyn Any>>, &'static CStr> {
        with_layout!(self, L => L::apply(action, meta))
    }

    /// Whether a separator between two of the window's `panes` panes splits
    /// its colour between their border styles.
    pub fn splits_separator_colours(self, panes: usize) -> bool {
        with_layout!(self, L => L::splits_separator_colours(panes))
    }
}

/// The whole window of size `(sx, sy)` as a rectangle.
fn window_geometry((sx, sy): (u32, u32)) -> layout_geometry {
    layout_geometry {
        sx,
        sy,
        xoff: 0,
        yoff: 0,
    }
}

/// `tree`, for `count` panes, placed in the window: the rectangles fill it.
fn place(tree: Option<LayoutNode>, count: usize, size: (u32, u32)) -> Vec<layout_geometry> {
    let mut cells = vec![layout_geometry::default(); count];
    if let Some(tree) = tree {
        tree.place(window_geometry(size), &mut cells);
    }
    cells
}

/// Whether every rectangle of `cells` stays inside the window, or `reason`.
fn fits(
    cells: &[layout_geometry],
    (sx, sy): (u32, u32),
    reason: &'static CStr,
) -> Result<(), &'static CStr> {
    cells
        .iter()
        .all(|cell| cell.xoff as u32 + cell.sx <= sx && cell.yoff as u32 + cell.sy <= sy)
        .then_some(())
        .ok_or(reason)
}

/// Columns the panes occupy, up to the rightmost pane's right edge.
pub(super) fn content_width(cells: &[layout_geometry]) -> u32 {
    cells
        .iter()
        .map(|cell| (cell.xoff as u32).saturating_add(cell.sx))
        .max()
        .unwrap_or(0)
}

/// The rectangle across one separator from `cells[from]` in `direction` that
/// shares the most of that edge, the earliest on a tie. Every layout's panes
/// are a partition with one-cell separators, so the rectangles alone decide
/// it; in the strip that is the previous or next pane, and nothing above or
/// below.
pub(super) fn adjacent(
    cells: &[layout_geometry],
    from: usize,
    direction: Direction,
) -> Option<usize> {
    let source = cells[from];
    let span = |start: i32, size: u32| (start, start + size as i32);
    let overlap = |(a0, a1): (i32, i32), (b0, b1): (i32, i32)| a1.min(b1) - a0.max(b0);
    let mut best: Option<(usize, i32)> = None;
    for (index, cell) in cells.iter().enumerate() {
        let (touches, shared) = match direction {
            Direction::Left => (
                cell.xoff + cell.sx as i32 + 1 == source.xoff,
                overlap(span(cell.yoff, cell.sy), span(source.yoff, source.sy)),
            ),
            Direction::Right => (
                source.xoff + source.sx as i32 + 1 == cell.xoff,
                overlap(span(cell.yoff, cell.sy), span(source.yoff, source.sy)),
            ),
            Direction::Up => (
                cell.yoff + cell.sy as i32 + 1 == source.yoff,
                overlap(span(cell.xoff, cell.sx), span(source.xoff, source.sx)),
            ),
            Direction::Down => (
                source.yoff + source.sy as i32 + 1 == cell.yoff,
                overlap(span(cell.xoff, cell.sx), span(source.xoff, source.sx)),
            ),
        };
        if touches && shared > 0 && best.is_none_or(|(_, most)| shared > most) {
            best = Some((index, shared));
        }
    }
    best.map(|(index, _)| index)
}

/// A layout as a tree: a pane, by index into the pane order, or a split of
/// children side by side (`across`) or stacked, each with its weight in the
/// split's extent.
pub(super) enum LayoutNode {
    Pane(usize),
    Split {
        across: bool,
        children: Vec<(u32, LayoutNode)>,
    },
}

impl LayoutNode {
    /// `panes`, by index, in one split of equal weights; a lone pane stands
    /// for itself.
    pub(super) fn line(across: bool, panes: impl IntoIterator<Item = usize>) -> LayoutNode {
        let mut children = panes
            .into_iter()
            .map(|index| (1, LayoutNode::Pane(index)))
            .collect::<Vec<_>>();
        if children.len() == 1 {
            children.pop().expect("one pane").1
        } else {
            LayoutNode::Split { across, children }
        }
    }

    /// `count` panes in an even grid: rows of as many columns as the smallest
    /// square that holds them all, the last row taking the rest.
    pub(super) fn grid(count: usize) -> LayoutNode {
        let columns = (1..)
            .find(|columns| columns * columns >= count)
            .expect("a square holds every pane");
        let rows = (0..count)
            .step_by(columns)
            .map(|start| {
                (
                    1,
                    LayoutNode::line(true, start..(start + columns).min(count)),
                )
            })
            .collect::<Vec<_>>();
        if rows.len() == 1 {
            rows.into_iter().next().expect("one row").1
        } else {
            LayoutNode::Split {
                across: false,
                children: rows,
            }
        }
    }

    /// Give every pane under this node its rectangle within `g`, by index
    /// into `cells`. A split shares its extent by weight, one separator
    /// between neighbours, the remainder going to the first children. No pane
    /// is narrower or shorter than PANE_MINIMUM, even when that takes it past
    /// `g`.
    pub(super) fn place(&self, g: layout_geometry, cells: &mut [layout_geometry]) {
        let LayoutNode::Split { across, children } = self else {
            let LayoutNode::Pane(index) = self else {
                unreachable!()
            };
            cells[*index] = g;
            return;
        };
        let count = children.len() as u32;
        let extent = if *across { g.sx } else { g.sy };
        let shared = extent.saturating_sub(count - 1);
        let total = children
            .iter()
            .map(|(weight, _)| weight)
            .sum::<u32>()
            .max(1);
        let sizes = children
            .iter()
            .map(|(weight, _)| shared * weight / total)
            .collect::<Vec<_>>();
        let remainder = shared - sizes.iter().sum::<u32>();
        let mut offset = 0u32;
        for (index, ((_, child), size)) in children.iter().zip(sizes).enumerate() {
            let size = (size + u32::from((index as u32) < remainder)).max(PANE_MINIMUM as u32);
            let child_g = if *across {
                layout_geometry {
                    sx: size,
                    xoff: g.xoff + offset as i32,
                    ..g
                }
            } else {
                layout_geometry {
                    sy: size,
                    yoff: g.yoff + offset as i32,
                    ..g
                }
            };
            child.place(child_g, cells);
            offset = offset.saturating_add(size + 1);
        }
    }
}

/// One pane's entry in a layout string, copied before serialization.
pub(super) struct LayoutEntry {
    pub(super) geometry: layout_geometry,
    pub(super) id: u32,
    pub(super) index: u32,
    pub(super) active: bool,
    /// Position in the selection history, when the pane is in it.
    pub(super) last: Option<u32>,
}

fn layout_checksum(layout: &[u8]) -> u16 {
    layout.iter().fold(0u16, |checksum, &byte| {
        checksum
            .rotate_right(1)
            .wrapping_add(byte as ::core::ffi::c_char as u16)
    })
}

impl LayoutNode {
    /// The rectangle a node covers: its pane's, or the bounds of its
    /// children's.
    fn geometry(&self, entries: &[LayoutEntry]) -> layout_geometry {
        match self {
            LayoutNode::Pane(index) => entries[*index].geometry,
            LayoutNode::Split { children, .. } => {
                let cells = children
                    .iter()
                    .map(|(_, child)| child.geometry(entries))
                    .collect::<Vec<_>>();
                let xoff = cells.iter().map(|g| g.xoff).min().unwrap_or(0);
                let yoff = cells.iter().map(|g| g.yoff).min().unwrap_or(0);
                let right = cells
                    .iter()
                    .map(|g| g.xoff + g.sx as i32)
                    .max()
                    .unwrap_or(0);
                let bottom = cells
                    .iter()
                    .map(|g| g.yoff + g.sy as i32)
                    .max()
                    .unwrap_or(0);
                layout_geometry {
                    sx: (right - xoff) as u32,
                    sy: (bottom - yoff) as u32,
                    xoff,
                    yoff,
                }
            }
        }
    }

    fn write_legacy(&self, entries: &[LayoutEntry], output: &mut Vec<u8>) {
        let g = self.geometry(entries);
        output.extend_from_slice(format!("{}x{},{},{}", g.sx, g.sy, g.xoff, g.yoff).as_bytes());
        match self {
            LayoutNode::Pane(index) => {
                output.extend_from_slice(format!(",{}", entries[*index].id).as_bytes())
            }
            LayoutNode::Split { across, children } => {
                let (open, close) = if *across { (b'{', b'}') } else { (b'[', b']') };
                output.push(open);
                for (position, (_, child)) in children.iter().enumerate() {
                    if position != 0 {
                        output.push(b',');
                    }
                    child.write_legacy(entries, output);
                }
                output.push(close);
            }
        }
    }

    fn write_json(&self, entries: &[LayoutEntry], output: &mut Vec<u8>) {
        match self {
            LayoutNode::Pane(index) => append_pane(output, &entries[*index]),
            LayoutNode::Split { across, children } => {
                let g = self.geometry(entries);
                output.extend_from_slice(
                    format!(
                        "{{\"t\":\"{}\",\"w\":{},\"h\":{},\"x\":{},\"y\":{},\"c\":[",
                        if *across { "h" } else { "v" },
                        g.sx,
                        g.sy,
                        g.xoff,
                        g.yoff
                    )
                    .as_bytes(),
                );
                for (position, (_, child)) in children.iter().enumerate() {
                    if position != 0 {
                        output.push(b',');
                    }
                    child.write_json(entries, output);
                }
                output.extend_from_slice(b"]}");
            }
        }
    }
}

/// Serialize a layout: `tree` over `entries`, the panes in pane order.
/// `legacy` selects the checksummed text format kept for older control
/// clients. A window without panes has no layout; the legacy format still
/// gives "0000,", as an empty string crashes old iTerm2 versions.
pub(super) fn layout_string(
    tree: Option<&LayoutNode>,
    entries: &[LayoutEntry],
    legacy: bool,
) -> Option<CString> {
    let Some(tree) = tree.filter(|_| !entries.is_empty()) else {
        return legacy.then(|| c"0000,".to_owned());
    };
    let mut output = Vec::new();
    if legacy {
        let mut body = Vec::new();
        tree.write_legacy(entries, &mut body);
        output.extend_from_slice(format!("{:04x},", layout_checksum(&body)).as_bytes());
        output.extend_from_slice(&body);
    } else {
        output.extend_from_slice(b"{\"V\":2,\"L\":");
        tree.write_json(entries, &mut output);
        output.push(b'}');
    }
    Some(CString::new(output).expect("layout serializer produced an interior NUL"))
}

fn append_pane(output: &mut Vec<u8>, entry: &LayoutEntry) {
    let g = entry.geometry;
    output.extend_from_slice(
        format!(
            "{{\"t\":\"p\",\"w\":{},\"h\":{},\"x\":{},\"y\":{}",
            g.sx, g.sy, g.xoff, g.yoff
        )
        .as_bytes(),
    );
    if entry.active {
        output.extend_from_slice(b",\"a\":true");
    } else if let Some(last) = entry.last {
        output.extend_from_slice(format!(",\"l\":{last}").as_bytes());
    }
    output
        .extend_from_slice(format!(",\"i\":{},\"I\":\"%{}\"}}", entry.index, entry.id).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(xoff: i32, id: u32, index: u32, active: bool, last: Option<u32>) -> LayoutEntry {
        LayoutEntry {
            geometry: layout_geometry {
                sx: 39,
                sy: 24,
                xoff,
                yoff: 0,
            },
            id,
            index,
            active,
            last,
        }
    }

    fn row(count: usize) -> LayoutNode {
        LayoutNode::Split {
            across: true,
            children: (0..count)
                .map(|index| (1, LayoutNode::Pane(index)))
                .collect(),
        }
    }

    #[test]
    fn layout_string_is_a_single_row() {
        let entries = [entry(0, 3, 0, false, Some(0)), entry(40, 5, 1, true, None)];
        assert_eq!(
            layout_string(Some(&row(2)), &entries, false)
                .unwrap()
                .to_str()
                .unwrap(),
            concat!(
                r#"{"V":2,"L":{"t":"h","w":79,"h":24,"x":0,"y":0,"c":["#,
                r#"{"t":"p","w":39,"h":24,"x":0,"y":0,"l":0,"i":0,"I":"%3"},"#,
                r#"{"t":"p","w":39,"h":24,"x":40,"y":0,"a":true,"i":1,"I":"%5"}]}}"#
            )
        );
        let body = b"79x24,0,0{39x24,0,0,3,39x24,40,0,5}";
        assert_eq!(
            layout_string(Some(&row(2)), &entries, true)
                .unwrap()
                .to_bytes(),
            [
                format!("{:04x},", layout_checksum(body)).as_bytes(),
                &body[..]
            ]
            .concat()
        );
        assert!(layout_string(Some(&row(0)), &[], false).is_none());
        assert!(layout_string(None, &[], false).is_none());
        assert_eq!(layout_string(None, &[], true).unwrap().to_bytes(), b"0000,");
    }

    #[test]
    fn a_lone_pane_is_the_root_cell() {
        let entries = [entry(0, 7, 1, true, None)];
        let lone = LayoutNode::Pane(0);
        assert_eq!(
            layout_string(Some(&lone), &entries, false)
                .unwrap()
                .to_str()
                .unwrap(),
            r#"{"V":2,"L":{"t":"p","w":39,"h":24,"x":0,"y":0,"a":true,"i":1,"I":"%7"}}"#
        );
        let body = b"39x24,0,0,7";
        assert_eq!(
            layout_string(Some(&lone), &entries, true)
                .unwrap()
                .to_bytes(),
            [
                format!("{:04x},", layout_checksum(body)).as_bytes(),
                &body[..]
            ]
            .concat()
        );
    }

    #[test]
    fn stacked_splits_nest_in_the_layout_string() {
        let mut entries = [
            entry(0, 1, 0, true, None),
            entry(40, 2, 1, false, None),
            entry(40, 3, 2, false, None),
        ];
        entries[1].geometry.sy = 12;
        entries[2].geometry.sy = 11;
        entries[2].geometry.yoff = 13;
        let tree = LayoutNode::Split {
            across: true,
            children: vec![(1, LayoutNode::Pane(0)), (1, LayoutNode::line(false, 1..3))],
        };
        assert_eq!(
            layout_string(Some(&tree), &entries, true)
                .unwrap()
                .to_bytes()[5..],
            b"79x24,0,0{39x24,0,0,1,39x24,40,0[39x12,40,0,2,39x11,40,13,3]}"[..]
        );
        let json = layout_string(Some(&tree), &entries, false).unwrap();
        assert!(json
            .to_str()
            .unwrap()
            .contains(r#"{"t":"v","w":39,"h":24,"x":40,"y":0,"c":["#));
    }

    fn rect(sx: u32, sy: u32, xoff: i32, yoff: i32) -> layout_geometry {
        layout_geometry { sx, sy, xoff, yoff }
    }

    #[test]
    fn a_tiled_neighbour_shares_the_most_of_the_edge() {
        let cells = [
            rect(40, 24, 0, 0),
            rect(39, 12, 41, 0),
            rect(39, 11, 41, 13),
        ];
        assert_eq!(adjacent(&cells, 0, Direction::Right), Some(1));
        assert_eq!(adjacent(&cells, 2, Direction::Left), Some(0));
        assert_eq!(adjacent(&cells, 1, Direction::Down), Some(2));
        assert_eq!(adjacent(&cells, 2, Direction::Up), Some(1));
        assert_eq!(adjacent(&cells, 0, Direction::Left), None);
        assert_eq!(adjacent(&cells, 0, Direction::Up), None);
        let row = [
            rect(39, 24, 0, 0),
            rect(80, 24, 40, 0),
            rect(39, 24, 121, 0),
        ];
        assert_eq!(adjacent(&row, 0, Direction::Right), Some(1));
        assert_eq!(adjacent(&row, 2, Direction::Left), Some(1));
        assert_eq!(adjacent(&row, 2, Direction::Right), None);
        assert_eq!(adjacent(&row, 1, Direction::Down), None);
        assert_eq!(adjacent(&row, 1, Direction::Up), None);
    }

    #[test]
    fn layout_names_round_trip_and_cycle() {
        for kind in LayoutKind::ALL {
            assert_eq!(LayoutKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(LayoutKind::from_name(c"tiled"), None);
        assert_eq!(LayoutKind::Scrolling.next(), LayoutKind::Tiling);
        assert_eq!(LayoutKind::Grid.next(), LayoutKind::Scrolling);
    }
}
