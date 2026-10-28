// src-native/src/ui/geo.rs
//
// Where everything sits, as the page's flex layout places it: the shell inset
// 44px from the top and 12px elsewhere, the icon column bled to the window's
// edges, the side bar at 23% of the shell, and the image slot between them.

use egui::{pos2, Rect};

#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub screen: Rect,
    pub colour_bar: Option<Rect>,
    pub slot: Rect,
    /// The box a fitted image is centred and fitted in.
    pub center: Rect,
    pub histogram: Rect,
    pub tools: Rect,
    pub footer: Rect,
}

impl Geo {
    pub fn new(screen: Rect, colour_bar: bool) -> Self {
        let shell = Rect::from_min_max(pos2(12.0, 44.0), pos2(screen.right() - 12.0, screen.bottom() - 12.0));
        let sbw = (0.23 * shell.width()).max(300.0);
        let sidebar = Rect::from_min_max(pos2(shell.right() - sbw, shell.top()), shell.max);
        let mut x = 60.0;
        let colour_bar = colour_bar.then(|| {
            let r = Rect::from_min_max(pos2(60.0, shell.top()), pos2(360.0, shell.bottom()));
            x = 372.0;
            r
        });
        let slot = Rect::from_min_max(pos2(x, shell.top()), pos2((sidebar.left() - 12.0).max(x), shell.bottom()));
        let center = Rect::from_min_max(
            pos2(slot.left() + 29.0, slot.top() + 36.0),
            pos2((slot.right() - 36.0).max(slot.left() + 29.0), (slot.bottom() - 36.0).max(slot.top() + 36.0)),
        );
        let histogram = Rect::from_min_size(sidebar.min, egui::vec2(sbw, 10.0 + (sbw - 10.0) / 2.0));
        let footer = Rect::from_min_max(pos2(sidebar.left(), sidebar.bottom() - 48.0), sidebar.max);
        let tools = Rect::from_min_max(pos2(sidebar.left(), histogram.bottom() + 12.0), pos2(sidebar.right(), footer.top() - 12.0));
        Self { screen, colour_bar, slot, center, histogram, tools, footer }
    }
}
