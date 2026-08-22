use tao::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;

fn main() {
    // Create the event loop
    let event_loop = EventLoop::new();

    // Create a window
    let _window = WindowBuilder::new()
        .with_title("Mouse Events Demo")
        .with_inner_size(tao::dpi::LogicalSize::new(600.0, 400.0))
        .build(&event_loop)
        .unwrap();

    println!("=== Mouse Events Demo ===");
    println!("Move your mouse over the window, click buttons, and scroll!");
    println!("Press ESC to exit\n");

    // Run the event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    // Mouse moved
                    WindowEvent::CursorMoved { position, .. } => {
                        println!("🖱️  Mouse Moved: x={:.1}, y={:.1}", position.x, position.y);
                    }

                    // Mouse entered window
                    WindowEvent::CursorEntered { .. } => {
                        println!("✨ Mouse entered window");
                    }

                    // Mouse left window
                    WindowEvent::CursorLeft { .. } => {
                        println!("👋 Mouse left window");
                    }

                    // Mouse button pressed
                    WindowEvent::MouseInput { state, button, .. } => match state {
                        ElementState::Pressed => match button {
                            MouseButton::Left => println!("🖱️  Left button pressed"),
                            MouseButton::Right => println!("🖱️  Right button pressed"),
                            MouseButton::Middle => println!("🖱️  Middle button pressed"),
                            MouseButton::Other(id) => println!("🖱️  Other button {} pressed", id),
                            _ => println!("🖱️  Unknown button pressed"),
                        },
                        ElementState::Released => match button {
                            MouseButton::Left => println!("🖱️  Left button released"),
                            MouseButton::Right => println!("🖱️  Right button released"),
                            MouseButton::Middle => println!("🖱️  Middle button released"),
                            MouseButton::Other(id) => println!("🖱️  Other button {} released", id),
                            _ => println!("🖱️  Unknown button released"),
                        },
                        _ => {}
                    },

                    // Mouse wheel scrolled
                    WindowEvent::MouseWheel { delta, .. } => match delta {
                        tao::event::MouseScrollDelta::LineDelta(x, y) => {
                            println!("🔄 Mouse wheel: horizontal={:.1}, vertical={:.1}", x, y);
                        }
                        tao::event::MouseScrollDelta::PixelDelta(position) => {
                            println!(
                                "🔄 Mouse wheel (pixels): x={:.1}, y={:.1}",
                                position.x, position.y
                            );
                        }
                        _ => {}
                    },

                    // Keyboard input
                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed {
                            use tao::keyboard::KeyCode;
                            if event.physical_key == KeyCode::Escape {
                                println!("\n👋 Exiting...");
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                    }

                    // Window close button clicked
                    WindowEvent::CloseRequested => {
                        println!("\n👋 Goodbye!");
                        *control_flow = ControlFlow::Exit;
                    }

                    // Ignore other window events
                    _ => {}
                }
            }

            // Device events (including mouse events outside window)
            Event::DeviceEvent { event, .. } => {
                match event {
                    DeviceEvent::MouseMotion { delta, .. } => {
                        // Uncomment to see raw mouse motion (even outside window)
                        // println!("📐 Raw mouse motion: dx={:.1}, dy={:.1}", delta.0, delta.1);
                    }
                    DeviceEvent::MouseWheel { delta, .. } => {
                        // Uncomment to see device-level scroll events
                        // println!("🔄 Device mouse wheel: {:?}", delta);
                    }
                    DeviceEvent::Button { button, state, .. } => {
                        // Uncomment to see device-level button events
                        // println!("🔘 Device button {}: {:?}", button, state);
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    });
}
