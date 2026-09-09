use super::*;

impl<B> Clone for ReplayBus<B> {
    fn clone(&self) -> Self {
        Self {
            shared: Rc::clone(&self.shared),
        }
    }
}

impl<B> ReplayBus<B> {
    pub(super) fn new(inner: B) -> Self {
        Self {
            shared: Rc::new(RefCell::new(SharedBus {
                inner,
                mode: Mode::Setup,
                records: Vec::new(),
                cursor: 0,
            })),
        }
    }

    pub(super) fn begin_live(&self) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        shared.mode = Mode::Live;
        shared.records.clear();
        shared.cursor = 0;
        Ok(())
    }

    pub(super) fn begin_playback(
        &self,
        cursor: usize,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        if cursor > shared.records.len() {
            return Err(ReplayBusError::Divergence {
                record: cursor,
                expected: None,
                actual: BusAccess::MemRead { address: 0 },
            });
        }
        shared.mode = Mode::Playback;
        shared.cursor = cursor;
        Ok(())
    }

    pub(super) fn restore_mode(
        &self,
        mode: Mode,
        cursor: usize,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        shared.mode = mode;
        shared.cursor = cursor;
        Ok(())
    }

    pub(super) fn record_count(&self) -> Result<usize, ReplayBusError<core::convert::Infallible>> {
        self.shared
            .try_borrow()
            .map(|shared| shared.records.len())
            .map_err(|_| ReplayBusError::Borrowed)
    }

    pub(super) fn cursor(&self) -> Result<usize, ReplayBusError<core::convert::Infallible>> {
        self.shared
            .try_borrow()
            .map(|shared| shared.cursor)
            .map_err(|_| ReplayBusError::Borrowed)
    }

    fn playback_read(
        shared: &mut SharedBus<B>,
        actual: BusAccess,
    ) -> Result<u8, ReplayBusError<core::convert::Infallible>> {
        let index = shared.cursor;
        let Some(record) = shared.records.get(index) else {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: None,
                actual,
            });
        };
        if record.access != actual {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: Some(record.access.clone()),
                actual,
            });
        }
        let failed = record.failed;
        let value = record.read_value;
        shared.cursor += 1;
        if failed {
            return Err(ReplayBusError::RecordedHostFailure { record: index });
        }
        value.ok_or_else(|| ReplayBusError::Divergence {
            record: index,
            expected: Some(record.access.clone()),
            actual: record.access.clone(),
        })
    }

    fn playback_write(
        shared: &mut SharedBus<B>,
        actual: BusAccess,
    ) -> Result<(), ReplayBusError<core::convert::Infallible>> {
        let index = shared.cursor;
        let Some(record) = shared.records.get(index) else {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: None,
                actual,
            });
        };
        if record.access != actual {
            return Err(ReplayBusError::Divergence {
                record: index,
                expected: Some(record.access.clone()),
                actual,
            });
        }
        let failed = record.failed;
        shared.cursor += 1;
        if failed {
            Err(ReplayBusError::RecordedHostFailure { record: index })
        } else {
            Ok(())
        }
    }
}

impl<B: HostBus> HostBus for ReplayBus<B> {
    type Error = ReplayBusError<B::Error>;

    fn mem_read(&mut self, address: u32) -> Result<u8, Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::MemRead { address };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_read(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.mem_read(address) {
            Ok(value) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: Some(value),
                    failed: false,
                });
                Ok(value)
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn mem_write(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::MemWrite { address, value };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_write(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.mem_write(address, value) {
            Ok(()) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: false,
                });
                Ok(())
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn io_read(&mut self, port: u16) -> Result<u8, Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::IoRead { port };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_read(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.io_read(port) {
            Ok(value) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: Some(value),
                    failed: false,
                });
                Ok(value)
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }

    fn io_write(&mut self, port: u16, value: u8) -> Result<(), Self::Error> {
        let mut shared = self
            .shared
            .try_borrow_mut()
            .map_err(|_| ReplayBusError::Borrowed)?;
        let access = BusAccess::IoWrite { port, value };
        if shared.mode == Mode::Playback {
            return ReplayBus::<B>::playback_write(&mut shared, access)
                .map_err(widen_infallible_error);
        }
        match shared.inner.io_write(port, value) {
            Ok(()) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: false,
                });
                Ok(())
            }
            Err(error) => {
                shared.records.push(BusRecord {
                    access,
                    read_value: None,
                    failed: true,
                });
                Err(ReplayBusError::Live(error))
            }
        }
    }
}

fn widen_infallible_error<E>(
    error: ReplayBusError<core::convert::Infallible>,
) -> ReplayBusError<E> {
    match error {
        ReplayBusError::Live(never) => match never {},
        ReplayBusError::RecordedHostFailure { record } => {
            ReplayBusError::RecordedHostFailure { record }
        }
        ReplayBusError::Divergence {
            record,
            expected,
            actual,
        } => ReplayBusError::Divergence {
            record,
            expected,
            actual,
        },
        ReplayBusError::Borrowed => ReplayBusError::Borrowed,
    }
}
