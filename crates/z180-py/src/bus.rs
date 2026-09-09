use super::*;

pub(super) struct PythonBus {
    pub(super) unmapped_read: u8,
    pub(super) mem_read: Option<Py<PyAny>>,
    pub(super) mem_write: Option<Py<PyAny>>,
    pub(super) io_read: Option<Py<PyAny>>,
    pub(super) io_write: Option<Py<PyAny>>,
}

impl PythonBus {
    fn read_callback(
        callback: &Option<Py<PyAny>>,
        address: u32,
        name: &str,
    ) -> PyResult<Option<u8>> {
        let Some(callback) = callback else {
            return Ok(None);
        };
        Python::attach(|py| {
            let value = callback.bind(py).call1((address,))?;
            if value.is_instance_of::<PyBool>() || !value.is_instance_of::<PyInt>() {
                return Err(PyTypeError::new_err(format!(
                    "{name} callback must return an integer"
                )));
            }
            let integer = value.extract::<i64>().map_err(|_| {
                PyTypeError::new_err(format!("{name} callback must return an integer"))
            })?;
            if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&integer) {
                return Err(PyTypeError::new_err(format!(
                    "{name} callback must return an integer"
                )));
            }
            Ok(Some((integer & 0xff) as u8))
        })
    }

    fn write_callback(callback: &Option<Py<PyAny>>, address: u32, value: u8) -> PyResult<()> {
        let Some(callback) = callback else {
            return Ok(());
        };
        Python::attach(|py| {
            callback.bind(py).call1((address, value))?;
            Ok(())
        })
    }
}

impl HostBus for PythonBus {
    type Error = PyErr;

    fn mem_read(&mut self, phys: u32) -> PyResult<u8> {
        Ok(Self::read_callback(&self.mem_read, phys, "memRead")?.unwrap_or(self.unmapped_read))
    }

    fn mem_write(&mut self, phys: u32, value: u8) -> PyResult<()> {
        Self::write_callback(&self.mem_write, phys, value)
    }

    fn io_read(&mut self, port: u16) -> PyResult<u8> {
        Ok(
            Self::read_callback(&self.io_read, u32::from(port), "ioRead")?
                .unwrap_or(self.unmapped_read),
        )
    }

    fn io_write(&mut self, port: u16, value: u8) -> PyResult<()> {
        Self::write_callback(&self.io_write, u32::from(port), value)
    }
}
