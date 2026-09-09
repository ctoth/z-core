use super::*;

pub(super) fn event_value(event: CoreEvent) -> Result<JsValue, JsValue> {
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

pub(super) fn trace_value(entry: TraceEntry) -> Result<JsValue, JsValue> {
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
