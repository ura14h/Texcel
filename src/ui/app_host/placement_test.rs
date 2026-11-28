use super::*;

#[test]
// Keeps the size and moves one step down and to the right.
fn new_window_follows_one_step_down_and_right() {
    let base = WindowPlacement {
        position: Pos2::new(100.0, 80.0),
        size: Vec2::new(900.0, 600.0),
    };
    let next = cascade_after(base, Some(Vec2::new(1920.0, 1080.0)));
    assert_eq!(next.position, Pos2::new(124.0, 104.0));
    assert_eq!(next.size, base.size);
}

#[test]
// Starts again at the screen's top-left corner when the next step would run off the screen.
fn cascade_returns_to_the_corner_at_the_screen_edge() {
    let base = WindowPlacement {
        position: Pos2::new(1000.0, 450.0),
        size: Vec2::new(900.0, 600.0),
    };
    let next = cascade_after(base, Some(Vec2::new(1920.0, 1080.0)));
    assert_eq!(next.position, Pos2::new(CASCADE_STEP, CASCADE_STEP));
    assert_eq!(next.size, base.size);
}
