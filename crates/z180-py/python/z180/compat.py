"""Compatibility surface for the incumbent qns ``qns.cpu.Z180`` wrapper."""

from __future__ import annotations

from collections.abc import Callable

from ._native import IrqLine, Reg, _compat_machine


class Z180:
    """Callback-backed compatibility wrapper over the z180 core."""

    PC = 0x100000
    SP = 0x100001
    AF = 0x100002
    BC = 0x100003
    DE = 0x100004
    HL = 0x100005
    IX = 0x100006
    IY = 0x100007

    IRQ0 = 0
    IRQ1 = 1
    IRQ2 = 2

    CLEAR = 0
    ASSERT = 1

    _REGS = {
        PC: Reg.PC,
        SP: Reg.SP,
        AF: Reg.AF,
        BC: Reg.BC,
        DE: Reg.DE,
        HL: Reg.HL,
        IX: Reg.IX,
        IY: Reg.IY,
    }
    _IRQ_LINES = {
        IRQ0: IrqLine.Int0,
        IRQ1: IrqLine.Int1,
        IRQ2: IrqLine.Int2,
    }

    def __init__(
        self,
        clock: int = 12_288_000,
        mem_read: Callable[[int], int] | None = None,
        mem_write: Callable[[int, int], None] | None = None,
        io_read: Callable[[int], int] | None = None,
        io_write: Callable[[int, int], None] | None = None,
        serial_rx: Callable[[int], int] | None = None,
        serial_tx: Callable[[int, int], None] | None = None,
        csio_rx: Callable[[], int] | None = None,
        csio_tx: Callable[[int], None] | None = None,
    ):
        self.clock = clock
        self._machine = _compat_machine(
            clock,
            mem_read,
            mem_write,
            io_read,
            io_write,
        )
        self._serial_rx = serial_rx
        self._serial_tx = serial_tx
        self._csio_rx = csio_rx
        self._csio_tx = csio_tx
        self._serial_pending: list[int | None] = [None, None]
        self._csio_pending: int | None = None
        self._watch_address: int | None = None
        self._pc_watch_cycle = 0
        self._pc_watch_cbar = 0
        self._cycle_count = 0
        self._in_callback_step = False
        self._callback_regs: dict[int, int] = {}
        self._callback_cbr = 0
        self._callback_bbr = 0
        self._callback_cbar = 0xF0
        self._callback_halted = False
        self._deferred_irq_states: dict[int, bool] = {}

    def reset(self) -> None:
        self._machine.reset()
        self._cycle_count = 0
        self._serial_pending = [None, None]
        self._csio_pending = None
        self._pc_watch_cycle = 0
        self._pc_watch_cbar = 0
        self._in_callback_step = False
        self._callback_regs.clear()
        self._callback_cbr = 0
        self._callback_bbr = 0
        self._callback_cbar = 0xF0
        self._callback_halted = False
        self._deferred_irq_states.clear()

    def step(self) -> int:
        return self.run(1)

    def run(self, cycles: int) -> int:
        consumed = 0
        actual = 0
        self._pump_inputs()
        while consumed < cycles:
            for line, state in self._deferred_irq_states.items():
                self._machine.set_irq(self._IRQ_LINES[line], state)
            self._deferred_irq_states.clear()
            cbar_before = self.cbar
            cycle_before = self._cycle_count
            self._callback_regs = {
                reg: self._machine.reg(mapped) for reg, mapped in self._REGS.items()
            }
            self._callback_cbr = self._machine.io_reg_peek(0x38)
            self._callback_bbr = self._machine.io_reg_peek(0x39)
            self._callback_cbar = self._machine.io_reg_peek(0x3A)
            self._callback_halted = self._machine.halted()
            self._in_callback_step = True
            try:
                step_cycles = self._machine.step()
            finally:
                self._in_callback_step = False
            self._capture_watch(cbar_before, cycle_before)
            self._drain_outputs()
            if step_cycles == 0:
                break
            credited = min(step_cycles, cycles - consumed)
            consumed += credited
            actual += step_cycles
            self._cycle_count += credited
        return actual

    @property
    def cycle_count(self) -> int:
        return self._cycle_count

    def get_reg(self, reg: int) -> int:
        if self._in_callback_step:
            return self._callback_regs.get(reg, 0)
        mapped = self._REGS.get(reg)
        return 0 if mapped is None else self._machine.reg(mapped)

    @property
    def pc(self) -> int:
        if self._in_callback_step:
            return self._callback_regs[self.PC]
        return self._machine.reg(Reg.PC)

    @property
    def instruction_pc(self) -> int:
        if self._in_callback_step:
            return self._callback_regs[self.PC]
        return self._machine.instruction_pc()

    @property
    def sp(self) -> int:
        if self._in_callback_step:
            return self._callback_regs[self.SP]
        return self._machine.reg(Reg.SP)

    @property
    def halted(self) -> bool:
        if self._in_callback_step:
            return self._callback_halted
        return self._machine.halted()

    def set_irq(self, line: int, state: int) -> None:
        mapped = self._IRQ_LINES.get(line)
        if mapped is not None:
            if self._in_callback_step:
                self._deferred_irq_states[line] = bool(state)
            else:
                self._machine.set_irq(mapped, bool(state))

    @property
    def cbr(self) -> int:
        if self._in_callback_step:
            return self._callback_cbr
        return self._machine.io_reg_peek(0x38)

    @property
    def bbr(self) -> int:
        if self._in_callback_step:
            return self._callback_bbr
        return self._machine.io_reg_peek(0x39)

    @property
    def cbar(self) -> int:
        if self._in_callback_step:
            return self._callback_cbar
        return self._machine.io_reg_peek(0x3A)

    def asci_debug_state(self, channel: int) -> dict[str, int | bool]:
        """Return incumbent-shaped ASCI diagnostics.

        ``status``, ``cntla``, and ``tx_data_register`` are direct core
        register values. The core has no public equivalents for shift-stage,
        FIFO-depth, derived timing, IRQ-internal, or transition-history
        diagnostics, so those fields are explicitly zero/false.
        """
        if channel not in (0, 1):
            raise ValueError(f"ASCI channel must be 0 or 1, got {channel}")
        return {
            "status": self._machine.io_reg_peek(0x04 + channel),
            "rx_bits_remaining": 0,
            "rx_fifo_depth": 0,
            "cntla": self._machine.io_reg_peek(channel),
            "tx_bits_remaining": 0,
            "tx_shift_register": 0,
            "tx_data_register": self._machine.io_reg_peek(0x06 + channel),
            "irq_pending": False,
            "brg_divisor": 0,
            "frame_bits": 0,
            "rie_set_count": 0,
            "rie_clear_count": 0,
            "rie_last_pc": 0,
            "rie_last_cycle": 0,
            "stat_write_count": 0,
            "stat_last_write": 0,
            "stat_last_write_pc": 0,
            "stat_last_write_cycle": 0,
        }

    def reset_asci_debug(self) -> None:
        """No-op because all unsupported transition counters remain zero."""

    def watch_pc(self, address: int | None) -> None:
        if address is not None and not 0 <= address <= 0xFFFF:
            raise ValueError(f"PC watch address must be 0..FFFF, got {address}")
        self._watch_address = address
        self._pc_watch_cycle = 0
        self._pc_watch_cbar = 0
        self._machine.set_pc_watch(address)
        self._machine.set_insn_trace(1 if address is not None else None)

    @property
    def pc_watch_count(self) -> int:
        return self._machine.pc_watch_hits()

    @property
    def pc_watch_cycle(self) -> int:
        return self._pc_watch_cycle

    @property
    def pc_watch_cbar(self) -> int:
        return self._pc_watch_cbar

    def _pump_inputs(self) -> None:
        for channel in range(2):
            if self._serial_pending[channel] is None and self._serial_rx is not None:
                value = self._serial_rx(channel)
                if value >= 0:
                    self._serial_pending[channel] = value & 0xFF
            pending = self._serial_pending[channel]
            if pending is not None and self._machine.asci_rx_push(channel, pending):
                self._serial_pending[channel] = None

        if self._csio_pending is None and self._csio_rx is not None:
            value = self._csio_rx()
            if value >= 0:
                self._csio_pending = value & 0xFF
        if self._csio_pending is not None and self._machine.csio_rx_push(
            self._csio_pending
        ):
            self._csio_pending = None

    def _drain_outputs(self) -> None:
        for channel in range(2):
            while (value := self._machine.asci_tx_pop(channel)) is not None:
                if self._serial_tx is not None:
                    self._serial_tx(channel, value)
        while (value := self._machine.csio_tx_pop()) is not None:
            if self._csio_tx is not None:
                self._csio_tx(value)

    def _capture_watch(self, cbar_before: int, cycle_before: int) -> None:
        for entry in self._machine.drain_insn_trace():
            if entry["pc"] == self._watch_address:
                self._pc_watch_cycle = cycle_before
                self._pc_watch_cbar = cbar_before


__all__ = ["Z180"]
