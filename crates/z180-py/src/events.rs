use super::*;

pub(super) fn event_dict(py: Python<'_>, event: Event) -> PyResult<Py<PyDict>> {
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

pub(super) fn trace_dict(py: Python<'_>, entry: TraceEntry) -> PyResult<Py<PyDict>> {
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
