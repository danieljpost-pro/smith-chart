# Smith Chart

An interactive, zoomable [Smith chart](https://en.wikipedia.org/wiki/Smith_chart)
that runs entirely in the browser. All math and rendering are implemented in
Rust and compiled to WebAssembly; the JS layer only handles DOM events and UI
chrome. There is no server side — loading data, computing, and sharing all
happen locally in the browser.

## Features

- **Infinite-feeling zoom** — the impedance/admittance grid refines adaptively
  as you zoom (like map tiles), with values chosen on a 1‑2‑5 ladder. Circles
  are sampled as curvature-adaptive polylines, so the chart stays sharp and
  exact at zoom levels where naive `canvas.arc` rendering falls apart.
- **Impedance and admittance grids**, individually toggleable, with
  collision-managed labels.
- **Live cursor readout** — Γ (rectangular and polar), Z, normalized z, Y,
  VSWR, return loss, mismatch loss, and wavelengths toward generator/load.
  Hovering also highlights the exact constant-R and constant-X circles through
  the cursor.
- **Markers** — click the chart (clicks snap to trace points and capture the
  frequency), or enter an impedance directly. Optional VSWR circles through
  each marker.
- **Touchstone traces** — load `.s1p` / `.s2p` files (MA, DB, RI formats, any
  frequency unit, wrapped rows, noise-parameter sections handled). Trace data
  referenced to a different Z₀ than the chart is renormalized. For 2-ports,
  pick which S-parameter to plot.
- **Constant-Q contours** and configurable system impedance Z₀.
- **In-browser sharing** — the entire chart state (view, options, markers,
  traces) is deflate-compressed and encoded into the URL fragment. Copy the
  link, send it to anyone; nothing is uploaded anywhere.
- **PNG export**, light/dark themes, touch support with pinch zoom.

## Building

Prerequisites:

```sh
rustup target add wasm32-unknown-unknown
# wasm-bindgen CLI, same version as the crate dependency (see Cargo.toml)
cargo install wasm-bindgen-cli --version 0.2.126
```

Then:

```sh
./build.sh    # cargo build + wasm-bindgen -> www/pkg/
./serve.sh    # static server on http://localhost:8080/
```

Any static file server works; the whole app is the `www/` directory. Wasm
modules can't load from `file://` URLs, hence the server.

## Tests

The core (transforms, grid geometry, Touchstone parser, state round-trips) is
plain Rust and runs natively:

```sh
cargo test
```

## Controls

| Action | Effect |
|---|---|
| drag | pan |
| scroll / pinch / double-click | zoom (anchored at the cursor) |
| click | add marker (snaps to trace points) |
| `+` / `-` / `0` | zoom in / out / reset view |
| drop `.s1p`/`.s2p` file on the chart | load trace |

## Architecture

```
src/
  complex.rs     minimal complex arithmetic
  transforms.rs  Γ ↔ z, VSWR, RL, wavelengths, renormalization, readout
  grid.rs        adaptive grid: circle windows, 1-2-5 refinement, pruning
  view.rs        pan/zoom viewport (gamma plane ↔ CSS pixels)
  touchstone.rs  .sNp parser
  chart.rs       chart model: options, markers, traces, hover, state
  state.rs       serializable share-state
  render.rs      Canvas 2D renderer (polyline sampling, labels, traces)
  lib.rs         wasm-bindgen API surface
www/
  index.html / style.css / main.js   UI shell
  examples/series-rlc.s1p            demo data
```

Key geometric idea: every grid circle is reduced to the angular window(s)
lying inside both the unit disk and the viewport's bounding circle
(circle–circle intersection), then sampled with an angular step chosen for
≤ 0.25 px sagitta error and clipped with Cohen–Sutherland outcodes. Grid
density comes from recursive interval refinement that stops when adjacent
circles are closer than ~26 px near the viewport or provably invisible, so
both a full-chart view and a 10⁷× zoom stay cheap and correctly dense.
