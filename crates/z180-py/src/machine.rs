use super::*;

fn state_error(error: StateError) -> PyErr {
    PyValueError::new_err(error.to_string())
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
            let exporter = slf
                .try_borrow()
                .map_err(|_| PyRuntimeError::new_err("machine is busy in a callback"))?;
            (
                exporter.owner.clone_ref(slf.py()),
                exporter.base,
                exporter.len,
                Arc::clone(&exporter.active_views),
            )
        };
        let ptr = {
            let mut machine = owner
                .try_borrow_mut(slf.py())
                .map_err(|_| PyRuntimeError::new_err("machine is busy in a callback"))?;
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
    #[pyo3(signature = (config_dict=None, *, mem_read=None, mem_write=None, io_read=None, io_write=None))]
    fn new(
        config_dict: Option<&Bound<'_, PyDict>>,
        mem_read: Option<Py<PyAny>>,
        mem_write: Option<Py<PyAny>>,
        io_read: Option<Py<PyAny>>,
        io_write: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let config = parse_config(config_dict)?;
        let has_callbacks =
            mem_read.is_some() || mem_write.is_some() || io_read.is_some() || io_write.is_some();
        let bus = PythonBus {
            unmapped_read: config.unmapped_read,
            mem_read,
            mem_write,
            io_read,
            io_write,
        };
        let inner = Z180::new(config, bus).map_err(config_error)?;
        Ok(Self {
            inner,
            active_views: Arc::new(AtomicUsize::new(0)),
            has_callbacks,
        })
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn step(&mut self) -> PyResult<u32> {
        self.inner.try_step()
    }

    fn run(&mut self, py: Python<'_>, cycles: u32) -> PyResult<u32> {
        let mut consumed = 0_u32;
        while consumed < cycles {
            let chunk_start = consumed;
            let chunk = cycles
                .saturating_sub(consumed)
                .min(RUN_RESPONSIVENESS_CYCLES);
            let chunk_consumed = if self.has_callbacks {
                self.inner.try_run(chunk)?
            } else {
                py.detach(|| self.inner.try_run(chunk))?
            };
            if chunk_consumed == 0 {
                break;
            }
            consumed = consumed.saturating_add(chunk_consumed);
            debug_assert!(consumed > chunk_start);
            py.check_signals()?;
        }
        Ok(consumed)
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
        slf.try_borrow_mut(py)
            .map_err(|_| PyRuntimeError::new_err("machine is busy in a callback"))?
            .inner
            .set_ext_map_table(table)
            .map_err(config_error)
    }

    fn ram_regions(&self) -> Vec<(u32, u32)> {
        self.inner.ram_regions()
    }

    fn ram(slf: Py<Self>, py: Python<'_>, base: u32) -> PyResult<Py<PyMemoryView>> {
        let (len, active_views) = {
            let machine = slf
                .try_borrow(py)
                .map_err(|_| PyRuntimeError::new_err("machine is busy in a callback"))?;
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
        Z180::<PythonBus>::is_instruction_implemented(opcodes)
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
