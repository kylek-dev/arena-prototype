# Developer setup from a fresh machine

This guide is for a developer cloning and modifying the project. Install the tools below once; Cargo and npm then download the pinned project dependencies. There is no separate Bevy/editor installation, database, asset download, or AI service requirement. The optional match server is built from this workspace (see below).

## Prerequisites

| Tool | Purpose |
| --- | --- |
| Git | Clone and contribute changes |
| Rust via rustup | Cargo, compiler, rustfmt and Clippy; rust-toolchain.toml selects Rust 1.99.1 |
| Platform C/C++ build tools | Linker and native build dependencies used by Rust tooling |
| Python 3 | Local static web server and GitHub export script |
| Node.js 24 with npm | Browser tests, JavaScript checks and npm scripts; matches CI |
| Current desktop browser with WebGL2 | Manual playtesting |

GitHub CLI (`gh`) is optional for gameplay development. It is used to manage/export GitHub Issues and Projects and synchronize the wiki; those tasks require authentication with the relevant repository/project permissions.

## Ubuntu / Windows with WSL

On a fresh Windows machine, open PowerShell as Administrator and run:

```powershell
wsl --install
```

Restart if requested, open Ubuntu, and complete its initial user setup. See [Microsoft's WSL installation guide](https://learn.microsoft.com/en-us/windows/wsl/install). Run the following Linux commands inside Ubuntu, not PowerShell.

For Ubuntu 24.04, install the base build tools and libraries:

```sh
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential pkg-config libssl-dev python3 libx11-dev libxkbcommon-dev
```

Install Rust using the [official rustup installer](https://rust-lang.org/tools/install/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Install **Node.js 24 and npm** using the [official Node.js downloads/instructions](https://nodejs.org/en/download), selecting the Linux installation appropriate for your architecture. Under WSL, install Node inside Ubuntu as well. Open a new terminal if the installer changes PATH, then check:

```sh
git --version
rustup --version
python3 --version
node --version
npm --version
```

Node should report `v24.x`. Continue with the common project setup below.

## macOS

1. Run `xcode-select --install` in Terminal and finish installing Apple's command-line tools, which provide Git and the C/C++ toolchain.
2. Install Python 3 from [python.org](https://www.python.org/downloads/).
3. Install Node.js 24 with npm from [nodejs.org](https://nodejs.org/en/download).
4. Install Rust using the [official rustup installer](https://rust-lang.org/tools/install/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Run the same version checks above, then continue below. The build scripts use Bash; macOS includes it. This guide uses WSL for Windows rather than mixing native Windows and Unix toolchains.

## Clone, install project dependencies, and verify

Run these commands in Bash on Linux/WSL or in a macOS terminal:

```sh
git clone https://github.com/kylek-dev/arena-prototype.git
cd arena-prototype

# The repository automatically selects and downloads its pinned Rust toolchain.
rustup show active-toolchain
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked

# Install pinned frontend tools; no browser installation is required.
npm ci
npm test

# Native and JavaScript unit checks, WASM checks, then the browser build.
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
./scripts/build-web.sh

# Start the game for manual E2E review (see docs/BROWSER.md).
npm run serve
```

The first Rust build compiles dependencies and takes longer than subsequent work; generated output stays outside version control.

The optional PixelLabs reference character has a separate download and license; see [character setup](../assets/characters/README.md). It is not required to build or test a clean clone.

## Run and iterate

```sh
npm run serve
```

Open http://localhost:8000. Keep that terminal running; Ctrl+C stops the server. Open `/?debug=1` for read-only game state and diagnostics.

- Rust rules/client changes: rerun the relevant checks and `./scripts/build-web.sh`, then reload the browser.
- React/TypeScript/CSS changes: `npm run dev` for hot reload using the existing generated engine; no Rust rebuild. `npm run build` rebuilds production UI only.
- Frontend checks: `npm run typecheck`, `npm run lint`, `npm run format:check`, `npm test`; write readable formatting with `npm run format`.
- Input/domain/DOM watch loop: `npm run test:watch`; dependency-free legacy input loop: `npm run test:input:watch`.
- Rules-only test loop: `cargo test -p arena-sim`.
- Scripted original headless example: `cargo run -p arena-headless`.

### Match server (no browser client yet)

The authoritative match server ([ADR 0044](decisions/0044-authoritative-match-server.md), [protocol](PROTOCOL.md)) runs locally without accounts:

```sh
cargo run -p arena-server -- --dev            # ws://127.0.0.1:8080/play
curl -X POST 'http://127.0.0.1:8080/dev/match?team_size=1'   # match id and one hex ticket per seat
curl http://127.0.0.1:8080/metrics            # Prometheus text counters
```

`--dev` generates a random ticket key and enables `POST /dev/match`; it refuses non-loopback `--bind` addresses. Without `--dev`, set `ARENA_TICKET_KEY` to at least 32 bytes of hex shared with whatever issues tickets. `--debug-json` logs every message as JSON on stderr. Ctrl+C or SIGTERM drains: readiness (`/readyz`) fails, new matches are refused, running matches get up to `--drain-seconds` (default 60) before players receive `ServerShutdown`. The browser cannot connect yet. Server tests: `cargo test -p arena-server`.

To play a network match in the browser, start the server and the client, then create a match and open one printed link per seat, each in its own tab or browser:

```sh
cargo run -p arena-server -- --dev        # terminal 1
npm run serve                              # terminal 2 (after ./scripts/build-web.sh)
./scripts/dev-match.sh 1                   # team size 1–3; prints one join link per seat
```

The tab joins its seat, reconnects by itself after a dropped connection, and rejoins after a reload; a banner shows the connection state, and Menu → Leave match returns to practice (Rejoin last match goes back). Add `?debug=1` before the `#` for network statistics in the diagnostics overlay. Join links carry the ticket in the URL fragment; the page removes it from the address bar. The match starts when every seat has joined.

Bot players exercise a running `--dev` server with simulated latency and report latency, consistency and recovery:

```sh
cargo run --release -p arena-bot -- --matches 1 --rtt 100 --jitter 20 --seconds 20
cargo run --release -p arena-bot -- --matches 167 --team-size 3 --seconds 30 --rtt 50 --jitter 10 --server-pid <server pid>
cargo run --release -p arena-bot -- --rtt 100 --reconnect-every 5 --seconds 20   # drop out and rejoin
```

`./scripts/server-smoke.sh` runs the same short (about 5 s) server-and-bots smoke test that CI runs (after `cargo build -p arena-server -p arena-bot`). `--json` prints the report as JSON. The bot exits non-zero on any error, unexpected close, state hash mismatch or failed rejoin. See the [load baseline](research/match-server-load.md).

## VS Code tasks

Open the repository folder in VS Code (on Windows, use **WSL: Open Folder in WSL**). The tasks in [`.vscode/tasks.json`](../.vscode/tasks.json) use the same tools and prerequisites as the commands above; they do not install Rust or the WASM tooling automatically.

- **Terminal → Run Build Task** (`Ctrl+Shift+B`): complete browser build, including Rust/WASM and frontend.
- **Tasks: Run Test Task**: frontend/input tests by default. Rules-only and dependency-free input tasks are also available.
- **Tasks: Run Task → Check: frontend**: types, lint, formatting and tests without compiling Rust.
- **Check: complete project**: native/frontend checks, WASM Clippy and a complete browser build, in sequence. This does not run a browser or establish rendered acceptance.
- **Dev: frontend hot reload** / **Dev: serve production build**: start a persistent server using an existing complete build. Use **Tasks: Terminate Task** to stop servers or the test watcher.
- Other tasks install npm dependencies, run the headless scenario, run individual checks, watch tests, or explicitly format frontend/Rust files.

Run **Setup: npm dependencies** once after cloning or when the lockfile changes, then **Build: complete browser client** before starting either server. Tasks do not run automatically when opening the folder; Playwright remains an explicitly manual diagnostic command.

### Debugging with F5

Select a configuration from **Run and Debug** in [`.vscode/launch.json`](../.vscode/launch.json):

- **Browser: frontend hot reload** starts Vite, waits for readiness, then launches Chrome with the read-only diagnostics panel enabled. Uses VS Code's built-in JavaScript debugger; install Chrome first. TypeScript/JavaScript breakpoints use Vite's development source maps.
- **Browser: production preview** serves the existing `dist/` build and opens Chrome. It does not rebuild; run the complete browser build after Rust changes or the frontend-only build after UI changes. Production source maps are not enabled, so use hot reload for source-level UI debugging.
- **Rust: headless scenario (CodeLLDB)** builds and debugs `arena-headless`. Install the **CodeLLDB** extension (`vadimcn.vscode-lldb`) in the workspace environment (inside WSL when applicable).

Browser launches require an existing complete browser build and do not debug Rust/WASM source. The servers use strict port 8000 to avoid silently debugging a different port; stop any other server on that port before switching configurations. Stopping the browser debugger may leave its server running; use **Tasks: Terminate Task** to stop it. Launching a browser is not a recorded manual acceptance pass.

See [browser development](BROWSER.md) for controls, optional Firefox coverage, pointer-lock automation, and how measurements are collected. Ubuntu native/JavaScript tests and WASM compile checks run in GitHub Actions; E2E validation is manual; fresh macOS and Windows/WSL installations have not been exhaustively rehearsed on pristine machines.
