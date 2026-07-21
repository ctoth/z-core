#![allow(clippy::upper_case_acronyms)]

use std::ffi::{CString, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pyo3::exceptions::{PyBufferError, PyKeyError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyMemoryView, PyModule};
use z180_core::{
    ConfigError, Event, HostBus, IrqLine as CoreIrqLine, IrqSource, MachineConfig, Reg as CoreReg,
    RegionDef, RegionKind, StateError, TraceEntry, Variant, WatchId as CoreWatchId,
    WatchKind as CoreWatchKind, Z180,
};

const EXT_MAP_TABLE_LEN: usize = 1 << 20;

struct NullBus {
    unmapped_read: u8,
}

impl HostBus for NullBus {
    fn mem_read(&mut self, _phys: u32) -> u8 {
        self.unmapped_read
    }

    fn mem_write(&mut self, _phys: u32, _value: u8) {}

    fn io_read(&mut self, _port: u16) -> u8 {
        self.unmapped_read
    }

    fn io_write(&mut self, _port: u16, _value: u8) {}
}

#[pyclass(name = "Reg", module = "z180", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum PyReg {
    PC,
    SP,
    AF,
    BC,
    DE,
    HL,
    IX,
    IY,
    AF2,
    BC2,
    DE2,
    HL2,
    IR,
}

impl From<PyReg> for CoreReg {
    fn from(value: PyReg) -> Self {
        match value {
            PyReg::PC => Self::PC,
            PyReg::SP => Self::SP,
            PyReg::AF => Self::AF,
            PyReg::BC => Self::BC,
            PyReg::DE => Self::DE,
            PyReg::HL => Self::HL,
            PyReg::IX => Self::IX,
            PyReg::IY => Self::IY,
            PyReg::AF2 => Self::AF2,
            PyReg::BC2 => Self::BC2,
            PyReg::DE2 => Self::DE2,
            PyReg::HL2 => Self::HL2,
            PyReg::IR => Self::IR,
        }
    }
}

#[pyclass(name = "IrqLine", module = "z180", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum PyIrqLine {
    Int0,
    Int1,
    Int2,
}

impl From<PyIrqLine> for CoreIrqLine {
    fn from(value: PyIrqLine) -> Self {
        match value {
            PyIrqLine::Int0 => Self::Int0,
            PyIrqLine::Int1 => Self::Int1,
            PyIrqLine::Int2 => Self::Int2,
        }
    }
}

#[pyclass(name = "WatchKind", module = "z180", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum PyWatchKind {
    Read,
    Write,
    Both,
}

impl From<PyWatchKind> for CoreWatchKind {
    fn from(value: PyWatchKind) -> Self {
        match value {
            PyWatchKind::Read => Self::Read,
            PyWatchKind::Write => Self::Write,
            PyWatchKind::Both => Self::Both,
        }
    }
}

#[pyclass(name = "WatchId", module = "z180", frozen)]
struct PyWatchId {
    inner: CoreWatchId,
}

#[pymethods]
impl PyWatchId {
    fn __repr__(&self) -> &'static str {
        "WatchId(<opaque>)"
    }
}

fn config_error(error: ConfigError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn state_error(error: StateError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn parse_config(config: Option<&Bound<'_, PyDict>>) -> PyResult<MachineConfig> {
    let mut parsed = MachineConfig::default();
    let Some(config) = config else {
        return Ok(parsed);
    };

    for (key, value) in config.iter() {
        let key: String = key.extract()?;
        match key.as_str() {
            "clock_hz" => parsed.clock_hz = value.extract()?,
            "phys_addr_bits" => parsed.phys_addr_bits = value.extract()?,
            "unmapped_read" => parsed.unmapped_read = value.extract()?,
            "variant" => {
                let variant: String = value.extract()?;
                parsed.variant = match variant.as_str() {
                    "Z80180" => Variant::Z80180,
                    "Z8S180" => Variant::Z8S180,
                    _ => {
                        return Err(PyValueError::new_err(
                            "variant must be 'Z80180' or 'Z8S180'",
                        ));
                    }
                };
            }
            "regions" => parsed.regions = parse_regions(value.cast::<PyList>()?)?,
            "event_capacity" => parsed.event_capacity = value.extract()?,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unknown config field: {key}"
                )));
            }
        }
    }
    Ok(parsed)
}

fn parse_regions(regions: &Bound<'_, PyList>) -> PyResult<Vec<RegionDef>> {
    regions
        .iter()
        .map(|item| parse_region(item.cast::<PyDict>()?))
        .collect()
}

fn parse_region(region: &Bound<'_, PyDict>) -> PyResult<RegionDef> {
    for (key, _) in region.iter() {
        let key: String = key.extract()?;
        if !matches!(key.as_str(), "base" | "size" | "kind" | "data") {
            return Err(PyValueError::new_err(format!(
                "unknown region field: {key}"
            )));
        }
    }

    let base: u32 = region
        .get_item("base")?
        .ok_or_else(|| PyKeyError::new_err("base"))?
        .extract()?;
    let size: u32 = region
        .get_item("size")?
        .ok_or_else(|| PyKeyError::new_err("size"))?
        .extract()?;
    let kind_name: String = region
        .get_item("kind")?
        .ok_or_else(|| PyKeyError::new_err("kind"))?
        .extract()?;
    let data = region.get_item("data")?;
    let kind = match kind_name.as_str() {
        "ram" => {
            reject_data(&data, "ram")?;
            RegionKind::Ram
        }
        "rom" => {
            let data = data.ok_or_else(|| PyValueError::new_err("ROM region requires data"))?;
            RegionKind::Rom(data.extract()?)
        }
        "external" => {
            reject_data(&data, "external")?;
            RegionKind::External
        }
        _ => {
            return Err(PyValueError::new_err(
                "region kind must be 'ram', 'rom', or 'external'",
            ));
        }
    };
    Ok(RegionDef { base, size, kind })
}

fn reject_data(data: &Option<Bound<'_, PyAny>>, kind: &str) -> PyResult<()> {
    if data.is_some() {
        return Err(PyValueError::new_err(format!(
            "{kind} region does not accept data"
        )));
    }
    Ok(())
}

fn remap_kind(kind: &str, data: Option<Vec<u8>>) -> PyResult<RegionKind> {
    match (kind, data) {
        ("ram", None) => Ok(RegionKind::Ram),
        ("rom", Some(data)) => Ok(RegionKind::Rom(data)),
        ("external", None) => Ok(RegionKind::External),
        ("rom", None) => Err(PyValueError::new_err("ROM remap requires data")),
        ("ram" | "external", Some(_)) => Err(PyValueError::new_err(format!(
            "{kind} remap does not accept data"
        ))),
        _ => Err(PyValueError::new_err(
            "kind must be 'ram', 'rom', or 'external'",
        )),
    }
}

#[pyclass(module = "z180")]
struct Machine {
    inner: Z180<NullBus>,
    active_views: Arc<AtomicUsize>,
}

#[pyclass]
struct RamView {
    owner: Py<Machine>,
    base: u32,
    len: usize,
    active_views: Arc<AtomicUsize>,
}

#[pymethods]
impl RamView {
    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        if view.is_null() {
            return Err(PyBufferError::new_err("view is null"));
        }

        let (owner, base, expected_len, active_views) = {
            let exporter = slf.borrow();
            (
                exporter.owner.clone_ref(slf.py()),
                exporter.base,
                exporter.len,
                Arc::clone(&exporter.active_views),
            )
        };
        let ptr = {
            let mut machine = owner.borrow_mut(slf.py());
            let data = machine
                .inner
                .ram_region_mut(base)
                .ok_or_else(|| PyBufferError::new_err("RAM region is no longer mapped"))?;
            if data.len() != expected_len {
                return Err(PyBufferError::new_err("RAM region size changed"));
            }
            data.as_mut_ptr()
        };

        unsafe {
            (*view).obj = slf.into_any().into_ptr();
            (*view).buf = ptr.cast::<c_void>();
            (*view).len = expected_len as isize;
            (*view).readonly = 0;
            (*view).itemsize = 1;
            (*view).format = if flags & ffi::PyBUF_FORMAT == ffi::PyBUF_FORMAT {
                CString::new("B")
                    .expect("static format has no NUL")
                    .into_raw()
            } else {
                ptr::null_mut()
            };
            (*view).ndim = 1;
            (*view).shape = if flags & ffi::PyBUF_ND == ffi::PyBUF_ND {
                &mut (*view).len
            } else {
                ptr::null_mut()
            };
            (*view).strides = if flags & ffi::PyBUF_STRIDES == ffi::PyBUF_STRIDES {
                &mut (*view).itemsize
            } else {
                ptr::null_mut()
            };
            (*view).suboffsets = ptr::null_mut();
            (*view).internal = ptr::null_mut();
        }
        active_views.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    unsafe fn __releasebuffer__(&self, view: *mut ffi::Py_buffer) {
        if !view.is_null() {
            let format = unsafe { (*view).format };
            if !format.is_null() {
                drop(unsafe { CString::from_raw(format) });
            }
        }
        self.active_views.fetch_sub(1, Ordering::Relaxed);
    }
}

#[pymethods]
impl Machine {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let config = parse_config(config)?;
        let bus = NullBus {
            unmapped_read: config.unmapped_read,
        };
        let inner = Z180::new(config, bus).map_err(config_error)?;
        Ok(Self {
            inner,
            active_views: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn step(&mut self) -> u32 {
        self.inner.step()
    }

    fn run(&mut self, cycles: u32) -> u32 {
        self.inner.run(cycles)
    }

    fn cycle_count(&self) -> u64 {
        self.inner.cycle_count()
    }

    fn halted(&self) -> bool {
        self.inner.halted()
    }

    fn sleeping(&self) -> bool {
        self.inner.sleeping()
    }

    fn reg(&self, reg: PyReg) -> u16 {
        self.inner.reg(reg.into())
    }

    fn set_reg(&mut self, reg: PyReg, value: u16) {
        self.inner.set_reg(reg.into(), value);
    }

    fn instruction_pc(&self) -> u16 {
        self.inner.instruction_pc()
    }

    fn iff1(&self) -> bool {
        self.inner.iff1()
    }

    fn set_iff1(&mut self, enabled: bool) {
        self.inner.set_iff1(enabled);
    }

    fn iff2(&self) -> bool {
        self.inner.iff2()
    }

    fn set_iff2(&mut self, enabled: bool) {
        self.inner.set_iff2(enabled);
    }

    fn interrupt_mode(&self) -> u8 {
        self.inner.interrupt_mode()
    }

    fn set_interrupt_mode(&mut self, mode: u8) {
        self.inner.set_interrupt_mode(mode);
    }

    fn set_irq(&mut self, line: PyIrqLine, level: bool) {
        self.inner.set_irq(line.into(), level);
    }

    fn set_nmi(&mut self, level: bool) {
        self.inner.set_nmi(level);
    }

    fn set_dreq(&mut self, channel: usize, level: bool) -> PyResult<()> {
        validate_channel(channel)?;
        self.inner.set_dreq(channel, level);
        Ok(())
    }

    fn io_reg_peek(&self, internal_addr: u8) -> u8 {
        self.inner.io_reg_peek(internal_addr)
    }

    fn mmu_translate(&self, logical: u16) -> u32 {
        self.inner.mmu_translate(logical)
    }

    fn asci_rx_push(&mut self, channel: usize, byte: u8) -> PyResult<bool> {
        validate_channel(channel)?;
        Ok(self.inner.asci_rx_push(channel, byte))
    }

    fn asci_tx_pop(&mut self, channel: usize) -> PyResult<Option<u8>> {
        validate_channel(channel)?;
        Ok(self.inner.asci_tx_pop(channel))
    }

    fn csio_rx_push(&mut self, byte: u8) -> bool {
        self.inner.csio_rx_push(byte)
    }

    fn csio_tx_pop(&mut self) -> Option<u8> {
        self.inner.csio_tx_pop()
    }

    fn set_asci_cts(&mut self, channel: usize, level: bool) -> PyResult<()> {
        validate_channel(channel)?;
        self.inner.set_asci_cts(channel, level);
        Ok(())
    }

    fn set_asci_dcd(&mut self, channel: usize, level: bool) -> PyResult<()> {
        validate_channel(channel)?;
        self.inner.set_asci_dcd(channel, level);
        Ok(())
    }

    fn mem_peek(&self, phys: u32) -> u8 {
        self.inner.mem_peek(phys)
    }

    fn mem_poke(&mut self, phys: u32, value: u8) {
        self.inner.mem_poke(phys, value);
    }

    #[pyo3(signature = (base, size, kind, data=None))]
    fn remap(&mut self, base: u32, size: u32, kind: &str, data: Option<Vec<u8>>) -> PyResult<()> {
        self.require_no_views("remap")?;
        self.inner
            .remap(base, size, remap_kind(kind, data)?)
            .map_err(config_error)
    }

    #[pyo3(signature = (mapper=None))]
    fn set_ext_mapper(slf: Py<Self>, py: Python<'_>, mapper: Option<Py<PyAny>>) -> PyResult<()> {
        let table = if let Some(mapper) = mapper {
            let mapper = mapper.bind(py);
            let mut table = Vec::with_capacity(EXT_MAP_TABLE_LEN);
            for address in 0..EXT_MAP_TABLE_LEN as u32 {
                table.push(mapper.call1((address,))?.extract::<u32>()?);
            }
            Some(table)
        } else {
            None
        };
        slf.borrow_mut(py)
            .inner
            .set_ext_map_table(table)
            .map_err(config_error)
    }

    fn ram_regions(&self) -> Vec<(u32, u32)> {
        self.inner.ram_regions()
    }

    fn ram(slf: Py<Self>, py: Python<'_>, base: u32) -> PyResult<Py<PyMemoryView>> {
        let (len, active_views) = {
            let machine = slf.borrow(py);
            let data = machine
                .inner
                .ram_region(base)
                .ok_or_else(|| PyKeyError::new_err(format!("no RAM region starts at {base:#x}")))?;
            (data.len(), Arc::clone(&machine.active_views))
        };
        let exporter = Py::new(
            py,
            RamView {
                owner: slf.clone_ref(py),
                base,
                len,
                active_views,
            },
        )?;
        Ok(PyMemoryView::from(exporter.bind(py).as_any())?.unbind())
    }

    fn add_mem_watch(&mut self, base: u32, size: u32, kind: PyWatchKind) -> PyWatchId {
        PyWatchId {
            inner: self.inner.add_mem_watch(base, size, kind.into()),
        }
    }

    fn remove_mem_watch(&mut self, id: PyRef<'_, PyWatchId>) {
        self.inner.remove_mem_watch(id.inner);
    }

    fn set_io_trace(&mut self, enabled: bool) {
        self.inner.set_io_trace(enabled);
    }

    fn set_irq_trace(&mut self, enabled: bool) {
        self.inner.set_irq_trace(enabled);
    }

    fn set_pc_watch(&mut self, addr: Option<u16>) {
        self.inner.set_pc_watch(addr);
    }

    fn pc_watch_hits(&self) -> u64 {
        self.inner.pc_watch_hits()
    }

    fn drain_events(&mut self, py: Python<'_>) -> PyResult<Vec<Py<PyDict>>> {
        self.inner
            .drain_events()
            .into_iter()
            .map(|event| event_dict(py, event))
            .collect()
    }

    fn events_lost(&self) -> bool {
        self.inner.events_lost()
    }

    fn clear_events_lost(&mut self) {
        self.inner.clear_events_lost();
    }

    #[pyo3(signature = (capacity=None))]
    fn set_insn_trace(&mut self, capacity: Option<usize>) {
        self.inner.set_insn_trace(capacity);
    }

    fn drain_insn_trace(&mut self, py: Python<'_>) -> PyResult<Vec<Py<PyDict>>> {
        self.inner
            .drain_insn_trace()
            .into_iter()
            .map(|entry| trace_dict(py, entry))
            .collect()
    }

    fn save_state(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.save_state()).unbind()
    }

    fn load_state(&mut self, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        self.require_no_views("load state")?;
        self.inner.load_state(data.as_bytes()).map_err(state_error)
    }

    #[staticmethod]
    fn is_instruction_implemented(opcodes: &[u8]) -> bool {
        Z180::<NullBus>::is_instruction_implemented(opcodes)
    }
}

impl Machine {
    fn require_no_views(&self, operation: &str) -> PyResult<()> {
        let count = self.active_views.load(Ordering::Relaxed);
        if count != 0 {
            return Err(PyBufferError::new_err(format!(
                "cannot {operation} while {count} RAM memoryview(s) are active"
            )));
        }
        Ok(())
    }
}

fn validate_channel(channel: usize) -> PyResult<()> {
    if channel > 1 {
        return Err(PyValueError::new_err("channel must be 0 or 1"));
    }
    Ok(())
}

fn event_dict(py: Python<'_>, event: Event) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    match event {
        Event::IoRead {
            cycle,
            pc,
            port,
            val,
        } => {
            dict.set_item("kind", "io_read")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("port", port)?;
            dict.set_item("value", val)?;
        }
        Event::IoWrite {
            cycle,
            pc,
            port,
            val,
        } => {
            dict.set_item("kind", "io_write")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("port", port)?;
            dict.set_item("value", val)?;
        }
        Event::MemWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            dict.set_item("kind", "mem_write")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("phys", phys)?;
            dict.set_item("value", val)?;
        }
        Event::MemRead {
            cycle,
            pc,
            phys,
            val,
        } => {
            dict.set_item("kind", "mem_read")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("phys", phys)?;
            dict.set_item("value", val)?;
        }
        Event::IrqAck {
            cycle,
            source,
            vector,
        } => {
            dict.set_item("kind", "irq_ack")?;
            dict.set_item("cycle", cycle)?;
            dict.set_item("source", irq_source_name(source))?;
            dict.set_item("vector", vector)?;
        }
        Event::Trap {
            cycle,
            pc,
            opcode,
            len,
        } => {
            dict.set_item("kind", "trap")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("opcode", PyBytes::new(py, &opcode[..usize::from(len)]))?;
            dict.set_item("len", len)?;
        }
        Event::RomWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            dict.set_item("kind", "rom_write")?;
            set_cycle_pc(&dict, cycle, pc)?;
            dict.set_item("phys", phys)?;
            dict.set_item("value", val)?;
        }
    }
    Ok(dict.unbind())
}

fn set_cycle_pc(dict: &Bound<'_, PyDict>, cycle: u64, pc: u16) -> PyResult<()> {
    dict.set_item("cycle", cycle)?;
    dict.set_item("pc", pc)?;
    Ok(())
}

fn irq_source_name(source: IrqSource) -> &'static str {
    match source {
        IrqSource::Nmi => "nmi",
        IrqSource::Int0 => "int0",
        IrqSource::Int1 => "int1",
        IrqSource::Int2 => "int2",
        IrqSource::Prt0 => "prt0",
        IrqSource::Prt1 => "prt1",
        IrqSource::Dma0 => "dma0",
        IrqSource::Dma1 => "dma1",
        IrqSource::Csio => "csio",
        IrqSource::Asci0 => "asci0",
        IrqSource::Asci1 => "asci1",
    }
}

fn trace_dict(py: Python<'_>, entry: TraceEntry) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("cycle", entry.cycle)?;
    dict.set_item("pc", entry.pc)?;
    dict.set_item("phys_pc", entry.phys_pc)?;
    dict.set_item(
        "bytes",
        PyBytes::new(py, &entry.bytes[..usize::from(entry.len)]),
    )?;
    dict.set_item("len", entry.len)?;
    Ok(dict.unbind())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Machine>()?;
    module.add_class::<PyReg>()?;
    module.add_class::<PyIrqLine>()?;
    module.add_class::<PyWatchKind>()?;
    module.add_class::<PyWatchId>()?;
    Ok(())
}
