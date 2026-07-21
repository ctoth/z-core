import inspect

import pytest

import z180.compat as compat
from z180.compat import Z180


def test_constructor_and_incumbent_method_signatures_are_preserved():
    parameters = inspect.signature(Z180).parameters
    assert list(parameters) == [
        "clock",
        "mem_read",
        "mem_write",
        "io_read",
        "io_write",
        "serial_rx",
        "serial_tx",
        "csio_rx",
        "csio_tx",
    ]
    assert parameters["clock"].default == 12_288_000
    assert all(parameters[name].default is None for name in list(parameters)[1:])

    assert list(inspect.signature(Z180.run).parameters) == ["self", "cycles"]
    assert list(inspect.signature(Z180.get_reg).parameters) == ["self", "reg"]
    assert list(inspect.signature(Z180.set_irq).parameters) == [
        "self",
        "line",
        "state",
    ]
    assert list(inspect.signature(Z180.asci_debug_state).parameters) == [
        "self",
        "channel",
    ]
    assert list(inspect.signature(Z180.watch_pc).parameters) == ["self", "address"]


def test_external_memory_and_io_callbacks_execute_real_instructions():
    memory = {
        0: 0x3E,
        1: 0x42,
        2: 0x32,
        3: 0x00,
        4: 0x10,
        5: 0xED,
        6: 0x38,
        7: 0x40,
    }
    reads = []
    writes = []
    io_reads = []

    def mem_read(address):
        reads.append(address)
        return memory.get(address, 0xFF)

    def mem_write(address, value):
        writes.append((address, value))
        memory[address] = value

    def io_read(port):
        io_reads.append(port)
        return 0x15A

    cpu = Z180(mem_read=mem_read, mem_write=mem_write, io_read=io_read)
    assert cpu.step() > 0
    assert cpu.get_reg(cpu.AF) >> 8 == 0x42
    assert cpu.pc == 2

    assert cpu.step() > 0
    assert writes == [(0x1000, 0x42)]
    assert memory[0x1000] == 0x42

    assert cpu.step() > 0
    assert io_reads == [0x40]
    assert cpu.get_reg(cpu.AF) >> 8 == 0x5A
    assert reads[:2] == [0, 1]
    assert cpu.cycle_count > 0
    assert cpu.instruction_pc == 5
    assert cpu.halted is False


def test_callback_exceptions_propagate_after_the_active_instruction():
    def fail(_address):
        raise RuntimeError("memory callback failed")

    cpu = Z180(mem_read=fail)
    with pytest.raises(RuntimeError, match="memory callback failed"):
        cpu.step()


def test_io_callback_can_read_exact_compatibility_cycle_position():
    program = bytes((0xED, 0x39, 0x40, 0x76))
    observed = []
    cpu = None

    def io_write(_port, _value):
        observed.append(cpu.cycle_count)

    cpu = Z180(
        mem_read=lambda address: program[address] if address < len(program) else 0xFF,
        io_write=io_write,
    )
    assert cpu.run(100) >= 100
    assert observed == [0]
    assert cpu.cycle_count == 100
    assert cpu.run(25) >= 25
    assert cpu.cycle_count == 125
    cpu.reset()
    assert cpu.cycle_count == 0


def test_register_irq_mmu_watch_and_debug_compatibility_surface():
    cpu = Z180(mem_read=lambda _address: 0x00)
    assert cpu.clock == 12_288_000
    assert cpu.get_reg(cpu.PC) == 0
    assert cpu.get_reg(0xDEADBEEF) == 0
    assert cpu.pc == 0
    assert cpu.sp == 0
    assert cpu.cbr == 0
    assert cpu.bbr == 0
    assert cpu.cbar == 0xF0

    cpu.set_irq(cpu.IRQ0, cpu.CLEAR)
    cpu.set_irq(99, cpu.ASSERT)
    cpu.watch_pc(0)
    assert cpu.step() > 0
    assert cpu.pc_watch_count == 1
    assert cpu.pc_watch_cycle == 0
    assert cpu.pc_watch_cbar == 0xF0
    with pytest.raises(ValueError, match="PC watch address"):
        cpu.watch_pc(0x1_0000)

    debug = cpu.asci_debug_state(0)
    assert debug["status"] == 0x02
    assert debug["cntla"] == 0x10
    assert debug["tx_data_register"] == 0
    unsupported = set(debug) - {"status", "cntla", "tx_data_register"}
    assert all(debug[field] in (0, False) for field in unsupported)
    cpu.reset_asci_debug()
    with pytest.raises(ValueError, match="ASCI channel"):
        cpu.asci_debug_state(2)

    cpu.reset()
    assert cpu.pc == 0
    assert cpu.cycle_count == 0
    assert cpu.pc_watch_count == 0
    assert cpu.step() > 0
    assert cpu.pc_watch_count == 1
    assert cpu.pc_watch_cycle == 0


class FakeQueueMachine:
    def __init__(self):
        self.asci_pushes = []
        self.csio_pushes = []
        self.asci_attempts = 0
        self.csio_attempts = 0
        self.asci_output = [[0x51], []]
        self.csio_output = [0x52]

    def io_reg_peek(self, address):
        return 0xF0 if address == 0x3A else 0

    def step(self):
        return 3

    def drain_insn_trace(self):
        return []

    def asci_rx_push(self, channel, value):
        self.asci_pushes.append((channel, value))
        if channel != 0:
            return False
        self.asci_attempts += 1
        return self.asci_attempts > 1

    def csio_rx_push(self, value):
        self.csio_pushes.append(value)
        self.csio_attempts += 1
        return self.csio_attempts > 1

    def asci_tx_pop(self, channel):
        return self.asci_output[channel].pop(0) if self.asci_output[channel] else None

    def csio_tx_pop(self):
        return self.csio_output.pop(0) if self.csio_output else None


def test_serial_callbacks_adapt_to_queue_retry_and_drain(monkeypatch):
    machine = FakeQueueMachine()
    monkeypatch.setattr(compat, "_compat_machine", lambda *_args: machine)
    serial_rx_calls = []
    serial_tx = []
    csio_rx_calls = []
    csio_tx = []

    def serial_rx(channel):
        serial_rx_calls.append(channel)
        return 0x141 if channel == 0 else -1

    def csio_rx():
        csio_rx_calls.append(None)
        return 0x142

    cpu = Z180(
        serial_rx=serial_rx,
        serial_tx=lambda channel, value: serial_tx.append((channel, value)),
        csio_rx=csio_rx,
        csio_tx=csio_tx.append,
    )
    assert cpu.step() == 3
    assert cpu.step() == 3

    assert serial_rx_calls.count(0) == 1
    assert machine.asci_pushes == [(0, 0x41), (0, 0x41)]
    assert csio_rx_calls == [None]
    assert machine.csio_pushes == [0x42, 0x42]
    assert serial_tx == [(0, 0x51)]
    assert csio_tx == [0x52]
