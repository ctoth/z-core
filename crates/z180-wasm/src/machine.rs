use super::*;

fn state_error(error: StateError) -> JsValue {
    js_error(error.to_string())
}

#[wasm_bindgen]
impl Machine {
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<JsValue>, callbacks: Option<JsValue>) -> Result<Machine, JsValue> {
        let config = parse_config(config)?;
        let bus = parse_callbacks(callbacks, config.unmapped_read)?;
        let inner = Z180::new(config, bus).map_err(config_error)?;
        Ok(Self { inner })
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn step(&mut self) -> Result<u32, JsValue> {
        self.inner.try_step()
    }

    pub fn run(&mut self, cycles: u32) -> Result<u32, JsValue> {
        self.inner.try_run(cycles)
    }

    #[wasm_bindgen(js_name = cycleCount)]
    pub fn cycle_count(&self) -> u64 {
        self.inner.cycle_count()
    }

    pub fn halted(&self) -> bool {
        self.inner.halted()
    }

    pub fn sleeping(&self) -> bool {
        self.inner.sleeping()
    }

    pub fn reg(&self, reg: Reg) -> u16 {
        self.inner.reg(reg.into())
    }

    #[wasm_bindgen(js_name = setReg)]
    pub fn set_reg(&mut self, reg: Reg, value: u16) {
        self.inner.set_reg(reg.into(), value);
    }

    #[wasm_bindgen(js_name = instructionPc)]
    pub fn instruction_pc(&self) -> u16 {
        self.inner.instruction_pc()
    }

    pub fn iff1(&self) -> bool {
        self.inner.iff1()
    }

    #[wasm_bindgen(js_name = setIff1)]
    pub fn set_iff1(&mut self, enabled: bool) {
        self.inner.set_iff1(enabled);
    }

    pub fn iff2(&self) -> bool {
        self.inner.iff2()
    }

    #[wasm_bindgen(js_name = setIff2)]
    pub fn set_iff2(&mut self, enabled: bool) {
        self.inner.set_iff2(enabled);
    }

    #[wasm_bindgen(js_name = interruptMode)]
    pub fn interrupt_mode(&self) -> u8 {
        self.inner.interrupt_mode()
    }

    #[wasm_bindgen(js_name = setInterruptMode)]
    pub fn set_interrupt_mode(&mut self, mode: u8) {
        self.inner.set_interrupt_mode(mode);
    }

    #[wasm_bindgen(js_name = setIrq)]
    pub fn set_irq(&mut self, line: IrqLine, level: bool) {
        self.inner.set_irq(line.into(), level);
    }

    #[wasm_bindgen(js_name = setNmi)]
    pub fn set_nmi(&mut self, level: bool) {
        self.inner.set_nmi(level);
    }

    #[wasm_bindgen(js_name = setDreq)]
    pub fn set_dreq(&mut self, channel: usize, level: bool) -> Result<(), JsValue> {
        validate_channel(channel)?;
        self.inner.set_dreq(channel, level);
        Ok(())
    }

    #[wasm_bindgen(js_name = ioRegPeek)]
    pub fn io_reg_peek(&self, internal_addr: u8) -> u8 {
        self.inner.io_reg_peek(internal_addr)
    }

    #[wasm_bindgen(js_name = mmuTranslate)]
    pub fn mmu_translate(&self, logical: u16) -> u32 {
        self.inner.mmu_translate(logical)
    }

    #[wasm_bindgen(js_name = asciRxPush)]
    pub fn asci_rx_push(&mut self, channel: usize, byte: u8) -> Result<bool, JsValue> {
        validate_channel(channel)?;
        Ok(self.inner.asci_rx_push(channel, byte))
    }

    #[wasm_bindgen(js_name = asciTxPop)]
    pub fn asci_tx_pop(&mut self, channel: usize) -> Result<Option<u8>, JsValue> {
        validate_channel(channel)?;
        Ok(self.inner.asci_tx_pop(channel))
    }

    #[wasm_bindgen(js_name = csioRxPush)]
    pub fn csio_rx_push(&mut self, byte: u8) -> bool {
        self.inner.csio_rx_push(byte)
    }

    #[wasm_bindgen(js_name = csioTxPop)]
    pub fn csio_tx_pop(&mut self) -> Option<u8> {
        self.inner.csio_tx_pop()
    }

    #[wasm_bindgen(js_name = setAsciCts)]
    pub fn set_asci_cts(&mut self, channel: usize, level: bool) -> Result<(), JsValue> {
        validate_channel(channel)?;
        self.inner.set_asci_cts(channel, level);
        Ok(())
    }

    #[wasm_bindgen(js_name = setAsciDcd)]
    pub fn set_asci_dcd(&mut self, channel: usize, level: bool) -> Result<(), JsValue> {
        validate_channel(channel)?;
        self.inner.set_asci_dcd(channel, level);
        Ok(())
    }

    #[wasm_bindgen(js_name = memPeek)]
    pub fn mem_peek(&self, phys: u32) -> u8 {
        self.inner.mem_peek(phys)
    }

    #[wasm_bindgen(js_name = memPoke)]
    pub fn mem_poke(&mut self, phys: u32, value: u8) {
        self.inner.mem_poke(phys, value);
    }

    pub fn remap(
        &mut self,
        base: u32,
        size: u32,
        kind: &str,
        data: Option<Uint8Array>,
    ) -> Result<(), JsValue> {
        let data = data.map(|array| array.to_vec());
        let kind = match (kind, data) {
            ("ram", None) => RegionKind::Ram,
            ("rom", Some(data)) => RegionKind::Rom(data),
            ("external", None) => RegionKind::External,
            ("rom", None) => return Err(js_error("ROM remap requires data")),
            ("ram" | "external", Some(_)) => {
                return Err(js_error(format!("{kind} remap does not accept data")));
            }
            _ => return Err(js_error("kind must be 'ram', 'rom', or 'external'")),
        };
        self.inner.remap(base, size, kind).map_err(config_error)
    }

    #[wasm_bindgen(js_name = setExtMapper)]
    pub fn set_ext_mapper(&mut self, mapper: Option<Function>) -> Result<(), JsValue> {
        let table = if let Some(mapper) = mapper {
            let mut table = Vec::with_capacity(EXT_MAP_TABLE_LEN);
            for address in 0..EXT_MAP_TABLE_LEN as u32 {
                let value = mapper.call1(&JsValue::UNDEFINED, &JsValue::from(address))?;
                table.push(expect_u32(value, "external mapper result")?);
            }
            Some(table)
        } else {
            None
        };
        self.inner.set_ext_map_table(table).map_err(config_error)
    }

    #[wasm_bindgen(js_name = ramRegions)]
    pub fn ram_regions(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner.ram_regions())
            .map_err(|error| js_error(error.to_string()))
    }

    #[wasm_bindgen(js_name = ramCopy)]
    pub fn ram_copy(&self, base: u32) -> Result<Uint8Array, JsValue> {
        self.inner
            .ram_region(base)
            .map(Uint8Array::from)
            .ok_or_else(|| js_error(format!("no RAM region starts at {base:#x}")))
    }

    #[wasm_bindgen(js_name = loadRam)]
    pub fn load_ram(&mut self, base: u32, data: &[u8]) -> Result<(), JsValue> {
        let target = self
            .inner
            .ram_region_mut(base)
            .ok_or_else(|| js_error(format!("no RAM region starts at {base:#x}")))?;
        if target.len() != data.len() {
            return Err(js_error(format!(
                "RAM region at {base:#x} has size {:#x}; received {:#x} bytes",
                target.len(),
                data.len()
            )));
        }
        target.copy_from_slice(data);
        Ok(())
    }

    #[wasm_bindgen(js_name = addMemWatch)]
    pub fn add_mem_watch(&mut self, base: u32, size: u32, kind: WatchKind) -> WatchId {
        WatchId {
            inner: self.inner.add_mem_watch(base, size, kind.into()),
        }
    }

    #[wasm_bindgen(js_name = removeMemWatch)]
    pub fn remove_mem_watch(&mut self, id: &WatchId) {
        self.inner.remove_mem_watch(id.inner);
    }

    #[wasm_bindgen(js_name = setIoTrace)]
    pub fn set_io_trace(&mut self, enabled: bool) {
        self.inner.set_io_trace(enabled);
    }

    #[wasm_bindgen(js_name = setIrqTrace)]
    pub fn set_irq_trace(&mut self, enabled: bool) {
        self.inner.set_irq_trace(enabled);
    }

    #[wasm_bindgen(js_name = setPcWatch)]
    pub fn set_pc_watch(&mut self, address: Option<u16>) {
        self.inner.set_pc_watch(address);
    }

    #[wasm_bindgen(js_name = pcWatchHits)]
    pub fn pc_watch_hits(&self) -> u64 {
        self.inner.pc_watch_hits()
    }

    #[wasm_bindgen(js_name = drainEvents)]
    pub fn drain_events(&mut self) -> Result<JsValue, JsValue> {
        let events = Array::new();
        for event in self.inner.drain_events_iter() {
            events.push(&event_value(event)?);
        }
        Ok(events.into())
    }

    #[wasm_bindgen(js_name = eventsLost)]
    pub fn events_lost(&self) -> bool {
        self.inner.events_lost()
    }

    #[wasm_bindgen(js_name = clearEventsLost)]
    pub fn clear_events_lost(&mut self) {
        self.inner.clear_events_lost();
    }

    #[wasm_bindgen(js_name = setInsnTrace)]
    pub fn set_insn_trace(&mut self, capacity: Option<usize>) {
        self.inner.set_insn_trace(capacity);
    }

    #[wasm_bindgen(js_name = drainInsnTrace)]
    pub fn drain_insn_trace(&mut self) -> Result<JsValue, JsValue> {
        let entries = Array::new();
        for entry in self.inner.drain_insn_trace_iter() {
            entries.push(&trace_value(entry)?);
        }
        Ok(entries.into())
    }

    #[wasm_bindgen(js_name = saveState)]
    pub fn save_state(&self) -> Uint8Array {
        Uint8Array::from(self.inner.save_state().as_slice())
    }

    #[wasm_bindgen(js_name = loadState)]
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.inner.load_state(data).map_err(state_error)
    }

    #[wasm_bindgen(js_name = isInstructionImplemented)]
    pub fn is_instruction_implemented(opcodes: &[u8]) -> bool {
        Z180::<JsBus>::is_instruction_implemented(opcodes)
    }
}

fn validate_channel(channel: usize) -> Result<(), JsValue> {
    if channel > 1 {
        return Err(js_error("channel must be 0 or 1"));
    }
    Ok(())
}
