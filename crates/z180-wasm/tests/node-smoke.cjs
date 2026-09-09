"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const { Machine, Reg } = require("../pkg/z180_wasm.js");

const CYCLE_BUDGET = 1_000_000;
const SAMPLE_COUNT = 7;
const ROM_SIZE = 0x1000;

const program = fs.readFileSync(path.join(__dirname, "fibonacci.bin"));
assert.ok(program.length <= ROM_SIZE, "Fibonacci program must fit in one ROM page");

const rom = new Uint8Array(ROM_SIZE);
rom.set(program);

const splitRamMachine = new Machine({
    regions: [
        { base: 0x00000, size: 0x01000, kind: "ram" },
        { base: 0x04000, size: 0x01000, kind: "ram" },
    ],
});
try {
    const ramCopy = splitRamMachine.ramCopy(0x04000);
    assert.equal(
        ramCopy.length,
        0x01000,
        "a separate nonzero-base RAM region must be exposed",
    );
    ramCopy[0] = 0x5a;
    assert.equal(
        splitRamMachine.memPeek(0x04000),
        0,
        "mutating a RAM copy must not mutate the machine",
    );
    const replacement = new Uint8Array(0x01000);
    replacement[0] = 0xa5;
    splitRamMachine.loadRam(0x04000, replacement);
    assert.equal(
        splitRamMachine.ramCopy(0x04000)[0],
        0xa5,
        "loadRam must be the explicit whole-region write path",
    );
} finally {
    splitRamMachine.free();
}

let callbackReads = 0;
const callbackMachine = new Machine(
    { regions: [{ base: 0x00000, size: 0x01000, kind: "external" }] },
    {
        memRead(address) {
            callbackReads += 1;
            if (callbackReads === 1) {
                throw new Error(`read failed at 0x${address.toString(16)}`);
            }
            return 0x00;
        },
    },
);
try {
    assert.throws(() => callbackMachine.run(1_000), /read failed at 0x0/);
    assert.equal(callbackReads, 1);
    assert.equal(callbackMachine.cycleCount(), 0n);
    assert.equal(callbackMachine.reg(Reg.PC), 0x0000);

    assert.equal(callbackMachine.step(), 6);
    assert.equal(callbackReads, 2);
    assert.equal(callbackMachine.cycleCount(), 6n);
} finally {
    callbackMachine.free();
}

let debuggerWrites = 0;
const debuggerMachine = new Machine(
    {
        unmappedRead: 0xa5,
        regions: [{ base: 0x00000, size: 0x01000, kind: "external" }],
    },
    {
        memRead() {
            return 0x00;
        },
        memWrite() {
            debuggerWrites += 1;
            throw new Error("debugger poke reached memWrite");
        },
    },
);
try {
    debuggerMachine.memPoke(0, 0x5a);
    assert.equal(debuggerWrites, 0, "debugger poke must bypass memWrite");
    assert.equal(debuggerMachine.memPeek(0), 0xa5);
    assert.equal(debuggerMachine.step(), 6, "debugger poke must not defer a callback error");
} finally {
    debuggerMachine.free();
}

for (const value of [
    true,
    3.5,
    Number.POSITIVE_INFINITY,
    "5",
    null,
    Number.MAX_SAFE_INTEGER + 1,
    Number.MIN_SAFE_INTEGER - 1,
]) {
    const invalidCallbackMachine = new Machine(
        { regions: [{ base: 0x00000, size: 0x01000, kind: "external" }] },
        { memRead() { return value; } },
    );
    try {
        assert.throws(
            () => invalidCallbackMachine.step(),
            /memRead callback must return an integer/,
        );
    } finally {
        invalidCallbackMachine.free();
    }
}

for (const [value, expectedPc] of [
    [-1, 0x0038],
    [Number.MAX_SAFE_INTEGER, 0x0038],
    [0x100, 0x0001],
]) {
    const integerCallbackMachine = new Machine(
        { regions: [{ base: 0x00000, size: 0x01000, kind: "external" }] },
        { memRead() { return value; } },
    );
    try {
        assert.ok(integerCallbackMachine.step() > 0);
        assert.equal(integerCallbackMachine.reg(Reg.PC), expectedPc);
    } finally {
        integerCallbackMachine.free();
    }
}

const abortProgram = Uint8Array.from([0x3E, 0x5A, 0x32, 0x00, 0x08]);
const callbackWrites = [];
const abortMachine = new Machine(
    { regions: [{ base: 0x00000, size: 0x01000, kind: "external" }] },
    {
        memRead(address) {
            if (address === 3) {
                throw new Error("operand read failed");
            }
            return abortProgram[address];
        },
        memWrite(address, value) {
            callbackWrites.push([address, value]);
        },
    },
);
try {
    assert.ok(abortMachine.step() > 0);
    const cycleCount = abortMachine.cycleCount();

    assert.throws(() => abortMachine.step(), /operand read failed/);
    assert.deepEqual(callbackWrites, []);
    assert.equal(abortMachine.reg(Reg.PC), 0x0002);
    assert.equal(abortMachine.cycleCount(), cycleCount);
} finally {
    abortMachine.free();
}

const machine = new Machine({
    regions: [
        { base: 0x00000, size: ROM_SIZE, kind: "rom", data: rom },
        { base: 0x01000, size: 0x0f000, kind: "ram" },
    ],
});

try {
    const initial = machine.saveState();
    machine.run(CYCLE_BUDGET);
    const rates = [];
    let consumed;
    for (let sample = 0; sample < SAMPLE_COUNT; sample++) {
        machine.loadState(initial);
        const started = process.hrtime.bigint();
        consumed = machine.run(CYCLE_BUDGET);
        rates.push(consumed / (Number(process.hrtime.bigint() - started) / 1e9));
    }
    const cyclesPerSecond = [...rates].sort((a, b) => a - b)[Math.floor(SAMPLE_COUNT / 2)];

    assert.ok(consumed >= CYCLE_BUDGET, "run must consume the requested cycle budget");
    assert.equal(machine.cycleCount(), BigInt(consumed));
    assert.equal(machine.reg(Reg.BC), 0x3759, "BC must contain Fibonacci(10), Fibonacci(11)");
    assert.equal(machine.reg(Reg.AF) >>> 8, 0x59, "A must contain Fibonacci(11)");
    assert.equal(machine.reg(Reg.DE), 0x0000, "D loop counter must reach zero");

    console.log(`Fibonacci registers: BC=${machine.reg(Reg.BC).toString(16).padStart(4, "0")} A=${(machine.reg(Reg.AF) >>> 8).toString(16).padStart(2, "0")} DE=${machine.reg(Reg.DE).toString(16).padStart(4, "0")}`);
    console.log(`Cycles consumed: ${consumed}`);
    console.log(`Cycles/second: ${Math.round(cyclesPerSecond).toLocaleString("en-US")}`);
    console.log(JSON.stringify({ samples: rates, medianCyclesPerSecond: cyclesPerSecond }));

    console.log("P9.3 Node smoke: PASS");
} finally {
    machine.free();
}
