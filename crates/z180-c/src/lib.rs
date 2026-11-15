//! C ABI 1 for z180-core.
//!
//! # Safety
//! All foreign pointers must reference live, aligned allocations of the stated
//! size. Inputs and outputs must not overlap each other or the machine. Outputs
//! may be uninitialized. One call at a time per machine, including getters and
//! destroy; callbacks must not reenter the same machine or unwind through Rust.
//! A nullable buffer is accepted only at length zero. See the public README.
#![allow(non_camel_case_types)]

mod bus;
mod types;
use bus::CBus;
use std::{
    ffi::c_char,
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
};
pub use types::*;
use z180_core::{
    Event, IrqLine, IrqSource, MachineConfig, Reg, RegionDef, RegionKind, SaveStateError, Variant,
    WatchId, WatchKind, Z180,
};

/// Opaque owned machine. Only z180_create and z180_destroy manage its allocation.
pub struct z180_machine {
    cpu: Z180<CBus>,
    poisoned: bool,
}

struct Failure {
    status: i32,
    host: i32,
    message: String,
}
type Result<T> = std::result::Result<T, Failure>;
fn fail(status: i32, message: impl ToString) -> Failure {
    Failure {
        status,
        host: 0,
        message: message.to_string(),
    }
}
fn invalid() -> Failure {
    fail(
        Z180_INVALID_ARGUMENT,
        "invalid pointer, range, or identifier",
    )
}
fn host_error(code: i32) -> Failure {
    Failure {
        status: Z180_HOST_ERROR,
        host: code,
        message: "host callback failed; prior effects may remain".into(),
    }
}
fn check<T>(data: *const T, len: usize) -> Result<()> {
    if len > isize::MAX as usize / mem::size_of::<T>()
        || (len != 0 && data.is_null())
        || (!data.is_null() && !(data as usize).is_multiple_of(mem::align_of::<T>()))
    {
        return Err(invalid());
    }
    Ok(())
}
fn output<T>(data: *mut T) -> Result<()> {
    check(data, 1)
}
// SAFETY: callers guarantee initialized readable storage; validation cannot
// establish allocation validity. The zero-length path never dereferences NULL.
unsafe fn input<'a, T>(data: *const T, len: usize) -> Result<&'a [T]> {
    check(data, len)?;
    if len == 0 {
        Ok(&[])
    } else {
        Ok(unsafe { slice::from_raw_parts(data, len) })
    }
}
fn boolean(value: u8) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid()),
    }
}
fn channel(value: u32) -> Result<usize> {
    if value < 2 {
        Ok(value as usize)
    } else {
        Err(invalid())
    }
}
fn register(value: u32) -> Result<Reg> {
    match value {
        Z180_REG_PC => Ok(Reg::PC),
        Z180_REG_SP => Ok(Reg::SP),
        Z180_REG_AF => Ok(Reg::AF),
        Z180_REG_BC => Ok(Reg::BC),
        Z180_REG_DE => Ok(Reg::DE),
        Z180_REG_HL => Ok(Reg::HL),
        Z180_REG_IX => Ok(Reg::IX),
        Z180_REG_IY => Ok(Reg::IY),
        Z180_REG_AF2 => Ok(Reg::AF2),
        Z180_REG_BC2 => Ok(Reg::BC2),
        Z180_REG_DE2 => Ok(Reg::DE2),
        Z180_REG_HL2 => Ok(Reg::HL2),
        Z180_REG_IR => Ok(Reg::IR),
        _ => Err(invalid()),
    }
}
unsafe fn region(value: &z180_region) -> Result<RegionDef> {
    let kind = match value.kind {
        Z180_REGION_ROM => {
            if value.data_len != value.size as usize {
                return Err(fail(
                    Z180_INVALID_CONFIG,
                    "ROM size differs from region size",
                ));
            }
            RegionKind::Rom(unsafe { input(value.data, value.data_len)? }.to_vec())
        }
        Z180_REGION_RAM if value.data_len == 0 => RegionKind::Ram,
        Z180_REGION_EXTERNAL if value.data_len == 0 => RegionKind::External,
        _ => return Err(invalid()),
    };
    Ok(RegionDef {
        base: value.base,
        size: value.size,
        kind,
    })
}
// Write without creating a reference to possibly uninitialized C output.
unsafe fn finish(error: *mut z180_error, result: Result<()>) -> i32 {
    let (status, host_code, message) = match result {
        Ok(()) => (Z180_OK, 0, String::new()),
        Err(failure) => (failure.status, failure.host, failure.message),
    };
    if !error.is_null() {
        let mut record = z180_error {
            status,
            host_code,
            message: [0; 256],
        };
        let mut end = message.len().min(255);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        for (dest, byte) in record.message.iter_mut().zip(message.bytes().take(end)) {
            *dest = byte as c_char;
        }
        unsafe {
            error.write(record);
        }
    }
    status
}
unsafe fn boundary(error: *mut z180_error, f: impl FnOnce() -> Result<()>) -> i32 {
    if !error.is_null() && output(error).is_err() {
        return Z180_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        // A panic payload can itself have a panicking destructor. Never let it
        // unwind across C; leaking it on this exceptional path is deliberate.
        mem::forget(payload);
        Err(fail(Z180_INTERNAL_ERROR, "Rust panic"))
    });
    unsafe { finish(error, result) }
}
unsafe fn machine(
    handle: *mut z180_machine,
    error: *mut z180_error,
    f: impl FnOnce(&mut Z180<CBus>) -> Result<()>,
) -> i32 {
    unsafe {
        boundary(error, || {
            output(handle)?;
            let state = &mut *handle;
            if state.poisoned {
                return Err(fail(
                    Z180_POISONED,
                    "machine was interrupted by a Rust panic",
                ));
            }
            match catch_unwind(AssertUnwindSafe(|| f(&mut state.cpu))) {
                Ok(result) => result,
                Err(payload) => {
                    state.poisoned = true;
                    mem::forget(payload);
                    Err(fail(
                        Z180_INTERNAL_ERROR,
                        "Rust panic; machine is now poisoned",
                    ))
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn z180_abi_version() -> u32 {
    1
}

/// Initialize configuration with core defaults.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_config_init(config: *mut z180_config, error: *mut z180_error) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        boundary(error, || {
            output(config)?;
            let c = MachineConfig::default();
            config.write(z180_config {
                clock_hz: c.clock_hz,
                phys_addr_bits: c.phys_addr_bits,
                unmapped_read: c.unmapped_read,
                variant: Z180_Z80180,
                event_capacity: c.event_capacity,
            });
            Ok(())
        })
    }
}

/// Create an owned machine; copies descriptors/ROM/callbacks. Clears out_machine on failure.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_create(
    config: *const z180_config,
    regions: *const z180_region,
    region_count: usize,
    bus: *const z180_bus,
    out_machine: *mut *mut z180_machine,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: same contract as create_impl; NULL selects legacy callbacks.
    unsafe {
        create_impl(
            config,
            regions,
            region_count,
            bus,
            ptr::null(),
            out_machine,
            error,
        )
    }
}

/// Create using immutable cycle/PC callbacks instead of the legacy bus.
/// # Safety
/// Same pointer, lifetime, exclusivity and non-reentry contract as z180_create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_create_with_context(
    config: *const z180_config,
    regions: *const z180_region,
    region_count: usize,
    bus: *const z180_context_bus,
    out_machine: *mut *mut z180_machine,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: caller supplies the same foreign-storage contract as z180_create.
    unsafe {
        create_impl(
            config,
            regions,
            region_count,
            ptr::null(),
            bus,
            out_machine,
            error,
        )
    }
}

unsafe fn create_impl(
    config: *const z180_config,
    regions: *const z180_region,
    region_count: usize,
    bus: *const z180_bus,
    context_bus: *const z180_context_bus,
    out_machine: *mut *mut z180_machine,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        boundary(error, || {
            output(out_machine)?;
            out_machine.write(ptr::null_mut());
            let config = input(config, 1)?[0];
            let variant = match config.variant {
                Z180_Z80180 => Variant::Z80180,
                Z180_Z8S180 => Variant::Z8S180,
                _ => return Err(invalid()),
            };
            let defs = input(regions, region_count)?
                .iter()
                .map(|r| region(r))
                .collect::<Result<Vec<_>>>()?;
            let callbacks = if bus.is_null() {
                z180_bus::default()
            } else {
                input(bus, 1)?[0]
            };
            let cpu = Z180::new(
                MachineConfig {
                    clock_hz: config.clock_hz,
                    phys_addr_bits: config.phys_addr_bits,
                    unmapped_read: config.unmapped_read,
                    variant,
                    regions: defs,
                    event_capacity: config.event_capacity,
                },
                CBus {
                    callbacks,
                    unmapped_read: config.unmapped_read,
                    context_callbacks: if context_bus.is_null() {
                        None
                    } else {
                        Some(input(context_bus, 1)?[0])
                    },
                    fetch_observer: z180_fetch_observer::default(),
                },
            )
            .map_err(|e| fail(Z180_INVALID_CONFIG, e))?;
            out_machine.write(Box::into_raw(Box::new(z180_machine {
                cpu,
                poisoned: false,
            })));
            Ok(())
        })
    }
}

/// Replace the opcode-fetch observer. NULL unregisters. Reset/load preserve it.
/// # Safety
/// Follow the pointer, callback lifetime, exclusivity and non-reentry contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_fetch_observer(
    handle: *mut z180_machine,
    observer: *const z180_fetch_observer,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: caller provides valid foreign storage and callback lifetime.
    unsafe {
        machine(handle, error, |cpu| {
            let observer = if observer.is_null() {
                z180_fetch_observer::default()
            } else {
                input(observer, 1)?[0]
            };
            cpu.host_bus_mut().fetch_observer = observer;
            Ok(())
        })
    }
}

/// Destroy an owned handle. NULL is accepted; every other copy becomes invalid.
/// # Safety
/// Handle must be NULL or a live handle from create, with no call in progress.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_destroy(handle: *mut z180_machine) {
    if !handle.is_null() {
        // SAFETY: the caller transfers the unique live allocation back.
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}

/// Reset hardware state, preserving the core's cycle counter and memory.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_reset(handle: *mut z180_machine, error: *mut z180_error) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.reset();
            Ok(())
        })
    }
}

/// Report charged cycle-counter delta even on host failure; prior effects may remain.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_step(
    handle: *mut z180_machine,
    out_cycles: *mut u64,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_cycles)?;
            let before = cpu.cycle_count();
            let result = cpu.try_step();
            out_cycles.write(cpu.cycle_count() - before);
            result.map(|_| ()).map_err(host_error)
        })
    }
}

/// Report charged cycle-counter delta even on host failure; prior effects may remain.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_run(
    handle: *mut z180_machine,
    budget: u32,
    out_cycles: *mut u64,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_cycles)?;
            let before = cpu.cycle_count();
            let result = cpu.try_run(budget);
            out_cycles.write(cpu.cycle_count() - before);
            result.map(|_| ()).map_err(host_error)
        })
    }
}

/// Inspect execution state without advancing the machine.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_get_status(
    handle: *mut z180_machine,
    out_status: *mut z180_status,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_status)?;
            out_status.write(z180_status {
                cycles: cpu.cycle_count(),
                instruction_pc: cpu.instruction_pc(),
                halted: u8::from(cpu.halted()),
                sleeping: u8::from(cpu.sleeping()),
                iff1: u8::from(cpu.iff1()),
                iff2: u8::from(cpu.iff2()),
                interrupt_mode: cpu.interrupt_mode(),
                itc: cpu.itc(),
            });
            Ok(())
        })
    }
}

/// Read one of the named Z180_REG registers.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_reg_get(
    handle: *mut z180_machine,
    id: u32,
    out_value: *mut u16,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            let id = register(id)?;
            out_value.write(cpu.reg(id));
            Ok(())
        })
    }
}

/// Write one of the named Z180_REG registers.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_reg_set(
    handle: *mut z180_machine,
    id: u32,
    value: u16,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_reg(register(id)?, value);
            Ok(())
        })
    }
}

/// Read interrupt flip-flop as 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_get_iff1(
    handle: *mut z180_machine,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            out_value.write(u8::from(cpu.iff1()));
            Ok(())
        })
    }
}

/// Set interrupt flip-flop; value must be 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_iff1(
    handle: *mut z180_machine,
    value: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_iff1(boolean(value)?);
            Ok(())
        })
    }
}

/// Read interrupt flip-flop as 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_get_iff2(
    handle: *mut z180_machine,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            out_value.write(u8::from(cpu.iff2()));
            Ok(())
        })
    }
}

/// Set interrupt flip-flop; value must be 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_iff2(
    handle: *mut z180_machine,
    value: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_iff2(boolean(value)?);
            Ok(())
        })
    }
}

/// Read interrupt mode.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_get_interrupt_mode(
    handle: *mut z180_machine,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            out_value.write(cpu.interrupt_mode());
            Ok(())
        })
    }
}

/// Set interrupt mode 0, 1, or 2.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_interrupt_mode(
    handle: *mut z180_machine,
    value: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            if value > 2 {
                return Err(invalid());
            }
            cpu.set_interrupt_mode(value);
            Ok(())
        })
    }
}

/// Set INT0, INT1, or INT2 level.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_irq(
    handle: *mut z180_machine,
    line: u32,
    level: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            let line = match line {
                Z180_INT0 => IrqLine::Int0,
                Z180_INT1 => IrqLine::Int1,
                Z180_INT2 => IrqLine::Int2,
                _ => return Err(invalid()),
            };
            cpu.set_irq(line, boolean(level)?);
            Ok(())
        })
    }
}

/// Set a board pin; level must be 0 or 1. DCD applies to channel zero.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_nmi(
    handle: *mut z180_machine,
    level: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_nmi(boolean(level)?);
            Ok(())
        })
    }
}

/// Set a board pin; level must be 0 or 1. DCD applies to channel zero.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_dreq(
    handle: *mut z180_machine,
    ch: u32,
    level: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_dreq(channel(ch)?, boolean(level)?);
            Ok(())
        })
    }
}

/// Set a board pin; level must be 0 or 1. DCD applies to channel zero.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_asci_cts(
    handle: *mut z180_machine,
    ch: u32,
    level: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_asci_cts(channel(ch)?, boolean(level)?);
            Ok(())
        })
    }
}

/// Set a board pin; level must be 0 or 1. DCD applies to channel zero.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_asci_dcd(
    handle: *mut z180_machine,
    level: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_asci_dcd(0, boolean(level)?);
            Ok(())
        })
    }
}

/// Offer a received byte; accepted=0 is normal backpressure/disabled receiver.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_asci_rx_push(
    handle: *mut z180_machine,
    ch: u32,
    value: u8,
    out_accepted: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_accepted)?;
            let ch = channel(ch)?;
            out_accepted.write(u8::from(cpu.asci_rx_push(ch, value)));
            Ok(())
        })
    }
}

/// Pop a transmitted byte; empty returns present=0 and value=0.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_asci_tx_pop(
    handle: *mut z180_machine,
    ch: u32,
    out_present: *mut u8,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_present)?;
            output(out_value)?;
            let ch = channel(ch)?;
            let value = cpu.asci_tx_pop(ch);
            out_present.write(u8::from(value.is_some()));
            out_value.write(value.unwrap_or(0));
            Ok(())
        })
    }
}

/// Offer a received byte; accepted=0 is normal backpressure/disabled receiver.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_csio_rx_push(
    handle: *mut z180_machine,
    value: u8,
    out_accepted: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_accepted)?;
            out_accepted.write(u8::from(cpu.csio_rx_push(value)));
            Ok(())
        })
    }
}

/// Pop a transmitted byte; empty returns present=0 and value=0.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_csio_tx_pop(
    handle: *mut z180_machine,
    out_present: *mut u8,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_present)?;
            output(out_value)?;
            let value = cpu.csio_tx_pop();
            out_present.write(u8::from(value.is_some()));
            out_value.write(value.unwrap_or(0));
            Ok(())
        })
    }
}

/// Inspect physical RAM/ROM; external/unmapped reads use the configured byte.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_mem_peek(
    handle: *mut z180_machine,
    physical: u32,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            out_value.write(cpu.mem_peek(physical));
            Ok(())
        })
    }
}

/// Debug write to RAM only; no callbacks, ROM writes, or watch events.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_mem_poke(
    handle: *mut z180_machine,
    physical: u32,
    value: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.mem_poke(physical, value);
            Ok(())
        })
    }
}

/// Translate through the MMU, before the optional external mapping table.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_mmu_translate(
    handle: *mut z180_machine,
    logical: u16,
    out_physical: *mut u32,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_physical)?;
            out_physical.write(cpu.mmu_translate(logical));
            Ok(())
        })
    }
}

/// Inspect internal I/O register 0..63 without read side effects.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_io_reg_peek(
    handle: *mut z180_machine,
    index: u8,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            if index >= 64 {
                return Err(invalid());
            }
            out_value.write(cpu.io_reg_peek(index));
            Ok(())
        })
    }
}

/// Replace a page-aligned region; ROM bytes are copied.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_remap(
    handle: *mut z180_machine,
    descriptor: *const z180_region,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            let r = region(&input(descriptor, 1)?[0])?;
            cpu.remap(r.base, r.size, r.kind)
                .map_err(|e| fail(Z180_INVALID_CONFIG, e))
        })
    }
}

/// Copy a complete 2^20-entry mapping table. NULL/zero clears it.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_ext_map_table(
    handle: *mut z180_machine,
    data: *const u32,
    count: usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            if count != 0 && count != 1 << 20 {
                return Err(invalid());
            }
            let entries = input(data, count)?;
            cpu.set_ext_map_table(if count == 0 {
                None
            } else {
                Some(entries.to_vec())
            })
            .map_err(|e| fail(Z180_INVALID_CONFIG, e))
        })
    }
}

/// Enumerate RAM. Short/query buffers report required count without partial output.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_ram_regions(
    handle: *mut z180_machine,
    regions: *mut z180_ram_region,
    capacity: usize,
    out_count: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_count)?;
            check(regions, capacity)?;
            let entries = cpu.ram_regions();
            out_count.write(entries.len());
            if capacity < entries.len() {
                return Err(fail(Z180_BUFFER_TOO_SMALL, "RAM region buffer too small"));
            }
            for (i, (base, size)) in entries.into_iter().enumerate() {
                regions.add(i).write(z180_ram_region { base, size });
            }
            Ok(())
        })
    }
}

/// Copy bytes within one RAM region; validate the entire range before copying.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_ram_read(
    handle: *mut z180_machine,
    base: u32,
    offset: usize,
    data: *mut u8,
    length: usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            check(data, length)?;
            let end = offset.checked_add(length).ok_or_else(invalid)?;
            let ram = cpu.ram_region(base).ok_or_else(invalid)?;
            let range = ram.get(offset..end).ok_or_else(invalid)?;
            if length != 0 {
                ptr::copy_nonoverlapping(range.as_ptr(), data, length);
            }
            Ok(())
        })
    }
}

/// Copy bytes within one RAM region; validate the entire range before copying.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_ram_write(
    handle: *mut z180_machine,
    base: u32,
    offset: usize,
    data: *const u8,
    length: usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            check(data, length)?;
            let end = offset.checked_add(length).ok_or_else(invalid)?;
            let ram = cpu.ram_region_mut(base).ok_or_else(invalid)?;
            let range = ram.get_mut(offset..end).ok_or_else(invalid)?;
            range.copy_from_slice(input(data, length)?);
            Ok(())
        })
    }
}

/// Add a physical memory watch; returned nonzero ID survives state serialization.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_mem_watch_add(
    handle: *mut z180_machine,
    base: u32,
    size: u32,
    kind: u32,
    out_id: *mut u64,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_id)?;
            if base.checked_add(size).is_none() {
                return Err(invalid());
            }
            let kind = match kind {
                Z180_WATCH_READ => WatchKind::Read,
                Z180_WATCH_WRITE => WatchKind::Write,
                Z180_WATCH_BOTH => WatchKind::Both,
                _ => return Err(invalid()),
            };
            out_id.write(cpu.add_mem_watch(base, size, kind).to_u64());
            Ok(())
        })
    }
}

/// Remove a nonzero watch ID. An absent ID is a successful no-op.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_mem_watch_remove(
    handle: *mut z180_machine,
    id: u64,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.remove_mem_watch(WatchId::from_u64(id).ok_or_else(invalid)?);
            Ok(())
        })
    }
}

/// Set/clear the PC watch and reset its hit count.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_pc_watch(
    handle: *mut z180_machine,
    enabled: u8,
    address: u16,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_pc_watch(if boolean(enabled)? {
                Some(address)
            } else {
                None
            });
            Ok(())
        })
    }
}

/// Read PC watch hit count.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_pc_watch_hits(
    handle: *mut z180_machine,
    out_count: *mut u64,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_count)?;
            out_count.write(cpu.pc_watch_hits());
            Ok(())
        })
    }
}

/// Enable/disable event capture; enabled must be 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_io_trace(
    handle: *mut z180_machine,
    enabled: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_io_trace(boolean(enabled)?);
            Ok(())
        })
    }
}

/// Enable/disable event capture; enabled must be 0 or 1.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_irq_trace(
    handle: *mut z180_machine,
    enabled: u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.set_irq_trace(boolean(enabled)?);
            Ok(())
        })
    }
}

/// Configure instruction ring. Disabling clears it; allocation failure retains the prior capacity.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_set_insn_trace(
    handle: *mut z180_machine,
    enabled: u8,
    capacity: usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            if capacity > isize::MAX as usize / mem::size_of::<z180_core::TraceEntry>() {
                return Err(invalid());
            }
            cpu.set_insn_trace(if boolean(enabled)? {
                Some(capacity)
            } else {
                None
            });
            Ok(())
        })
    }
}

/// Read sticky event-loss indicator.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_events_lost(
    handle: *mut z180_machine,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_value)?;
            out_value.write(u8::from(cpu.events_lost()));
            Ok(())
        })
    }
}

/// Clear sticky event-loss indicator.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_clear_events_lost(
    handle: *mut z180_machine,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.clear_events_lost();
            Ok(())
        })
    }
}

/// Drain all retained events. A short/query buffer consumes nothing and reports required count.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_drain_events(
    handle: *mut z180_machine,
    events: *mut z180_event,
    capacity: usize,
    out_count: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_count)?;
            check(events, capacity)?;
            let count = cpu.event_count();
            out_count.write(count);
            if capacity < count {
                return Err(fail(Z180_BUFFER_TOO_SMALL, "event buffer too small"));
            }
            for (i, event) in cpu.drain_events_iter().enumerate() {
                events.add(i).write(event_record(event));
            }
            Ok(())
        })
    }
}

/// Drain all traces. A short/query buffer consumes nothing and reports required count.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_drain_insn_trace(
    handle: *mut z180_machine,
    entries: *mut z180_trace,
    capacity: usize,
    out_count: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_count)?;
            check(entries, capacity)?;
            let count = cpu.insn_trace_count();
            out_count.write(count);
            if capacity < count {
                return Err(fail(Z180_BUFFER_TOO_SMALL, "trace buffer too small"));
            }
            for (i, trace) in cpu.drain_insn_trace_iter().enumerate() {
                entries.add(i).write(z180_trace {
                    cycle: trace.cycle,
                    physical_pc: trace.phys_pc,
                    pc: trace.pc,
                    bytes: trace.bytes,
                    len: trace.len,
                });
            }
            Ok(())
        })
    }
}

/// Compute exact current size using temporary serialization. Does not cache a snapshot.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_save_state_size(
    handle: *mut z180_machine,
    out_size: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_size)?;
            let mut bytes = Vec::new();
            cpu.save_state_into(&mut bytes)
                .map_err(|e| fail(Z180_INTERNAL_ERROR, e))?;
            out_size.write(bytes.len());
            Ok(())
        })
    }
}

/// Save into caller storage. Failure sets written=0 and leaves storage unchanged.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_save_state(
    handle: *mut z180_machine,
    data: *mut u8,
    capacity: usize,
    out_written: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            output(out_written)?;
            out_written.write(0);
            check(data, capacity)?;
            // Storage may be uninitialized, so serialize separately and copy raw bytes.
            let mut bytes = Vec::new();
            cpu.save_state_into(&mut bytes)
                .map_err(|e| fail(Z180_INTERNAL_ERROR, e))?;
            if capacity < bytes.len() {
                return Err(fail(Z180_BUFFER_TOO_SMALL, SaveStateError::BufferTooSmall));
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            out_written.write(bytes.len());
            Ok(())
        })
    }
}

/// Load state; retains host bus and mapper. Invalid state leaves the machine unchanged.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_load_state(
    handle: *mut z180_machine,
    data: *const u8,
    length: usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        machine(handle, error, |cpu| {
            cpu.load_state(input(data, length)?)
                .map_err(|e| fail(Z180_INVALID_STATE, e))
        })
    }
}

/// Decode one instruction/data directive. Required text capacity includes NUL; empty input is invalid.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_disassemble(
    data: *const u8,
    length: usize,
    address: u16,
    out_instruction: *mut z180_instruction,
    text: *mut c_char,
    capacity: usize,
    out_required: *mut usize,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        boundary(error, || {
            output(out_instruction)?;
            output(out_required)?;
            check(text, capacity)?;
            let decoded =
                z180_core::disassemble_one(input(data, length)?, address).ok_or_else(invalid)?;
            let required = decoded.text.len() + 1;
            out_required.write(required);
            if capacity < required {
                return Err(fail(
                    Z180_BUFFER_TOO_SMALL,
                    "disassembly text buffer too small",
                ));
            }
            ptr::copy_nonoverlapping(decoded.text.as_ptr(), text.cast(), required - 1);
            text.add(required - 1).write(0);
            out_instruction.write(z180_instruction {
                address: decoded.address,
                bytes: decoded.bytes,
                len: decoded.len,
            });
            Ok(())
        })
    }
}

/// Query core opcode coverage.
///
/// # Safety
/// Follow the pointer, lifetime, exclusivity, and callback contracts in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z180_is_instruction_implemented(
    data: *const u8,
    length: usize,
    out_value: *mut u8,
    error: *mut z180_error,
) -> i32 {
    // SAFETY: foreign storage/lifetimes are the caller's contract; helpers validate shape.
    unsafe {
        boundary(error, || {
            output(out_value)?;
            out_value.write(u8::from(Z180::<CBus>::is_instruction_implemented(input(
                data, length,
            )?)));
            Ok(())
        })
    }
}

fn event_record(event: Event) -> z180_event {
    let mut r = z180_event::default();
    match event {
        Event::IoRead {
            cycle,
            pc,
            port,
            val,
        } => {
            r.kind = Z180_EVENT_IO_READ;
            r.cycle = cycle;
            r.pc = pc;
            r.port = port;
            r.value = val;
        }
        Event::IoWrite {
            cycle,
            pc,
            port,
            val,
        } => {
            r.kind = Z180_EVENT_IO_WRITE;
            r.cycle = cycle;
            r.pc = pc;
            r.port = port;
            r.value = val;
        }
        Event::MemRead {
            cycle,
            pc,
            phys,
            val,
        } => {
            r.kind = Z180_EVENT_MEM_READ;
            r.cycle = cycle;
            r.pc = pc;
            r.physical = phys;
            r.value = val;
        }
        Event::MemWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            r.kind = Z180_EVENT_MEM_WRITE;
            r.cycle = cycle;
            r.pc = pc;
            r.physical = phys;
            r.value = val;
        }
        Event::RomWrite {
            cycle,
            pc,
            phys,
            val,
        } => {
            r.kind = Z180_EVENT_ROM_WRITE;
            r.cycle = cycle;
            r.pc = pc;
            r.physical = phys;
            r.value = val;
        }
        Event::Trap {
            cycle,
            pc,
            opcode,
            len,
        } => {
            r.kind = Z180_EVENT_TRAP;
            r.cycle = cycle;
            r.pc = pc;
            r.opcode = opcode;
            r.len = len;
        }
        Event::IrqAck {
            cycle,
            source,
            vector,
        } => {
            r.kind = Z180_EVENT_IRQ_ACK;
            r.cycle = cycle;
            r.vector = vector;
            r.source = match source {
                IrqSource::Nmi => Z180_IRQ_NMI,
                IrqSource::Int0 => Z180_IRQ_INT0,
                IrqSource::Int1 => Z180_IRQ_INT1,
                IrqSource::Int2 => Z180_IRQ_INT2,
                IrqSource::Prt0 => Z180_IRQ_PRT0,
                IrqSource::Prt1 => Z180_IRQ_PRT1,
                IrqSource::Dma0 => Z180_IRQ_DMA0,
                IrqSource::Dma1 => Z180_IRQ_DMA1,
                IrqSource::Csio => Z180_IRQ_CSIO,
                IrqSource::Asci0 => Z180_IRQ_ASCI0,
                IrqSource::Asci1 => Z180_IRQ_ASCI1,
            };
        }
    }
    r
}

#[cfg(test)]
mod tests;
