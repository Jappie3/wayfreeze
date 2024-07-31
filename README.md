# Wayfreeze

A small CLI tool to freeze the screen of a wlroots compositor, this can be useful to, for example, take a screenshot. Supports multiple monitors & fractional scaling.

## Usage

Run `wayfreeze`, click or press escape to exit.

```bash
Usage: wayfreeze [OPTIONS]

Options:
      --hide-cursor  Hide cursor when freezing the screen
  -h, --help         Print help
  -V, --version      Print version
```

Example usage with [Grim](https://git.sr.ht/~emersion/grim) & [Slurp](https://github.com/emersion/slurp):

```bash
wayfreeze & PID=$!; sleep .1; grim -g "$(slurp)" - | wl-copy; kill $PID
# or
$(grim -g "$(slurp)" - | wl-copy; killall wayfreeze) & sleep .1; wayfreeze
```

> Note: the Wayland specification [states the following](https://wayland.app/protocols/wlr-layer-shell-unstable-v1#zwlr_layer_shell_v1:enum:layer): "Multiple surfaces can share a single layer, and ordering within a single layer is undefined." This means that compositors can put new layer surfaces **over or under** existing layer surfaces (given they're on the same layer), and **both of those options are compliant to the spec**. That's why there are 2 example commands provided: the first one works on compositors like e.g. Hyprland (where new layer surfaces appear over older ones), while the second one works on e.g. Sway (which puts new layer surfaces underneath already existing ones).

## Installing

Wayfreeze can be installed either by using nixpkgs-unstable or flake.

### Nixpkgs:
Add this to your configuration and rebuild your system:
```nix
environment.systemPackages = [ pkgs.wayfreeze ];
```

### Flake:
Add this repository as a flake to your inputs:
```nix
wayfreeze.url = "github:jappie3/wayfreeze";
```

Define the package and then rebuild your system:
```nix
environment.systemPackages = [ inputs.wayfreeze.packages.${pkgs.system}.wayfreeze ];
```

### From source:

First, make sure you have Rust & Cargo installed. Then clone this repo & run `cargo build --release`, the compiled binary should be under `./target/release/wayfreeze`.

## Technical

The following protocols should be supported by your compositor:

- `wlr-layer-shell-unstable-v1` -> used for creating & rendering a layer surface
- `wlr-screencopy-unstable-v1` -> used for copying the current output to a client buffer
- `wp-fractional-scale-v1` -> to support fractional scaling
- `wp-viewporter` -> for scaling the surface

## Credits

In no particular order, here are some resources that were helpful when creating this tool & learning about the Wayland protocol:

- https://wayland.app/protocols/wayland
- https://github.com/hiasen/wayland-rust-client-experiment/
- https://github.com/Smithay/wayland-window/blob/master/examples/simple_window.rs
- https://github.com/rafaelrc7/wayland-pipewire-idle-inhibit/
- https://levans.fr/rust_wayland_1.html
- https://bugaevc.gitbooks.io/writing-wayland-clients/content/black-square/allocating-a-buffer.html
- https://danyspin97.org/talks/writing-a-wayland-wallpaper-daemon-in-rust
- https://docs.rs/wayland-client/latest/wayland_client
- https://wayland-book.com
