# z180 WebAssembly binding

This crate exposes the shared Z180 machine to Node.js and browsers with
`wasm-bindgen`. The generated declarations include strict machine config,
trace, and discriminated `Event` types.

The package currently uses the plan placeholder name `@zcore/z180`; the final
published package name still requires Q's decision.

## Node.js

From `crates/z180-wasm`, build the CommonJS target and run the committed
Fibonacci ROM smoke:

```powershell
wasm-pack build --target nodejs --scope zcore
node tests/node-smoke.cjs
```

The smoke constructs a core-owned ROM and RAM map, runs one million cycles,
checks the Fibonacci result in registers, and requires at least 25 million
emulated cycles per second.

A minimal Node consumer has the same shape:

```javascript
const { Machine, Reg } = require("./pkg/z180_wasm.js");

const rom = new Uint8Array(0x1000);
rom[0] = 0x00; // NOP

const machine = new Machine({
  regions: [
    { base: 0, size: rom.length, kind: "rom", data: rom },
    { base: 0x1000, size: 0xf000, kind: "ram" },
  ],
});

try {
  console.log({ cycles: machine.step(), pc: machine.reg(Reg.PC) });
} finally {
  machine.free();
}
```

## Browser demo

The browser build and demo are static files; no framework or bundler is
required. From `crates/z180-wasm`:

```powershell
wasm-pack build --target web --scope zcore
uv run python -m http.server 8000
```

Open `http://127.0.0.1:8000/demo/`, choose a ROM, set a cycle budget, and run
it. The page prints the public registers and drains both ASCI transmit queues
as text.

`wasm-pack` writes each target to `pkg/`, so rebuilding the Node.js target
replaces the browser target and vice versa.

## TypeScript contract

After either package build, check a strict consumer with:

```powershell
npx --yes --package typescript@5.9.3 tsc --noEmit --project types/tsconfig.json
```

`types/refinements.d.ts` is embedded as a wasm-bindgen custom section. In
particular, `drainEvents()` returns a seven-variant union whose `kind` field
narrows each event to only its valid properties.

ROM data must fill its entire 4 KiB-aligned region. RAM and ROM execute inside
the core; JavaScript callbacks are used only for `external` regions and I/O.
