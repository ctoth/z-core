import init, { Machine, Reg } from "../pkg/z180_wasm.js";

const PAGE_SIZE = 4096;
const MAX_ROM_SIZE = 1 << 20;
const REGISTER_ROWS = [
  ["PC", Reg.PC],
  ["SP", Reg.SP],
  ["AF", Reg.AF],
  ["BC", Reg.BC],
  ["DE", Reg.DE],
  ["HL", Reg.HL],
  ["IX", Reg.IX],
  ["IY", Reg.IY],
  ["AF'", Reg.AF2],
  ["BC'", Reg.BC2],
  ["DE'", Reg.DE2],
  ["HL'", Reg.HL2],
  ["IR", Reg.IR],
];

const romInput = document.querySelector("#rom");
const cyclesInput = document.querySelector("#cycles");
const runButton = document.querySelector("#run");
const status = document.querySelector("#status");
const registers = document.querySelector("#registers");
const serial = document.querySelector("#serial");

await init();
status.textContent = "WebAssembly loaded. Choose a ROM file.";

runButton.addEventListener("click", async () => {
  runButton.disabled = true;
  status.textContent = "Running…";

  try {
    const file = romInput.files?.[0];
    if (!file) {
      throw new Error("Choose a ROM file first.");
    }

    const cycleBudget = Number(cyclesInput.value);
    if (!Number.isInteger(cycleBudget) || cycleBudget < 1 || cycleBudget > 0xffffffff) {
      throw new Error("Cycle budget must be an integer from 1 through 4294967295.");
    }

    const rom = new Uint8Array(await file.arrayBuffer());
    if (rom.length === 0 || rom.length > MAX_ROM_SIZE) {
      throw new Error("ROM size must be from 1 byte through 1 MiB.");
    }

    const regionSize = Math.ceil(rom.length / PAGE_SIZE) * PAGE_SIZE;
    const regionData = new Uint8Array(regionSize);
    regionData.set(rom);

    const machine = new Machine({
      regions: [{ base: 0, size: regionSize, kind: "rom", data: regionData }],
    });
    const consumed = machine.run(cycleBudget);

    registers.textContent = REGISTER_ROWS
      .map(([name, reg]) => `${name.padEnd(3)} ${hex16(machine.reg(reg))}`)
      .join("\n");
    serial.textContent = readSerial(machine);
    status.textContent = `Ran ${consumed.toLocaleString()} cycles from ${file.name}.`;
    machine.free();
  } catch (error) {
    status.textContent = error instanceof Error ? error.message : String(error);
  } finally {
    runButton.disabled = false;
  }
});

function hex16(value) {
  return value.toString(16).toUpperCase().padStart(4, "0");
}

function readSerial(machine) {
  const decoder = new TextDecoder();
  const outputs = [];

  for (let channel = 0; channel < 2; channel += 1) {
    const bytes = [];
    for (let byte = machine.asciTxPop(channel); byte !== undefined; byte = machine.asciTxPop(channel)) {
      bytes.push(byte);
    }
    if (bytes.length > 0) {
      outputs.push(`ASCI${channel}:\n${decoder.decode(Uint8Array.from(bytes))}`);
    }
  }

  return outputs.join("\n\n") || "(no serial output)";
}
