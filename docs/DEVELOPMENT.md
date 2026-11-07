# Developer setup from a fresh machine

This guide is for a developer cloning and modifying the project. Install the tools below once; Cargo and npm then download the pinned project dependencies. There is no separate Bevy/editor installation, database, game server, asset download, or AI service requirement.

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

# Install the exact JavaScript dependencies and a browser for automated checks.
npm ci
npx playwright install --with-deps chromium

# Native lint/rules checks, browser-specific Rust checks, then the browser build.
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
./scripts/build-web.sh

# Starts its own temporary local server and runs the Chromium interaction checks.
npm run test:browser
```

Playwright's Linux system dependency installation can request sudo. It does not mean a separate game engine needs installation. The first Rust build compiles dependencies and takes longer than subsequent work; generated output stays outside version control.

The optional PixelLabs reference character has a separate download and license; see [character setup](../assets/characters/README.md). It is not required to build or test a clean clone.

## Run and iterate

```sh
npm run serve
```

Open http://localhost:8000. Keep that terminal running; Ctrl+C stops the server. Open `/?debug=1` for read-only game state and diagnostics.

- Rust rules/client changes: rerun the relevant checks and `./scripts/build-web.sh`, then reload the browser.
- HTML/CSS/JavaScript changes in web/: reload the browser; no Rust rebuild is needed.
- Rules-only test loop: `cargo test -p arena-sim`.
- Scripted original headless example: `cargo run -p arena-headless`.

See [browser development](BROWSER.md) for controls, optional Firefox coverage, pointer-lock automation, and how measurements are collected. Ubuntu native/WASM checks and browser checks run in GitHub Actions; fresh macOS and Windows/WSL installations have not been exhaustively rehearsed on pristine machines.
