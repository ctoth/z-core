const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
const $ = id => document.getElementById(id);
const hex = (v, width = 4) => v.toString(16).toUpperCase().padStart(width, '0');
let ready = false, running = false;
function controls() {
  for (const id of ['run', 'step', 'back', 'reset', 'sample', 'load', 'export', 'session']) $(id).disabled = !ready || running;
  $('pause').disabled = !running;
}
function send(command, data = {}) { worker.postMessage({ command, ...data }); }
function number(id, max) {
  const v = Number($(id).value);
  if (!Number.isSafeInteger(v) || v < 0 || v > max) throw new Error(`Invalid ${id}`);
  return v;
}
for (const command of ['run', 'step']) $(command).onclick = () => {
  try {
    const cycles = number('cycles', 0xffffffff);
    if (!cycles) throw new Error('Cycle budget must be positive');
    send(command, { cycles, breakpoint: $('breakpoint').value.trim() === '' ? null : number('breakpoint', 0xffff), watch: number('watch', 0xffffff) });
    running = true; controls();
  } catch (error) { $('status').textContent = error.message; }
};
for (const command of ['back', 'reset', 'sample', 'pause', 'export']) $(command).onclick = () => send(command);
$('inspect').onclick = () => {
  try { send('inspect', { address: number('address', 0xfffff0) }); }
  catch (error) { $('status').textContent = error.message; }
};
$('load').onclick = async () => {
  try {
    const file = $('rom').files?.[0];
    if (!file) throw new Error('Choose a ROM first.');
    send('load', { rom: new Uint8Array(await file.arrayBuffer()) });
  } catch (error) { $('status').textContent = error.message; }
};
$('session').onchange = async () => {
  try { const file = $('session').files?.[0]; if (file) send('import', { recording: JSON.parse(await file.text()) }); }
  catch (error) { $('status').textContent = error.message; }
};
worker.onmessage = ({ data }) => {
  if (data.recording) {
    const url = URL.createObjectURL(new Blob([JSON.stringify(data.recording)], { type: 'application/json' }));
    const link = document.createElement('a'); link.href = url; link.download = 'z180-session.json'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000); return;
  }
  ready = true; running = data.running ?? false; controls();
  $('status').textContent = data.error ?? data.status;
  if (!data.registers) return;
  $('registers').textContent = Object.entries(data.registers).map(([n, v]) => `${n.padEnd(3)} ${hex(v)}`).join('\n');
  $('memory').textContent = `${hex(data.address, 6)}: ${data.memory.map(v => hex(v, 2)).join(' ')}`;
  $('mapping').textContent = data.mapping.map(([l, p]) => `${hex(l)} → ${hex(p, 6)}`).join('\n');
  $('serial').textContent = data.serial || '(no serial output)';
  $('history').textContent = `Attempt ${data.attempt}; retained from ${data.oldest}; checkpoint bytes ${data.bytes.toLocaleString()}`;
};
worker.onerror = event => { ready = false; running = false; controls(); $('status').textContent = event.message; };
controls();
