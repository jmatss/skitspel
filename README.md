# Skitspel
A multiplayer game created to play with my colleagues at our weekly "fredagsfika". The game is ran on a computer that shares its screen. The clients connects through a webbrowser and communicates with the server over websockets.

I currently have a client hosted remotely at: [skitspel.2a.se](https://skitspel.2a.se/). If the link doesn't work when you are reading this, you can simply launch the basic `client` locally in your browser with no problems.


# Build
```
cargo build --release
```
Create an executable located in `.../server/target/release/server.exe` that can be ran to launch the game server.


# Usage
```
USAGE:
    server.exe [OPTIONS] --port <PORT>

OPTIONS:
    -c, --cert <PATH>    Path to certificate in pkcs12 format. Used for TLS.
    -h, --help           Print help information
    -n, --nocert         Specify if no TLS should be used.
    -p, --port <PORT>    The port number to listen on.
```
One of the options `cert` or `nocert` must be specified. If `cert` is specified the server will use TLS when communicating with the clients. `nocert` indicates that no TLs should be used when communicating with the clients.

OBS! All connections from private/local IPv4 or IPv6 addresses will NOT use TLS even when `cert` is specified on the server. So in these cases the clients must make sure to connect without TLS (done by unchecking the `TLS` checkbox when connecting to the server).


# Games

The game consists of multiple mini-games. All games are implemented to support atleast 9 players at the same time.

## Push
<p align="center">
    <img src="https://github.com/jmatss/skitspel/blob/master/media/push.png?raw=true">
</p>
The object of the game is to stay alive as long as possible. A player that touches either the red walls or the red circle in the middle is out. The last survivor gets a point. Holding `A` makes the player spin.


## Hockey
<p align="center">
    <img src="https://github.com/jmatss/skitspel/blob/master/media/hockey.png?raw=true">
</p>
The players are divided evenly into two teams; team left and team right. A point is given to all players of a team that scores a goal. Pressing `A` dashes the player in the direction that it is currently holding down. If it is not holding down any direction keys, the player is dashed in the direction that it is currently traveling.


## Volleyball
<p align="center">
    <img src="https://github.com/jmatss/skitspel/blob/master/media/volleyball.png?raw=true">
</p>
The players are divided evenly into two teams; team left and team right. A point is given to all players of a team that manages to get the ball to touch the other teams floor.


## Achtung dies Kurve
<p align="center">
    <img src="https://github.com/jmatss/skitspel/blob/master/media/achtung.png?raw=true">
</p>
The object of the game is to stay alive as long as possible. A player that touches either the red walls or the "tail" of a snake is out. The last survivor gets a point. Pressing `A` makes the player jump allowing the player to jump over "tails" without losing.


## Pong
<p align="center">
    <img src="https://github.com/jmatss/skitspel/blob/master/media/pong.png?raw=true">
</p>
The object of the game is to stay alive as long as possible. A player is out when the ball touches the outer ring in the color of the player. The last survivor gets a point.
