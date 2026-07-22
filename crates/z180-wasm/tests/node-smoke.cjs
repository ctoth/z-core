"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { Machine, Reg } = require("../pkg/z180_wasm.js");

const CYCLE_BUDGET = 1_000_000;
const TARGET_CYCLES_PER_SECOND = 25_000_000;
const ROM_SIZE = 0x1000;

const program = fs.readFileSync(path.join(__dirname, "fibonacci.bin"));
assert.ok(program.length <= ROM_SIZE, "Fibonacci program must fit in one ROM page");

const rom = new Uint8Array(ROM_SIZE);
rom.set(program);

const machine = new Machine({
    regions: [
        { base: 0x00000, size: ROM_SIZE, kind: "rom", data: rom },
        { base: 0x01000, size: 0x0f000, kind: "ram" },
    ],
});

try {
    const started = process.hrtime.bigint();
    const consumed = machine.run(CYCLE_BUDGET);
    const elapsedNanoseconds = process.hrtime.bigint() - started;
    const cyclesPerSecond = consumed / (Number(elapsedNanoseconds) / 1_000_000_000);

    assert.ok(consumed >= CYCLE_BUDGET, "run must consume the requested cycle budget");
    assert.equal(machine.cycleCount(), BigInt(consumed));
    assert.equal(machine.reg(Reg.BC), 0x3759, "BC must contain Fibonacci(10), Fibonacci(11)");
    assert.equal(machine.reg(Reg.AF) >>> 8, 0x59, "A must contain Fibonacci(11)");
    assert.equal(machine.reg(Reg.DE), 0x0000, "D loop counter must reach zero");

    console.log(`Fibonacci registers: BC=${machine.reg(Reg.BC).toString(16).padStart(4, "0")} A=${(machine.reg(Reg.AF) >>> 8).toString(16).padStart(2, "0")} DE=${machine.reg(Reg.DE).toString(16).padStart(4, "0")}`);
    console.log(`Cycles consumed: ${consumed}`);
    console.log(`Elapsed seconds: ${(Number(elapsedNanoseconds) / 1_000_000_000).toFixed(6)}`);
    console.log(`Cycles/second: ${Math.round(cyclesPerSecond).toLocaleString("en-US")}`);
    console.log(`Target: ${TARGET_CYCLES_PER_SECOND.toLocaleString("en-US")} cycles/second`);

    assert.ok(
        cyclesPerSecond >= TARGET_CYCLES_PER_SECOND,
        `WASM throughput ${Math.round(cyclesPerSecond)} is below ${TARGET_CYCLES_PER_SECOND}`,
    );
    console.log("P9.3 Node smoke: PASS");
} finally {
    machine.free();
}
