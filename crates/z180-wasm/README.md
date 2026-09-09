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
checks the Fibonacci result in registers, and reports seven warmed samples of
emulated cycles per second. Performance is informational, separate from correctness.

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

**[Try the hosted workbench](https://ctoth.github.io/z-core/demo/)**, or build
and serve it locally:

The browser build and demo are static files; no framework or bundler is
required. From `crates/z180-wasm`:

```powershell
wasm-pack build --target web --out-dir pkg-web --scope zcore
uv run python -m http.server 8000
```

Open `http://127.0.0.1:8000/demo/`. The bundled counter is ready immediately:
Run stops on its write to RAM at 0x1000; Back returns to the instruction that
made it. The worker keeps execution responsive, with Pause, Step, Reset,
logical PC breakpoints, physical write watches, memory inspection, MMU views,
and serial output. Load ROM maps a zero-based ROM followed by writable RAM.
This bare board has no external devices; firmware requiring board devices
needs a host integration.

Export/import preserves the current state and indexed history in a versioned
browser-session JSON format. This core-only format is distinct from native
`z180-replay` archives. Checkpoints every 256 attempted steps retain a rolling
16 MiB of state payloads; Back is available from the oldest retained position.
The display reports that boundary. Reset after import starts from the imported
current state. Serial display is capped at its latest 8192 characters.

Browser and Node packages use separate `pkg-web/` and `pkg/` directories.
Validate the actual browser workflow with `npm ci`, `npx playwright install
chromium`, and `npm run test:browser` after building both packages.

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

`ramCopy(base)` returns a detached `Uint8Array` snapshot of one RAM region;
mutating that array does not change the machine. Use `loadRam(base, data)` to
replace the complete region explicitly. The supplied array must match the
region size.
