const m = document.querySelector("#m");
const k = document.querySelector("#k");
const s = document.querySelector("#s");
const kd = document.querySelectorAll(".key");
let con = 0;


// Keep the original camera / keyboard interaction untouched.
const base = (e) => {
    const x = e.pageX / window.innerWidth - 0.5;
    const y = e.pageY / window.innerHeight - 0.5;
    k.style.transform = `
        perspective(10000px)
        rotateX(${y * 10 + 60}deg)
        rotateZ(-${x * 40 + 35}deg)
    `;
};

// Physical keyboard layout -> existing virtual-key indexes.
// Nothing in the keyboard CSS/geometry is changed.
const keyMap = new Map([
    // Number row / symbols
    ["Backquote", 0],
    ["Digit1", 1], ["Digit2", 2], ["Digit3", 3], ["Digit4", 4],
    ["Digit5", 5], ["Digit6", 6], ["Digit7", 7], ["Digit8", 8],
    ["Digit9", 9], ["Digit0", 10], ["Minus", 11], ["Equal", 12],
    ["Backspace", 13],

    // QWERTY row
    ["Tab", 14],
    ["KeyQ", 15], ["KeyW", 16], ["KeyE", 17], ["KeyR", 18],
    ["KeyT", 19], ["KeyY", 20], ["KeyU", 21], ["KeyI", 22],
    ["KeyO", 23], ["KeyP", 24], ["BracketLeft", 25], ["BracketRight", 26],
    ["Backslash", 27],

    // Home row
    ["CapsLock", 28],
    ["KeyA", 29], ["KeyS", 30], ["KeyD", 31], ["KeyF", 32],
    ["KeyG", 33], ["KeyH", 34], ["KeyJ", 35], ["KeyK", 36],
    ["KeyL", 37], ["Semicolon", 38], ["Quote", 38],
    ["Enter", 39],

    // Bottom letter row
    ["ShiftLeft", 40],
    ["KeyZ", 41], ["KeyX", 42], ["KeyC", 43], ["KeyV", 44],
    ["KeyB", 45], ["KeyN", 46], ["KeyM", 47], ["Comma", 48],
    ["Period", 49], ["Slash", 50],
    ["ShiftRight", 51],

    // Modifier / space row
    ["ControlLeft", 52], ["ControlRight", 61],
    ["MetaLeft", 53], ["MetaRight", 58],
    ["AltLeft", 54], ["AltRight", 57],
    ["Space", 56],
    ["ContextMenu", 59],

    // A few browsers / layouts report the numpad keys separately.
    // They animate the corresponding visible number keys.
    ["Numpad1", 1], ["Numpad2", 2], ["Numpad3", 3], ["Numpad4", 4],
    ["Numpad5", 5], ["Numpad6", 6], ["Numpad7", 7], ["Numpad8", 8],
    ["Numpad9", 9], ["Numpad0", 10]
]);

// Additional compatibility mapping for keyboard events where `code` is unavailable.
const keyCodeMap = {
    192: 0,
    49: 1, 50: 2, 51: 3, 52: 4, 53: 5, 54: 6, 55: 7, 56: 8, 57: 9, 48: 10,
    189: 11, 187: 12, 8: 13,
    9: 14,
    81: 15, 87: 16, 69: 17, 82: 18, 84: 19, 89: 20, 85: 21, 73: 22, 79: 23, 80: 24,
    219: 25, 221: 26, 220: 27,
    20: 28,
    65: 29, 83: 30, 68: 31, 70: 32, 71: 33, 72: 34, 74: 35, 75: 36, 76: 37,
    186: 38, 222: 38, 13: 39,
    16: 40, 90: 41, 88: 42, 67: 43, 86: 44, 66: 45, 78: 46, 77: 47,
    188: 48, 190: 49, 191: 50,
    17: 52, 91: 53, 18: 54, 32: 56, 93: 59
};

const getVirtualIndex = (e) => {
    if (keyMap.has(e.code)) return keyMap.get(e.code);
    return keyCodeMap[e.keyCode];
};

const pressVirtualKey = (index) => {
    if (index == null || !kd[index]) return;
    kd[index].classList.add("key--down");
};

const releaseVirtualKey = (index) => {
    if (index == null || !kd[index]) return;
    kd[index].classList.remove("key--down");
};

const addKey = (e) => {
    const index = getVirtualIndex(e);
    pressVirtualKey(index);

    // Preserve the original text capture behavior, extended to the number row.
    // Letters are kept in the original A-Z behavior; numbers and -/= use e.key
    // so shifted variants still reflect the actual physical key being pressed.
    const kc = e.keyCode;
    const printableNumberRow =
        (kc >= 48 && kc <= 57) || kc === 189 || kc === 187;

    if ((kc >= 65 && kc <= 90) || kc === 32 || printableNumberRow) {
        if (kc === 32) {
            s.innerHTML += "&nbsp;";
        } else if (printableNumberRow) {
            s.innerHTML += e.key;
        } else {
            s.innerHTML += String.fromCharCode(kc);
        }

        con++;
        if (con > 10) {
            s.innerHTML = "";
            con = 0;
        }
    }

    // Backspace clears the screen, exactly like the original.
    if (kc === 8) {
        s.innerHTML = "";
        con = 0;
    }
};

const removeKey = (e) => {
    releaseVirtualKey(getVirtualIndex(e));
};

m.addEventListener("mousemove", base);
window.addEventListener("keydown", addKey);
window.addEventListener("keyup", removeKey);

// Also release all virtual keys when the page loses focus so a held modifier
// cannot remain visually pressed after Alt+Tab or another focus change.
window.addEventListener("blur", () => {
    kd.forEach((key) => key.classList.remove("key--down"));
});

