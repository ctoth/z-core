#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "JavaScript numeric inputs are range-validated before conversion to fixed-width Z180 values"
)]
#![allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::ref_option,
    reason = "wasm-bindgen exports owned ABI values and JavaScript-facing Result/getter semantics rather than a native Rust API"
)]

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use z180_core::{
    ConfigError, Event as CoreEvent, HostBus, IrqLine as CoreIrqLine, IrqSource, MachineConfig,
    Reg as CoreReg, RegionDef, RegionKind, StateError, TraceEntry, Variant, WatchId as CoreWatchId,
    WatchKind as CoreWatchKind, Z180,
};

const EXT_MAP_TABLE_LEN: usize = 1 << 20;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT_REFINEMENTS: &str = include_str!("../types/refinements.d.ts");

struct JsBus {
    unmapped_read: u8,
    mem_read: Option<Function>,
    mem_write: Option<Function>,
    io_read: Option<Function>,
    io_write: Option<Function>,
    callback_error: Rc<RefCell<Option<JsValue>>>,
}

impl JsBus {
    fn read_callback(&self, callback: &Option<Function>, address: u32, name: &str) -> u8 {
        let Some(callback) = callback else {
            return self.unmapped_read;
        };
        let result = callback.call1(&JsValue::UNDEFINED, &JsValue::from(address));
        match result.and_then(|value| callback_byte(value, name)) {
            Ok(value) => value,
            Err(error) => {
                self.record_error(error);
                self.unmapped_read
            }
        }
    }

    fn write_callback(&self, callback: &Option<Function>, address: u32, value: u8, _name: &str) {
        let Some(callback) = callback else {
            return;
        };
        if let Err(error) = callback.call2(
            &JsValue::UNDEFINED,
            &JsValue::from(address),
            &JsValue::from(value),
        ) {
            self.record_error(error);
        }
    }

    fn record_error(&self, error: JsValue) {
        let mut pending = self.callback_error.borrow_mut();
        if pending.is_none() {
            *pending = Some(error);
        }
    }
}

impl HostBus for JsBus {
    fn mem_read(&mut self, phys: u32) -> u8 {
        self.read_callback(&self.mem_read, phys, "memRead")
    }

    fn mem_write(&mut self, phys: u32, value: u8) {
        self.write_callback(&self.mem_write, phys, value, "memWrite");
    }

    fn io_read(&mut self, port: u16) -> u8 {
        self.read_callback(&self.io_read, u32::from(port), "ioRead")
    }

    fn io_write(&mut self, port: u16, value: u8) {
        self.write_callback(&self.io_write, u32::from(port), value, "ioWrite");
    }
}

fn callback_byte(value: JsValue, name: &str) -> Result<u8, JsValue> {
    let Some(number) = value.as_f64() else {
        return Err(js_error(format!("{name} callback must return an integer")));
    };
    if !number.is_finite() || number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER {
        return Err(js_error(format!("{name} callback must return an integer")));
    }
    Ok(((number as i64) & 0xff) as u8)
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum Reg {
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

impl From<Reg> for CoreReg {
    fn from(value: Reg) -> Self {
        match value {
            Reg::PC => Self::PC,
            Reg::SP => Self::SP,
            Reg::AF => Self::AF,
            Reg::BC => Self::BC,
            Reg::DE => Self::DE,
            Reg::HL => Self::HL,
            Reg::IX => Self::IX,
            Reg::IY => Self::IY,
            Reg::AF2 => Self::AF2,
            Reg::BC2 => Self::BC2,
            Reg::DE2 => Self::DE2,
            Reg::HL2 => Self::HL2,
            Reg::IR => Self::IR,
        }
    }
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum IrqLine {
    Int0,
    Int1,
    Int2,
}

impl From<IrqLine> for CoreIrqLine {
    fn from(value: IrqLine) -> Self {
        match value {
            IrqLine::Int0 => Self::Int0,
            IrqLine::Int1 => Self::Int1,
            IrqLine::Int2 => Self::Int2,
        }
    }
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum WatchKind {
    Read,
    Write,
    Both,
}

impl From<WatchKind> for CoreWatchKind {
    fn from(value: WatchKind) -> Self {
        match value {
            WatchKind::Read => Self::Read,
            WatchKind::Write => Self::Write,
            WatchKind::Both => Self::Both,
        }
    }
}

#[wasm_bindgen]
pub struct WatchId {
    inner: CoreWatchId,
}

#[wasm_bindgen(skip_typescript)]
pub struct Machine {
    inner: Z180<JsBus>,
    callback_error: Rc<RefCell<Option<JsValue>>>,
}

fn parse_config(value: Option<JsValue>) -> Result<MachineConfig, JsValue> {
    let mut config = MachineConfig::default();
    let Some(value) = value.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(config);
    };
    let object = expect_object(value, "config")?;
    reject_unknown_keys(
        &object,
        &[
            "clockHz",
            "physAddrBits",
            "unmappedRead",
            "variant",
            "regions",
            "eventCapacity",
        ],
        "config",
    )?;

    if let Some(value) = optional_property(&object, "clockHz")? {
        config.clock_hz = expect_u32(value, "config.clockHz")?;
    }
    if let Some(value) = optional_property(&object, "physAddrBits")? {
        config.phys_addr_bits = expect_u8(value, "config.physAddrBits")?;
    }
    if let Some(value) = optional_property(&object, "unmappedRead")? {
        config.unmapped_read = expect_u8(value, "config.unmappedRead")?;
    }
    if let Some(value) = optional_property(&object, "variant")? {
        config.variant = match value.as_string().as_deref() {
            Some("Z80180") => Variant::Z80180,
            Some("Z8S180") => Variant::Z8S180,
            _ => return Err(js_error("config.variant must be 'Z80180' or 'Z8S180'")),
        };
    }
    if let Some(value) = optional_property(&object, "regions")? {
        config.regions = parse_regions(value)?;
    }
    if let Some(value) = optional_property(&object, "eventCapacity")? {
        config.event_capacity = expect_u32(value, "config.eventCapacity")? as usize;
    }
    Ok(config)
}

fn parse_regions(value: JsValue) -> Result<Vec<RegionDef>, JsValue> {
    if !Array::is_array(&value) {
        return Err(js_error("config.regions must be an array"));
    }
    Array::from(&value)
        .iter()
        .enumerate()
        .map(|(index, value)| parse_region(value, index))
        .collect()
}

fn parse_region(value: JsValue, index: usize) -> Result<RegionDef, JsValue> {
    let label = format!("config.regions[{index}]");
    let object = expect_object(value, &label)?;
    reject_unknown_keys(&object, &["base", "size", "kind", "data"], &label)?;
    let base = expect_u32(
        required_property(&object, "base", &label)?,
        &format!("{label}.base"),
    )?;
    let size = expect_u32(
        required_property(&object, "size", &label)?,
        &format!("{label}.size"),
    )?;
    let kind_value = required_property(&object, "kind", &label)?;
    let kind_name = kind_value
        .as_string()
        .ok_or_else(|| js_error(format!("{label}.kind must be a string")))?;
    let data = optional_property(&object, "data")?;
    let kind = match (kind_name.as_str(), data) {
        ("ram", None) => RegionKind::Ram,
        ("external", None) => RegionKind::External,
        ("rom", Some(data)) => RegionKind::Rom(expect_bytes(data, &format!("{label}.data"))?),
        ("rom", None) => return Err(js_error(format!("{label}.data is required for ROM"))),
        ("ram" | "external", Some(_)) => {
            return Err(js_error(format!("{label}.data is only valid for ROM")));
        }
        _ => {
            return Err(js_error(format!(
                "{label}.kind must be 'ram', 'rom', or 'external'"
            )));
        }
    };
    Ok(RegionDef { base, size, kind })
}

fn parse_callbacks(value: Option<JsValue>, unmapped_read: u8) -> Result<JsBus, JsValue> {
    let callback_error = Rc::new(RefCell::new(None));
    let mut bus = JsBus {
        unmapped_read,
        mem_read: None,
        mem_write: None,
        io_read: None,
        io_write: None,
        callback_error,
    };
    let Some(value) = value.filter(|value| !value.is_null() && !value.is_undefined()) else {
        return Ok(bus);
    };
    let object = expect_object(value, "callbacks")?;
    reject_unknown_keys(
        &object,
        &["memRead", "memWrite", "ioRead", "ioWrite"],
        "callbacks",
    )?;
    bus.mem_read = optional_function(&object, "memRead")?;
    bus.mem_write = optional_function(&object, "memWrite")?;
    bus.io_read = optional_function(&object, "ioRead")?;
    bus.io_write = optional_function(&object, "ioWrite")?;
    Ok(bus)
}

fn expect_object(value: JsValue, label: &str) -> Result<Object, JsValue> {
    if !value.is_object() || Array::is_array(&value) {
        return Err(js_error(format!("{label} must be an object")));
    }
    value
        .dyn_into::<Object>()
        .map_err(|_| js_error(format!("{label} must be an object")))
}

fn reject_unknown_keys(object: &Object, allowed: &[&str], label: &str) -> Result<(), JsValue> {
    for key in Object::keys(object).iter() {
        let key = key
            .as_string()
            .ok_or_else(|| js_error(format!("{label} contains a non-string key")))?;
        if !allowed.contains(&key.as_str()) {
            return Err(js_error(format!("unknown {label} field: {key}")));
        }
    }
    Ok(())
}

fn optional_property(object: &Object, name: &str) -> Result<Option<JsValue>, JsValue> {
    let value = Reflect::get(object, &JsValue::from_str(name))?;
    Ok((!value.is_null() && !value.is_undefined()).then_some(value))
}

fn required_property(object: &Object, name: &str, label: &str) -> Result<JsValue, JsValue> {
    optional_property(object, name)?.ok_or_else(|| js_error(format!("{label}.{name} is required")))
}

fn optional_function(object: &Object, name: &str) -> Result<Option<Function>, JsValue> {
    optional_property(object, name)?
        .map(|value| {
            value
                .dyn_into::<Function>()
                .map_err(|_| js_error(format!("callbacks.{name} must be a function")))
        })
        .transpose()
}

fn expect_u8(value: JsValue, label: &str) -> Result<u8, JsValue> {
    let number = expect_integer(value, label)?;
    if number > f64::from(u8::MAX) {
        return Err(js_error(format!("{label} must be in 0..=255")));
    }
    Ok(number as u8)
}

fn expect_u32(value: JsValue, label: &str) -> Result<u32, JsValue> {
    let number = expect_integer(value, label)?;
    if number > f64::from(u32::MAX) {
        return Err(js_error(format!("{label} must be in 0..=4294967295")));
    }
    Ok(number as u32)
}

fn expect_integer(value: JsValue, label: &str) -> Result<f64, JsValue> {
    let Some(number) = value.as_f64() else {
        return Err(js_error(format!("{label} must be an integer")));
    };
    if !number.is_finite() || number.fract() != 0.0 || number < 0.0 {
        return Err(js_error(format!("{label} must be a non-negative integer")));
    }
    Ok(number)
}

fn expect_bytes(value: JsValue, label: &str) -> Result<Vec<u8>, JsValue> {
    value
        .dyn_into::<Uint8Array>()
        .map(|array| array.to_vec())
        .map_err(|_| js_error(format!("{label} must be a Uint8Array")))
}

fn js_error(message: impl AsRef<str>) -> JsValue {
    js_sys::Error::new(message.as_ref()).into()
}

fn config_error(error: ConfigError) -> JsValue {
    js_error(error.to_string())
}

fn state_error(error: StateError) -> JsValue {
    js_error(error.to_string())
}

#[wasm_bindgen]
impl Machine {
    #[wasm_bindgen(constructor)]
    pub fn new(config: Option<JsValue>, callbacks: Option<JsValue>) -> Result<Machine, JsValue> {
        let config = parse_config(config)?;
        let bus = parse_callbacks(callbacks, config.unmapped_read)?;
        let callback_error = Rc::clone(&bus.callback_error);
        let inner = Z180::new(config, bus).map_err(config_error)?;
        Ok(Self {
            inner,
            callback_error,
        })
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn step(&mut self) -> Result<u32, JsValue> {
        let cycles = self.inner.step();
        self.return_callback_result(cycles)
    }

    pub fn run(&mut self, cycles: u32) -> Result<u32, JsValue> {
        let consumed = self.inner.run(cycles);
        self.return_callback_result(consumed)
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

    pub fn ram(&self, base: u32) -> Result<Uint8Array, JsValue> {
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
        for event in self.inner.drain_events() {
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
        for entry in self.inner.drain_insn_trace() {
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

impl Machine {
    fn return_callback_result<T>(&self, value: T) -> Result<T, JsValue> {
        if let Some(error) = self.callback_error.borrow_mut().take() {
            return Err(error);
        }
        Ok(value)
    }
}

fn validate_channel(channel: usize) -> Result<(), JsValue> {
    if channel > 1 {
        return Err(js_error("channel must be 0 or 1"));
    }
    Ok(())
}

fn event_value(event: CoreEvent) -> Result<JsValue, JsValue> {
    let object = Object::new();
    match event {
        CoreEvent::IoRead {
            cycle,
            pc,
            port,
            val,
        } => {
            set_string(&object, "kind", "io_read")?;
            set_cycle_pc(&object, cycle, pc)?;
            set_number(&object, "port", port)?;
            set_number(&object, "value", val)?;
        }
        CoreEvent::IoWrite {
            cycle,
            pc,
            port,
            val,
        } => {
            set_string(&object, "kind", "io_write")?;
            set_cycle_pc(&object, cycle, pc)?;
            set_number(&object, "port", port)?;
            set_number(&object, "value", val)?;
        }
        CoreEvent::MemWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            set_string(&object, "kind", "mem_write")?;
            set_cycle_pc(&object, cycle, pc)?;
            set_number(&object, "phys", phys)?;
            set_number(&object, "value", val)?;
        }
        CoreEvent::MemRead {
            cycle,
            pc,
            phys,
            val,
        } => {
            set_string(&object, "kind", "mem_read")?;
            set_cycle_pc(&object, cycle, pc)?;
            set_number(&object, "phys", phys)?;
            set_number(&object, "value", val)?;
        }
        CoreEvent::IrqAck {
            cycle,
            source,
            vector,
        } => {
            set_string(&object, "kind", "irq_ack")?;
            set_bigint(&object, "cycle", cycle)?;
            set_string(&object, "source", irq_source_name(source))?;
            set_number(&object, "vector", vector)?;
        }
        CoreEvent::Trap {
            cycle,
            pc,
            opcode,
            len,
        } => {
            set_string(&object, "kind", "trap")?;
            set_cycle_pc(&object, cycle, pc)?;
            let bytes = Uint8Array::from(&opcode[..usize::from(len)]);
            set_property(&object, "opcode", &bytes.into())?;
            set_number(&object, "len", len)?;
        }
        CoreEvent::RomWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            set_string(&object, "kind", "rom_write")?;
            set_cycle_pc(&object, cycle, pc)?;
            set_number(&object, "phys", phys)?;
            set_number(&object, "value", val)?;
        }
    }
    Ok(object.into())
}

fn trace_value(entry: TraceEntry) -> Result<JsValue, JsValue> {
    let object = Object::new();
    set_bigint(&object, "cycle", entry.cycle)?;
    set_number(&object, "pc", entry.pc)?;
    set_number(&object, "physPc", entry.phys_pc)?;
    let bytes = Uint8Array::from(&entry.bytes[..usize::from(entry.len)]);
    set_property(&object, "bytes", &bytes.into())?;
    set_number(&object, "len", entry.len)?;
    Ok(object.into())
}

fn set_cycle_pc(object: &Object, cycle: u64, pc: u16) -> Result<(), JsValue> {
    set_bigint(object, "cycle", cycle)?;
    set_number(object, "pc", pc)
}

fn set_string(object: &Object, name: &str, value: &str) -> Result<(), JsValue> {
    set_property(object, name, &JsValue::from_str(value))
}

fn set_number(object: &Object, name: &str, value: impl Into<f64>) -> Result<(), JsValue> {
    set_property(object, name, &JsValue::from_f64(value.into()))
}

fn set_bigint(object: &Object, name: &str, value: u64) -> Result<(), JsValue> {
    set_property(object, name, &js_sys::BigInt::from(value).into())
}

fn set_property(object: &Object, name: &str, value: &JsValue) -> Result<(), JsValue> {
    let set = Reflect::set(object, &JsValue::from_str(name), value)?;
    if !set {
        return Err(js_error(format!("could not set {name}")));
    }
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
