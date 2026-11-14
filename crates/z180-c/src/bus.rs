use crate::{z180_bus, z180_bus_context, z180_context_bus};
use z180_core::{BusContext, HostBus};

pub(crate) struct CBus {
    pub callbacks: z180_bus,
    pub unmapped_read: u8,
    pub context_callbacks: Option<z180_context_bus>,
}

fn result(code: i32) -> Result<(), i32> {
    if code == 0 { Ok(()) } else { Err(code) }
}

impl HostBus for CBus {
    type Error = i32;

    fn mem_read_context(&mut self, address: u32, context: BusContext) -> Result<u8, i32> {
        let Some(bus) = self.context_callbacks else {
            return self.mem_read(address);
        };
        let mut value = self.unmapped_read;
        let context = z180_bus_context {
            cycles: context.cycles,
            pc: context.pc,
        };
        if let Some(callback) = bus.mem_read {
            // SAFETY: creation guarantees callback lifetime/non-reentry; snapshot lives through call.
            result(unsafe { callback(bus.user_data, address, &context, &mut value) })?;
        }
        Ok(value)
    }
    fn mem_write_context(
        &mut self,
        address: u32,
        value: u8,
        context: BusContext,
    ) -> Result<(), i32> {
        let Some(bus) = self.context_callbacks else {
            return self.mem_write(address, value);
        };
        let context = z180_bus_context {
            cycles: context.cycles,
            pc: context.pc,
        };
        if let Some(callback) = bus.mem_write {
            // SAFETY: creation guarantees callback lifetime/non-reentry; snapshot lives through call.
            result(unsafe { callback(bus.user_data, address, value, &context) })?;
        }
        Ok(())
    }
    fn io_read_context(&mut self, address: u16, context: BusContext) -> Result<u8, i32> {
        let Some(bus) = self.context_callbacks else {
            return self.io_read(address);
        };
        let mut value = self.unmapped_read;
        let context = z180_bus_context {
            cycles: context.cycles,
            pc: context.pc,
        };
        if let Some(callback) = bus.io_read {
            // SAFETY: creation guarantees callback lifetime/non-reentry; snapshot lives through call.
            result(unsafe { callback(bus.user_data, address, &context, &mut value) })?;
        }
        Ok(value)
    }
    fn io_write_context(
        &mut self,
        address: u16,
        value: u8,
        context: BusContext,
    ) -> Result<(), i32> {
        let Some(bus) = self.context_callbacks else {
            return self.io_write(address, value);
        };
        let context = z180_bus_context {
            cycles: context.cycles,
            pc: context.pc,
        };
        if let Some(callback) = bus.io_write {
            // SAFETY: creation guarantees callback lifetime/non-reentry; snapshot lives through call.
            result(unsafe { callback(bus.user_data, address, value, &context) })?;
        }
        Ok(())
    }

    fn mem_read(&mut self, address: u32) -> Result<u8, i32> {
        let mut value = self.unmapped_read;
        if let Some(callback) = self.callbacks.mem_read {
            // SAFETY: creation's caller guarantees callback/context lifetime and no reentry.
            result(unsafe { callback(self.callbacks.user_data, address, &mut value) })?;
        }
        Ok(value)
    }

    fn mem_write(&mut self, address: u32, value: u8) -> Result<(), i32> {
        if let Some(callback) = self.callbacks.mem_write {
            // SAFETY: creation's caller guarantees callback/context lifetime and no reentry.
            result(unsafe { callback(self.callbacks.user_data, address, value) })?;
        }
        Ok(())
    }

    fn io_read(&mut self, address: u16) -> Result<u8, i32> {
        let mut value = self.unmapped_read;
        if let Some(callback) = self.callbacks.io_read {
            // SAFETY: creation's caller guarantees callback/context lifetime and no reentry.
            result(unsafe { callback(self.callbacks.user_data, address, &mut value) })?;
        }
        Ok(value)
    }

    fn io_write(&mut self, address: u16, value: u8) -> Result<(), i32> {
        if let Some(callback) = self.callbacks.io_write {
            // SAFETY: creation's caller guarantees callback/context lifetime and no reentry.
            result(unsafe { callback(self.callbacks.user_data, address, value) })?;
        }
        Ok(())
    }
}
