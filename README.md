# Neonet2

Draws moving lines on the screen. This is an example for building a WGPU
application that can be run on both the desktop and on the web.

## Running

### Desktop

You can run this application like any cargo-based rust application:

```bash
cargo run --release
```

### Web

In order to run this application on the web. First you need to build the web
assets:

```bash
wasm-pack build --target web
```

And then you can use a static web server to serve the built files:

```bash
python -m http.server
```

And open the hosted web-page in your browser (for example
at [http://127.0.0.1:8000/](http://127.0.0.1:8000/)).
