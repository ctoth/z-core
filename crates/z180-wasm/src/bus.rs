use super::*;

pub(super) struct JsBus {
    pub(super) unmapped_read: u8,
    pub(super) mem_read: Option<Function>,
    pub(super) mem_write: Option<Function>,
    pub(super) io_read: Option<Function>,
    pub(super) io_write: Option<Function>,
}

impl JsBus {
    fn read_callback(
        &self,
        callback: &Option<Function>,
        address: u32,
        name: &str,
    ) -> Result<u8, JsValue> {
        let Some(callback) = callback else {
            return Ok(self.unmapped_read);
        };
        callback
            .call1(&JsValue::UNDEFINED, &JsValue::from(address))
            .and_then(|value| callback_byte(value, name))
    }

    fn write_callback(
        &self,
        callback: &Option<Function>,
        address: u32,
        value: u8,
        _name: &str,
    ) -> Result<(), JsValue> {
        let Some(callback) = callback else {
            return Ok(());
        };
        callback
            .call2(
                &JsValue::UNDEFINED,
                &JsValue::from(address),
                &JsValue::from(value),
            )
            .map(|_| ())
    }
}

impl HostBus for JsBus {
    type Error = JsValue;

    fn mem_read(&mut self, phys: u32) -> Result<u8, Self::Error> {
        self.read_callback(&self.mem_read, phys, "memRead")
    }

    fn mem_write(&mut self, phys: u32, value: u8) -> Result<(), Self::Error> {
        self.write_callback(&self.mem_write, phys, value, "memWrite")
    }

    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
        self.read_callback(&self.io_read, u32::from(port), "ioRead")
    }

    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
        self.write_callback(&self.io_write, u32::from(port), value, "ioWrite")
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
