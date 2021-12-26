var webSocket;

// Variable used to indicate if a button is pressed or not.
// Used to only act on `onmouseout` events if a button is pressed. 
var buttonPressed = false;

function init() {
    if ("WebSocket" in window) {
        if (typeof webSocket !== "undefined") {
            alert("Already connected.");
            return
        }

        const addr = "ws://localhost:8080";
        const localWebSocket = new WebSocket(addr);

        localWebSocket.onopen = function(_) {
            alert("Websocket connection established.");
            webSocket = localWebSocket;
        };

        localWebSocket.onclose = function(_) {
            alert("Websocket connection closed.");
            webSocket = undefined;
        };
    } else {
        alert("Websockets not supported in this browser.");
    }
}

function exit() {
    if (typeof webSocket !== "undefined") {
        webSocket.close();
        webSocket = undefined;
    }
}

function send(bytes) {
    if (typeof webSocket !== "undefined") {
        webSocket.send(new Uint8Array(bytes));
    }
}

function upPressed() { send([0, 0]) }
function upReleased() { send([0, 1]) }
function rightPressed() { send([0, 2]) }
function rightReleased() { send([0, 3]) }
function downPressed() { send([0, 4]) }
function downReleased() { send([0, 5]) }
function leftPressed() { send([0, 6]) }
function leftReleased() { send([0, 7]) }
function aPressed() { send([0, 8]) }
function aReleased() { send([0, 9]) }
function bPressed() { send([0, 10]) }
function bReleased() { send([0, 11]) }

function mouseDown(f) {
    buttonPressed = true;
    f();
}

function mouseUp(f) {
    buttonPressed = false;
    f();
}

function mouseOut(f) {
    if (buttonPressed) {
        buttonPressed = false;
        f();
    }
}

window.addEventListener("keydown", function (event) {
    if (event.defaultPrevented || event.repeat) {
        return;
    }

    switch (event.key) {
        case "Up":
        case "ArrowUp":
            upPressed();
            break;
        case "Right":
        case "ArrowRight":
            rightPressed();
            break;
        case "Down":
        case "ArrowDown":
            downPressed();
            break;
        case "Left":
        case "ArrowLeft":
            leftPressed();
            break;
        case "a":
            aPressed();
            break;
        case "b":
            bPressed();
            break;
        default:
            return;
    }

    event.preventDefault();
}, true);

window.addEventListener("keyup", function (event) {
    if (event.defaultPrevented || event.repeat) {
        return;
    }

    switch (event.key) {
        case "Up":
        case "ArrowUp":
            upReleased();
            break;
        case "Right":
        case "ArrowRight":
            rightReleased();
            break;
        case "Down":
        case "ArrowDown":
            downReleased();
            break;
        case "Left":
        case "ArrowLeft":
            leftReleased();
            break;
        case "a":
            aReleased();
            break;
        case "b":
            bReleased();
            break;
        default:
            return;
    }

    event.preventDefault();
}, true);
