use lumin_wallpaper_rs::{EventForwarder, WindowsPlatform};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::windows::WindowExtWindows,
    window::WindowBuilder,
};

use wry::WebViewBuilder;

fn main() {
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Wry Input Event Tester")
        .with_inner_size(tao::dpi::LogicalSize::new(1000.0, 700.0))
        .build(&event_loop)
        .expect("failed to create window");

    let html = r###"
<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">

<style>
    * {
        box-sizing: border-box;
    }

    html, body {
        margin: 0;
        width: 100%;
        height: 100%;
        overflow: hidden;

        background: #111;
        color: #eee;

        font-family:
            Consolas,
            "Cascadia Code",
            monospace;
    }

    body {
        display: flex;
        flex-direction: column;
    }

    #header {
        padding: 14px 18px;
        background: #181818;
        border-bottom: 1px solid #333;
    }

    #title {
        font-size: 20px;
        font-weight: bold;
    }

    #status {
        margin-top: 6px;
        color: #888;
        font-size: 13px;
    }

    #events {
        flex: 1;
        overflow-y: auto;
        padding: 10px;
    }

    .event {
        padding: 7px 10px;
        margin-bottom: 5px;

        background: #181818;
        border: 1px solid #292929;
        border-radius: 5px;

        white-space: pre-wrap;
        word-break: break-word;
    }

    .mouse {
        border-left: 3px solid #4fc3f7;
    }

    .keyboard {
        border-left: 3px solid #ce93d8;
    }

    .wheel {
        border-left: 3px solid #ffca28;
    }

    .focus {
        border-left: 3px solid #66bb6a;
    }

    #footer {
        display: flex;
        gap: 8px;
        padding: 10px;
        background: #181818;
        border-top: 1px solid #333;
    }

    button {
        padding: 7px 12px;
        background: #252525;
        color: white;
        border: 1px solid #444;
        border-radius: 4px;
        cursor: pointer;
    }

    button:hover {
        background: #333;
    }
</style>
</head>

<body>

<div id="header">
    <div id="title">WebView2 Input Event Tester</div>
    <div id="status">
        Click this window and move the mouse / scroll / press keys.
    </div>
</div>

<div id="events"></div>

<div id="footer">
    <button onclick="clearEvents()">Clear</button>
    <button onclick="testFocus()">Focus WebView</button>
</div>

<script>
const eventsElement = document.getElementById("events");

let eventCount = 0;

function logEvent(type, data, category = "mouse") {
    eventCount++;

    const element = document.createElement("div");

    element.className = "event " + category;

    const time = new Date().toLocaleTimeString();

    element.textContent =
        "#" + eventCount +
        " [" + time + "] " +
        type +
        "\n" +
        JSON.stringify(data, null, 2);

    eventsElement.prepend(element);

    while (eventsElement.children.length > 500) {
        eventsElement.removeChild(eventsElement.lastChild);
    }
}

function mouseData(event) {
    return {
        type: event.type,
        button: event.button,
        buttons: event.buttons,

        clientX: event.clientX,
        clientY: event.clientY,

        screenX: event.screenX,
        screenY: event.screenY,

        ctrl: event.ctrlKey,
        shift: event.shiftKey,
        alt: event.altKey,
        meta: event.metaKey,
    };
}

function keyboardData(event) {
    return {
        type: event.type,

        key: event.key,
        code: event.code,

        keyCode: event.keyCode,
        which: event.which,

        repeat: event.repeat,

        ctrl: event.ctrlKey,
        shift: event.shiftKey,
        alt: event.altKey,
        meta: event.metaKey,
    };
}

// ------------------------------------------------------------
// Mouse
// ------------------------------------------------------------

window.addEventListener("mousedown", event => {
    logEvent(
        "mousedown",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("mouseup", event => {
    logEvent(
        "mouseup",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("mousemove", event => {
    logEvent(
        "mousemove",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("mouseenter", event => {
    logEvent(
        "mouseenter",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("mouseleave", event => {
    logEvent(
        "mouseleave",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("click", event => {
    logEvent(
        "click",
        mouseData(event),
        "mouse"
    );
});

window.addEventListener("dblclick", event => {
    logEvent(
        "dblclick",
        mouseData(event),
        "mouse"
    );
});

// ------------------------------------------------------------
// Wheel
// ------------------------------------------------------------

window.addEventListener("wheel", event => {
    logEvent(
        "wheel",
        {
            deltaX: event.deltaX,
            deltaY: event.deltaY,
            deltaZ: event.deltaZ,
            deltaMode: event.deltaMode,

            clientX: event.clientX,
            clientY: event.clientY,

            ctrl: event.ctrlKey,
            shift: event.shiftKey,
            alt: event.altKey,
        },
        "wheel"
    );
}, { passive: true });

// ------------------------------------------------------------
// Keyboard
// ------------------------------------------------------------

window.addEventListener("keydown", event => {
    logEvent(
        "keydown",
        keyboardData(event),
        "keyboard"
    );
});

window.addEventListener("keyup", event => {
    logEvent(
        "keyup",
        keyboardData(event),
        "keyboard"
    );
});

window.addEventListener("keypress", event => {
    logEvent(
        "keypress",
        keyboardData(event),
        "keyboard"
    );
});

// ------------------------------------------------------------
// Focus
// ------------------------------------------------------------

window.addEventListener("focus", () => {
    logEvent(
        "focus",
        {},
        "focus"
    );
});

window.addEventListener("blur", () => {
    logEvent(
        "blur",
        {},
        "focus"
    );
});

document.addEventListener("visibilitychange", () => {
    logEvent(
        "visibilitychange",
        {
            visibilityState: document.visibilityState
        },
        "focus"
    );
});

// ------------------------------------------------------------
// Helpers
// ------------------------------------------------------------

function clearEvents() {
    eventsElement.innerHTML = "";
    eventCount = 0;
}

function testFocus() {
    window.focus();

    logEvent(
        "manual focus()",
        {},
        "focus"
    );
}

// Show initial state.
logEvent(
    "WebView loaded",
    {
        userAgent: navigator.userAgent,
        hasFocus: document.hasFocus(),
        visibility: document.visibilityState
    },
    "focus"
);
</script>

</body>
</html>
"###;

    let _webview = WebViewBuilder::new()
        .with_html(html)
        .with_devtools(true)
        .build(&window)
        .expect("failed to create WebView");

    let hwnd = window.hwnd();
    let a = EventForwarder::new(hwnd, true, true, true).unwrap();
    let b = a.forward_events().unwrap();

    std::thread::spawn(move || {
        loop {
            b.pause();
            println!("{}", b.is_resume());
            std::io::stdin().read_line(&mut String::new());
            b.resume();
            println!("{}", b.is_resume());
            std::io::stdin().read_line(&mut String::new());
        }
    });

    WindowsPlatform::auto_attach(hwnd, false);
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }

            _ => {}
        }
    });
}
