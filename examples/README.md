# cino examples

## Standalone `.cino` examples

| File | Description | Key points |
|---|---|---|
| `counter.cino` | Counter (increment, decrement, reset) | Minimal State / Event / update / query setup |
| `traffic_light.cino` | Traffic light (Red → Green → Yellow → Red …) | State machine using an enum as state |
| `todo_list.cino` | Incomplete task count management | Demo of generating an `Action` on task completion |
| `shopping_cart.cino` | Shopping cart (total management, checkout) | Covers user-defined `fn`, multiple Actions, and multiple Events |

## Rust Integration Example

### `counter_app/`

A counter app that loads `counter.cino` via `cino-runtime` and displays a GUI using libui.

```
examples/counter_app/
  Cargo.toml    # dependencies for libui + cino crates
  counter.cino  # cino source (embedded in the binary)
  src/
    main.rs     # Rust application entry point
```

#### Requirements

| OS | Dependency |
|---|---|
| macOS | Xcode Command Line Tools (Cocoa is used automatically) |
| Linux | GTK3 (e.g. `sudo apt install libgtk-3-dev`) |
| Windows | Win32 API (automatic) |

#### Run

```sh
cd examples/counter_app
cargo run
```

## Verification with the cino CLI

```sh
# Build
cargo build -p cino-cli --bin cino

# Check
./target/debug/cino check --file examples/counter.cino

# Run update
./target/debug/cino run update \
  --file examples/counter.cino \
  --state 0 \
  --event '{"$tag":"Increment","$fields":{}}'

# Run query
./target/debug/cino run query \
  --file examples/counter.cino \
  --state 5 \
  --query '{"$tag":"GetCount","$fields":{}}'
```
