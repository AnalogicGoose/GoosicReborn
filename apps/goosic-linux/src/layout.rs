//! Window geometry, independent of GTK. Breakpoints follow the Windows shell.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub narrow: bool,
    pub sidebar: i32,
    pub panel: i32,
    pub left: i32,
    pub right: i32,
    pub player_left: i32,
    pub player_width: i32,
    pub stacked_player: bool,
}

pub fn window(width: i32, sidebar: bool, panel: bool) -> Layout {
    let width = width.max(480);
    let narrow = width < 820;
    let sidebar_width = if width >= 1200 { 280 } else { 236 };
    let panel_width = if width >= 1200 { 360 } else { 300 };
    let left = if sidebar && !narrow {
        sidebar_width + 16
    } else {
        0
    };
    let right = if panel && !narrow {
        panel_width + 16
    } else {
        0
    };
    // When both panels leave too little room, the right panel floats over the page.
    let right = if width - left - right < 440 { 0 } else { right };
    let available = width - left - right - 40;
    let player_width = available.min(740);
    let player_left = ((left + width - player_width) / 2)
        .min(width - right - 20 - player_width)
        .max(left + 20);
    Layout {
        narrow,
        sidebar: sidebar_width,
        panel: panel_width,
        left,
        right,
        player_left,
        player_width,
        stacked_player: player_width < 620,
    }
}

pub fn cover_size(width: i32, height: i32) -> i32 {
    (height - 320).min(width - 80).clamp(0, 440)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_overlay_at_narrow_width_and_the_transport_stays_inside() {
        for width in [480, 600, 819, 820, 1100, 1200, 1600] {
            for sidebar in [false, true] {
                for panel in [false, true] {
                    let layout = window(width, sidebar, panel);
                    assert!(layout.player_left >= 20);
                    assert!(layout.player_left + layout.player_width <= width - 20);
                    assert!(layout.player_width >= 384);
                    if layout.narrow {
                        assert_eq!((layout.left, layout.right), (0, 0));
                    }
                }
            }
        }
    }

    #[test]
    fn wide_windows_limit_the_pill_and_short_windows_shrink_the_cover() {
        assert_eq!(window(1800, true, true).player_width, 740);
        assert!(window(480, false, false).stacked_player);
        assert_eq!(cover_size(1100, 560), 240);
        assert_eq!(cover_size(1100, 900), 440);
    }
}
