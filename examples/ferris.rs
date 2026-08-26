use lumin_wallpaper_rs::{AttachWindow, EventForwarder};
use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Ferris Follows the Cursor".to_owned(),
        window_width: 900,
        window_height: 700,
        window_resizable: true,
        ..Default::default()
    }
}

use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::FindWindowW};
use windows::core::w;

fn get_hwnd() -> Option<HWND> {
    unsafe { FindWindowW(None, w!("Ferris Follows the Cursor")).ok() }
}
fn draw_ferris(pos: Vec2, look_dir: Vec2, time: f32, moving: bool) {
    let bob = if moving {
        (time * 12.0).sin() * 3.0
    } else {
        0.0
    };
    let body_y = pos.y + bob;
    let body_center = Vec2::new(pos.x, body_y);
    let body_color = Color::from_rgba(196, 85, 8, 255); // Ferris orange
    let dark_orange = Color::from_rgba(150, 60, 5, 255);
    let claw_color = Color::from_rgba(220, 100, 20, 255);
    let eye_white = Color::from_rgba(255, 255, 255, 255);
    let pupil_color = Color::from_rgba(20, 20, 20, 255);

    // Shadow (rotation = 0.0)
    draw_ellipse(
        body_center.x,
        pos.y + 30.0,
        50.0,
        12.0,
        0.0,
        Color::from_rgba(0, 0, 0, 80),
    );

    // Legs (three on each side)
    let leg_color = dark_orange;
    for i in 0..3 {
        let x_offset = 25.0 + i as f32 * 8.0;
        let y_offset = 15.0 + i as f32 * 6.0;
        // left legs
        draw_line(
            body_center.x - 30.0,
            body_center.y + 10.0,
            body_center.x - x_offset - 5.0,
            body_center.y + y_offset,
            3.0,
            leg_color,
        );
        // right legs
        draw_line(
            body_center.x + 30.0,
            body_center.y + 10.0,
            body_center.x + x_offset + 5.0,
            body_center.y + y_offset,
            3.0,
            leg_color,
        );
    }

    // Claws
    draw_circle(body_center.x - 55.0, body_center.y - 10.0, 18.0, claw_color);
    draw_circle(body_center.x + 55.0, body_center.y - 10.0, 18.0, claw_color);
    // Claw pincers
    draw_line(
        body_center.x - 55.0,
        body_center.y - 10.0,
        body_center.x - 70.0,
        body_center.y - 20.0,
        4.0,
        claw_color,
    );
    draw_line(
        body_center.x - 55.0,
        body_center.y - 10.0,
        body_center.x - 70.0,
        body_center.y,
        4.0,
        claw_color,
    );
    draw_line(
        body_center.x + 55.0,
        body_center.y - 10.0,
        body_center.x + 70.0,
        body_center.y - 20.0,
        4.0,
        claw_color,
    );
    draw_line(
        body_center.x + 55.0,
        body_center.y - 10.0,
        body_center.x + 70.0,
        body_center.y,
        4.0,
        claw_color,
    );

    // Body (rotation = 0.0)
    draw_ellipse(body_center.x, body_center.y, 80.0, 50.0, 0.0, body_color);
    // Body highlight (rotation = 0.0)
    draw_ellipse(
        body_center.x - 10.0,
        body_center.y - 8.0,
        30.0,
        18.0,
        0.0,
        Color::from_rgba(240, 130, 40, 120),
    );

    // Eye stalks
    draw_line(
        body_center.x - 18.0,
        body_center.y - 22.0,
        body_center.x - 18.0,
        body_center.y - 42.0,
        4.0,
        dark_orange,
    );
    draw_line(
        body_center.x + 18.0,
        body_center.y - 22.0,
        body_center.x + 18.0,
        body_center.y - 42.0,
        4.0,
        dark_orange,
    );

    // Eyes
    let eye_offset = look_dir.normalize_or_zero() * 2.5;
    let eye_y = body_center.y - 45.0;
    draw_circle(body_center.x - 18.0, eye_y, 7.0, eye_white);
    draw_circle(body_center.x + 18.0, eye_y, 7.0, eye_white);
    // Pupils track cursor
    draw_circle(
        body_center.x - 18.0 + eye_offset.x,
        eye_y + eye_offset.y,
        3.5,
        pupil_color,
    );
    draw_circle(
        body_center.x + 18.0 + eye_offset.x,
        eye_y + eye_offset.y,
        3.5,
        pupil_color,
    );

    // Mouth (small smile)
    draw_line(
        body_center.x - 5.0,
        body_center.y + 10.0,
        body_center.x + 5.0,
        body_center.y + 10.0,
        2.0,
        dark_orange,
    );
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut crab_pos = Vec2::new(screen_width() / 2.0, screen_height() / 2.0);
    let mut target = crab_pos;
    let mut flash = 0.0;
    let mut flash_pos = Vec2::new(0.0, 0.0);
    let hwnd = get_hwnd().unwrap();

    let click_forwarder = EventForwarder::new(hwnd.0 as _, None, true, false);
    click_forwarder.unwrap().forward_events();
    // unsafe {
    //     let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

    //     SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | MA_NOACTIVATE as isize);
    // }
    let a = AttachWindow::auto_attach(hwnd.0 as _, true);
    a.unwrap()
        .start_watcher(std::time::Duration::from_millis(100));
    loop {
        let dt = get_frame_time();
        let time = get_time();

        // Mouse input
        let (mx, my) = mouse_position();
        let mouse_pos = Vec2::new(mx, my);
        let look_dir = mouse_pos - crab_pos;

        // Determine if we are moving (left mouse held)
        let moving = is_mouse_button_down(MouseButton::Left);

        if moving {
            target = mouse_pos;
        }

        // Smooth movement toward target when mouse is held
        if moving {
            // Exponential smoothing for fluent motion
            let t = 1.0 - (-8.0 * dt).exp();
            crab_pos += (target - crab_pos) * t;
        }

        // Click flash
        if is_mouse_button_pressed(MouseButton::Left) {
            flash = 1.0;
            flash_pos = mouse_pos;
        }
        if flash > 0.0 {
            flash = (flash - dt * 2.0).max(0.0);
        }

        // Drawing
        clear_background(Color::from_rgba(18, 18, 30, 255)); // dark theme

        // Draw grid
        let grid_color = Color::from_rgba(45, 45, 65, 120);
        let step = 40.0;
        let mut x = 0.0;
        while x < screen_width() {
            draw_line(x, 0.0, x, screen_height(), 1.0, grid_color);
            x += step;
        }
        let mut y = 0.0;
        while y < screen_height() {
            draw_line(0.0, y, screen_width(), y, 1.0, grid_color);
            y += step;
        }

        // "RUST" background text
        let text_color = if flash > 0.0 {
            let glow = (flash * 255.0) as u8;
            Color::from_rgba(255, 150, 40, glow)
        } else {
            Color::from_rgba(180, 70, 20, 120)
        };
        draw_text("RUST", 25.0, 55.0, 70.0, text_color);

        // Click flash glow (radial)
        if flash > 0.0 {
            let alpha = (flash * 0.35 * 255.0) as u8;
            let radius = 60.0 + (1.0 - flash) * 250.0;
            draw_circle(
                flash_pos.x,
                flash_pos.y,
                radius,
                Color::from_rgba(255, 150, 50, alpha),
            );
            // full screen subtle glow
            let overlay_alpha = (flash * 0.05 * 255.0) as u8;
            draw_rectangle(
                0.0,
                0.0,
                screen_width(),
                screen_height(),
                Color::from_rgba(255, 120, 30, overlay_alpha),
            );
        }

        // Draw Ferris
        draw_ferris(crab_pos, look_dir, time as _, moving);

        // Instructions
        draw_text(
            "Hold left mouse button to make Ferris walk",
            10.0,
            screen_height() - 10.0,
            20.0,
            Color::from_rgba(200, 200, 200, 180),
        );

        next_frame().await
    }
}
