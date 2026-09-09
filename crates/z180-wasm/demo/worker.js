import init, { Machine, Reg, WatchKind } from '../pkg-web/z180_wasm.js';

const MAX_BYTES = 16 * 1024 * 1024;
const INTERVAL = 256;
const sample = new Uint8Array([0x31, 0x00, 0xf0, 0x3e, 0x00, 0x3c, 0x32, 0x00, 0x10, 0x18, 0xfa]);
let machine, initial;
let checkpoints = [], attempt = 0, bytes = 0, address = 0x1000, serial = '', running = false;
const regs = ['PC', 'SP', 'AF', 'BC', 'DE', 'HL', 'IX', 'IY', 'AF2', 'BC2', 'DE2', 'HL2', 'IR'];

function remember(force = false) {
  if (!force && attempt % INTERVAL !== 0) return;
  if (checkpoints.some(cp => cp.attempt === attempt)) return;
  const state = machine.saveState();
  checkpoints.push({ attempt, state, serial }); bytes += state.length;
  while (bytes > MAX_BYTES && checkpoints.length > 1) bytes -= checkpoints.shift().state.length;
}
function snapshot(status) {
  postMessage({ status, running, attempt, oldest: checkpoints[0].attempt, bytes, serial,
    registers: Object.fromEntries(regs.map(name => [name, machine.reg(Reg[name])])),
    address, memory: Array.from({ length: 16 }, (_, i) => machine.memPeek(address + i)),
    mapping: Array.from({ length: 16 }, (_, i) => [i * 4096, machine.mmuTranslate(i * 4096)]) });
}
function restartHistory() { attempt = 0; checkpoints = []; bytes = 0; serial = ''; remember(true); }
function load(rom) {
  if (!(rom instanceof Uint8Array) || !rom.length || rom.length > 0xff000) throw new Error('ROM must contain 1 through 1,044,480 bytes, leaving at least one RAM page.');
  const size = Math.ceil(rom.length / 4096) * 4096;
  const data = new Uint8Array(size); data.set(rom);
  const next = new Machine({ eventCapacity: 1024, regions: [{ base: 0, size, kind: 'rom', data }, { base: size, size: Math.max(65536, size + 4096) - size, kind: 'ram' }] });
  try {
    next.addMemWatch(0, 1 << 24, WatchKind.Write).free();
    next.setReg(Reg.SP, 0xfffe);
    const state = next.saveState();
    machine?.free(); machine = next; initial = state;
  } catch (error) { next.free(); throw error; }
  restartHistory();
}
function one() {
  const cycles = machine.step(); attempt++;
  const events = machine.drainEvents();
  if (machine.eventsLost()) throw new Error('Event overflow; write-watch result is incomplete.');
  for (let channel = 0; channel < 2; channel++) {
    for (let byte = machine.asciTxPop(channel); byte !== undefined; byte = machine.asciTxPop(channel)) serial = (serial + String.fromCharCode(byte)).slice(-8192);
  }
  remember(); return { cycles, events };
}
function back() {
  const target = attempt - 1;
  const checkpoint = checkpoints.findLast(cp => cp.attempt <= target);
  if (!checkpoint) throw new Error('Beginning of retained history.');
  machine.loadState(checkpoint.state); serial = checkpoint.serial; attempt = checkpoint.attempt;
  checkpoints = checkpoints.filter(cp => cp.attempt <= target);
  bytes = checkpoints.reduce((sum, cp) => sum + cp.state.length, 0);
  // No external devices or host inputs: replay from a checkpoint is deterministic.
  while (attempt < target) one();
  snapshot('Stepped back one instruction.');
}
async function run(data) {
  if (!Number.isInteger(data.cycles) || data.cycles < 1 || data.cycles > 0xffffffff) throw new Error('Invalid cycle budget.');
  if (!Number.isInteger(data.watch) || data.watch < 0 || data.watch > 0xffffff) throw new Error('Invalid watch address.');
  if (data.breakpoint !== null && (!Number.isInteger(data.breakpoint) || data.breakpoint < 0 || data.breakpoint > 65535)) throw new Error('Invalid breakpoint.');
  running = true; let consumed = 0; let reason = 'Cycle budget reached.';
  while (running && consumed < data.cycles) {
    for (let i = 0; i < 256 && consumed < data.cycles; i++) {
      if (data.command !== 'step' && data.breakpoint === machine.reg(Reg.PC)) { reason = 'Breakpoint reached.'; running = false; break; }
      const result = one(); consumed += result.cycles;
      const hit = result.events.find(event => event.kind === 'mem_write' && event.phys === data.watch);
      if (hit) { reason = `Write at PC 0x${hit.pc.toString(16)}: 0x${hit.phys.toString(16)} = 0x${hit.value.toString(16)}. Back returns to the instruction responsible.`; running = false; break; }
      if (result.events.some(event => event.kind === 'trap')) { reason = 'Z180 TRAP.'; running = false; break; }
      if (!result.cycles || machine.sleeping() || machine.halted()) { reason = 'CPU halted or sleeping.'; running = false; break; }
      if (data.command === 'step') { reason = 'Stepped one instruction.'; running = false; break; }
    }
    if (running) { await new Promise(resolve => setTimeout(resolve, 0)); if (!running) reason = 'Paused.'; }
  }
  running = false; snapshot(reason);
}
function importSession(record) {
  if (record?.format !== 'z180-demo-session' || record.version !== 1 || !Number.isSafeInteger(record.attempt) || record.attempt < 0 || !Array.isArray(record.checkpoints) || !record.checkpoints.length) throw new Error('Unsupported session.');
  let total = 0, previous = -1;
  const decode = value => {
    if (!Array.isArray(value) || value.length > MAX_BYTES || value.some(v => !Number.isInteger(v) || v < 0 || v > 255)) throw new Error('Invalid state bytes.');
    return Uint8Array.from(value);
  };
  const candidate = new Machine();
  try {
    const loaded = record.checkpoints.map(cp => {
      if (!Number.isSafeInteger(cp.attempt) || cp.attempt <= previous || cp.attempt > record.attempt || typeof cp.serial !== 'string' || cp.serial.length > 8192) throw new Error('Invalid checkpoint index.');
      previous = cp.attempt; const state = decode(cp.state); total += state.length;
      if (total > MAX_BYTES) throw new Error('Session exceeds history budget.');
      candidate.loadState(state);
      return { attempt: cp.attempt, serial: cp.serial, state };
    });
    const state = decode(record.state); candidate.loadState(state);
    if (typeof record.serial !== 'string' || record.serial.length > 8192) throw new Error('Invalid serial output.');
    machine.loadState(state); initial = state; attempt = record.attempt; serial = record.serial;
    checkpoints = loaded; bytes = total;
  } finally { candidate.free(); }
}
await init(); load(sample);
snapshot('Sample loaded: Run stops when the counter writes RAM at 0x1000.');
self.onmessage = async ({ data }) => {
  try {
    if (data.command === 'pause') { running = false; return; }
    if (running && data.command !== 'inspect') throw new Error('Pause execution first.');
    switch (data.command) {
      case 'run': case 'step': await run(data); return;
      case 'back': back(); return;
      case 'sample': load(sample); break;
      case 'load': load(data.rom); break;
      case 'reset': machine.loadState(initial); restartHistory(); break;
      case 'inspect': if (!Number.isInteger(data.address) || data.address < 0 || data.address > 0xfffff0) throw new Error('Invalid memory address.'); address = data.address; break;
      case 'export': postMessage({ recording: { format: 'z180-demo-session', version: 1, attempt, serial, state: Array.from(machine.saveState()), checkpoints: checkpoints.map(cp => ({ ...cp, state: Array.from(cp.state) })) } }); return;
      case 'import': importSession(data.recording); break;
      default: throw new Error('Unknown command.');
    }
    snapshot('Ready.');
  } catch (error) { running = false; postMessage({ error: error instanceof Error ? error.message : String(error), running: false }); }
};
