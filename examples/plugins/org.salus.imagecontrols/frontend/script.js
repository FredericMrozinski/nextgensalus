import { Salus, Payload } from "./salus_sdk.js";

const salus = Salus.connect();

// The Image Viewer opened this plugin; its frontend is our parent.
const viewer = salus.parent;
const status = document.getElementById("status");
const STEP = 40;

const commands = {
    "zoom-in": { op: "zoom", factor: 1.25 },
    "zoom-out": { op: "zoom", factor: 0.8 },
    up: { op: "pan", dx: 0, dy: -STEP },
    down: { op: "pan", dx: 0, dy: STEP },
    left: { op: "pan", dx: -STEP, dy: 0 },
    right: { op: "pan", dx: STEP, dy: 0 },
    reset: { op: "reset" },
};

// --8<-- [start:send]
function run(name) {
    viewer.send("view", Payload.json(commands[name]));
}
// --8<-- [end:send]

if (viewer === null) {
    status.textContent = "Open the Image Viewer to use these controls.";
    document.querySelectorAll("button").forEach((button) => (button.disabled = true));
} else {
    status.textContent = `Controlling the image viewer (frontend ${viewer.id})`;

    document.querySelectorAll("[data-command]").forEach((button) => {
        button.addEventListener("click", () => run(button.dataset.command));
    });

    const keys = { "+": "zoom-in", "=": "zoom-in", "-": "zoom-out", "0": "reset",
        ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };
    window.addEventListener("keydown", (event) => {
        if (keys[event.key]) {
            event.preventDefault();
            run(keys[event.key]);
        }
    });
}
