#![allow(clippy::upper_case_acronyms)]
#![allow(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::needless_pass_by_value,
    clippy::ref_option,
    clippy::unused_self,
    reason = "PyO3 requires owned Python ABI values and raw-buffer callbacks; lengths are validated before fixed-width conversion"
)]

mod bus;
use bus::*;
mod config;
use config::*;
mod events;
mod machine;
use events::*;

use std::ffi::{CString, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pyo3::exceptions::{PyBufferError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyInt, PyList, PyMemoryView, PyModule};
use z180_core::{
    ConfigError, Event, HostBus, IrqLine as CoreIrqLine, IrqSource, MachineConfig, Reg as CoreReg,
    RegionDef, RegionKind, StateError, TraceEntry, Variant, WatchId as CoreWatchId,
    WatchKind as CoreWatchKind, Z180,
};

const EXT_MAP_TABLE_LEN: usize = 1 << 20;
const MAX_SAFE_INTEGER: i64 = (1_i64 << 53) - 1;
const RUN_RESPONSIVENESS_CYCLES: u32 = 1_000_000;

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

#[pyclass(module = "z180")]
struct Machine {
    inner: Z180<PythonBus>,
    active_views: Arc<AtomicUsize>,
    has_callbacks: bool,
}

#[pyclass]
struct RamView {
    owner: Py<Machine>,
    base: u32,
    len: usize,
    active_views: Arc<AtomicUsize>,
}

#[pyfunction]
#[pyo3(signature = (clock, mem_read=None, mem_write=None, io_read=None, io_write=None))]
fn _compat_machine(
    clock: u32,
    mem_read: Option<Py<PyAny>>,
    mem_write: Option<Py<PyAny>>,
    io_read: Option<Py<PyAny>>,
    io_write: Option<Py<PyAny>>,
) -> PyResult<Machine> {
    let has_callbacks =
        mem_read.is_some() || mem_write.is_some() || io_read.is_some() || io_write.is_some();
    let bus = PythonBus {
        unmapped_read: 0xff,
        mem_read,
        mem_write,
        io_read,
        io_write,
    };
    let config = MachineConfig {
        clock_hz: clock,
        regions: vec![RegionDef {
            base: 0,
            size: 1 << 20,
            kind: RegionKind::External,
        }],
        ..MachineConfig::default()
    };
    Ok(Machine {
        inner: Z180::new(config, bus).map_err(config_error)?,
        active_views: Arc::new(AtomicUsize::new(0)),
        has_callbacks,
    })
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Machine>()?;
    module.add_class::<PyReg>()?;
    module.add_class::<PyIrqLine>()?;
    module.add_class::<PyWatchKind>()?;
    module.add_class::<PyWatchId>()?;
    module.add_function(wrap_pyfunction!(_compat_machine, module)?)?;
    Ok(())
}
