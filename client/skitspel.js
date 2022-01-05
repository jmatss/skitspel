var webSocket;

// Variable used to indicate if a button is pressed or not.
// Used to only act on `onmouseout` events if a button is pressed. 
var buttonPressed = false;

// Set to true when we should listen for key events when ex. pressing arrow keys
// to move the player in games.
var keyEventsActive = false;

function navigateToConnect() {
    document.body.innerHTML = 
          '<div id="header">SKITSPEL</div>'
        + '<div id="login-form">'
        + '    <div class="login-field">'
        + '        <div class="login-label">Name</div>'
        + '        <input class="login-input" type="text" id="name">'
        + '    </div>'
        + '    <div class="login-field">'
        + '        <div class="login-label">Host</div>'
        + '        <input class="login-input" type="text" id="host">'
        + '    </div>'
        + '    <div class="login-field">'
        + '        <div class="login-label">Port</div>'
        + '        <input class="login-input" type="text" id="port">'
        + '    </div>'
        + '    <button class="login-button" onmousedown="connect()">Connect</button>'
        + '</div>';
    keyEventsActive = false;
}

function navigateToButtons() {
    document.body.innerHTML =
          '<button class="game-button game-button-up" onmousedown="mouseDown(upPressed)" onmouseup="mouseUp(upReleased)" onmouseout="mouseOut(upReleased)">Up</button>'
        + '<div class="game-button-sides">'
        + '    <button class="game-button game-button-left" onmousedown="mouseDown(leftPressed)" onmouseup="mouseUp(leftReleased)" onmouseout="mouseOut(leftReleased)">Left</button>'
        + '    <button class="game-button game-button-right" onmousedown="mouseDown(rightPressed)" onmouseup="mouseUp(rightReleased)" onmouseout="mouseOut(rightReleased)">Right</button>'
        + '</div>'
        + '<button class="game-button game-button-down" onmousedown="mouseDown(downPressed)" onmouseup="mouseUp(downReleased)" onmouseout="mouseOut(downReleased)">Down</button>'
        + '<div class="bottom">'
        + '    <button class="game-button game-button-a" onmousedown="mouseDown(aPressed)" onmouseup="mouseUp(aReleased)" onmouseout="mouseOut(aReleased)">A</button>'
        + '    <button class="game-button game-button-b" onmousedown="mouseDown(bPressed)" onmouseup="mouseUp(bReleased)" onmouseout="mouseOut(bReleased)">B</button>'
        + '</div>';
    keyEventsActive = true;
}

function connect() {
    if ("WebSocket" in window) {
        if (typeof webSocket !== "undefined") {
            navigateToButtons();
            alert("Already connected.");
            return
        }

        const name = document.getElementById("name").value;
        const host = document.getElementById("host").value;
        const port = parseInt(document.getElementById("port").value);

        if (isNaN(port)) {
            alert("Unable to parse port as number. Try again.");
            return;
        } else if (!name) {
            alert("Need to specify a non-empty name.");
            return;
        } else if (!host) {
            alert("Need to specify a non-empty host.");
            return;
        }

        const addr = "ws://" + host + ":" + port;
        const localWebSocket = new WebSocket(addr);

        console.log("Connecting to address \"" + addr + "\" with name \"" + name + "\"");

        localWebSocket.onopen = function(_) {
            navigateToButtons();
            const connectMsg = [1].concat(stringToUTF8Array(name));
            localWebSocket.send(new Uint8Array(connectMsg));
            webSocket = localWebSocket;
        };

        localWebSocket.onclose = function(_) {
            navigateToConnect();
            alert("Websocket connection closed.");
            webSocket = undefined;
        };
    } else {
        alert("Websockets not supported in this browser.");
    }
}

function disconnect() {
    if (typeof webSocket !== "undefined") {
        webSocket.close();
        webSocket = undefined;
        navigateToConnect();
    }
}

function send(bytes) {
    if (typeof webSocket !== "undefined") {
        webSocket.send(new Uint8Array(bytes));
    }
}

function upPressed() { send([0, 0]); }
function upReleased() { send([0, 1]); }
function rightPressed() { send([0, 2]); }
function rightReleased() { send([0, 3]); }
function downPressed() { send([0, 4]); }
function downReleased() { send([0, 5]); }
function leftPressed() { send([0, 6]); }
function leftReleased() { send([0, 7]); }
function aPressed() { send([0, 8]); }
function aReleased() { send([0, 9]); }
function bPressed() { send([0, 10]); }
function bReleased() { send([0, 11]); }

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

// Converts the given UTF-16 encoded javascript string to an UTF-8 encoded
// byte array that can be sent to the server.
function stringToUTF8Array(utf16_str) {
    const utf8_str = unescape(encodeURIComponent(utf16_str));
    const utf8_arr = [];
    for (var i = 0; i < utf8_str.length; i++) {
        utf8_arr.push(utf8_str.charCodeAt(i));
    }
    return utf8_arr;
}

window.addEventListener("keydown", function (event) {
    if (!keyEventsActive || event.defaultPrevented || event.repeat) {
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
    if (!keyEventsActive || event.defaultPrevented || event.repeat) {
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
