use crate::z180_bus;
use z180_core::HostBus;

pub(crate) struct CBus {
    pub callbacks: z180_bus,
    pub unmapped_read: u8,
}

fn result(code: i32) -> Result<(), i32> {
    if code == 0 { Ok(()) } else { Err(code) }
}

impl HostBus for CBus {
    type Error = i32;

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

