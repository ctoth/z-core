use std::ffi::{c_char, c_void};

pub const Z180_OK: i32 = 0;
pub const Z180_INVALID_ARGUMENT: i32 = 1;
pub const Z180_INVALID_CONFIG: i32 = 2;
pub const Z180_HOST_ERROR: i32 = 3;
pub const Z180_BUFFER_TOO_SMALL: i32 = 4;
pub const Z180_INVALID_STATE: i32 = 5;
pub const Z180_INTERNAL_ERROR: i32 = 6;
pub const Z180_POISONED: i32 = 7;
pub const Z180_Z80180: u32 = 0;
pub const Z180_Z8S180: u32 = 1;
pub const Z180_REGION_RAM: u32 = 0;
pub const Z180_REGION_ROM: u32 = 1;
pub const Z180_REGION_EXTERNAL: u32 = 2;
pub const Z180_REG_PC: u32 = 0;
pub const Z180_REG_SP: u32 = 1;
pub const Z180_REG_AF: u32 = 2;
pub const Z180_REG_BC: u32 = 3;
pub const Z180_REG_DE: u32 = 4;
pub const Z180_REG_HL: u32 = 5;
pub const Z180_REG_IX: u32 = 6;
pub const Z180_REG_IY: u32 = 7;
pub const Z180_REG_AF2: u32 = 8;
pub const Z180_REG_BC2: u32 = 9;
pub const Z180_REG_DE2: u32 = 10;
pub const Z180_REG_HL2: u32 = 11;
pub const Z180_REG_IR: u32 = 12;
pub const Z180_INT0: u32 = 0;
pub const Z180_INT1: u32 = 1;
pub const Z180_INT2: u32 = 2;
pub const Z180_WATCH_READ: u32 = 0;
pub const Z180_WATCH_WRITE: u32 = 1;
pub const Z180_WATCH_BOTH: u32 = 2;
pub const Z180_EVENT_IO_READ: u32 = 0;
pub const Z180_EVENT_IO_WRITE: u32 = 1;
pub const Z180_EVENT_MEM_READ: u32 = 2;
pub const Z180_EVENT_MEM_WRITE: u32 = 3;
pub const Z180_EVENT_IRQ_ACK: u32 = 4;
pub const Z180_EVENT_TRAP: u32 = 5;
pub const Z180_EVENT_ROM_WRITE: u32 = 6;
pub const Z180_IRQ_NMI: u32 = 0;
pub const Z180_IRQ_INT0: u32 = 1;
pub const Z180_IRQ_INT1: u32 = 2;
pub const Z180_IRQ_INT2: u32 = 3;
pub const Z180_IRQ_PRT0: u32 = 4;
pub const Z180_IRQ_PRT1: u32 = 5;
pub const Z180_IRQ_DMA0: u32 = 6;
pub const Z180_IRQ_DMA1: u32 = 7;
pub const Z180_IRQ_CSIO: u32 = 8;
pub const Z180_IRQ_ASCI0: u32 = 9;
pub const Z180_IRQ_ASCI1: u32 = 10;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct z180_config {
    pub clock_hz: u32,
    pub phys_addr_bits: u8,
    pub unmapped_read: u8,
    pub variant: u32,
    pub event_capacity: usize,
}
/// ROM bytes are copied. Non-ROM regions require data_len=0.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct z180_region {
    pub base: u32,
    pub size: u32,
    pub kind: u32,
    pub data: *const u8,
    pub data_len: usize,
}
/// Callback code and user_data must outlive the machine. Zero means success.
/// Reads must initialize out_value on success. No same-machine reentry or unwinding.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct z180_bus {
    pub user_data: *mut c_void,
    pub mem_read: Option<unsafe extern "C" fn(*mut c_void, u32, *mut u8) -> i32>,
    pub mem_write: Option<unsafe extern "C" fn(*mut c_void, u32, u8) -> i32>,
    pub io_read: Option<unsafe extern "C" fn(*mut c_void, u16, *mut u8) -> i32>,
    pub io_write: Option<unsafe extern "C" fn(*mut c_void, u16, u8) -> i32>,
}
/// Caller-owned diagnostic; optional on every fallible call. UTF-8, NUL terminated.
#[repr(C)]
pub struct z180_error {
    pub status: i32,
    pub host_code: i32,
    pub message: [c_char; 256],
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct z180_status {
    pub cycles: u64,
    pub instruction_pc: u16,
    pub halted: u8,
    pub sleeping: u8,
    pub iff1: u8,
    pub iff2: u8,
    pub interrupt_mode: u8,
    pub itc: u8,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct z180_ram_region {
    pub base: u32,
    pub size: u32,
}
/// Fields not applicable to kind are zero. PC is zero for IRQ acknowledgements.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct z180_event {
    pub cycle: u64,
    pub kind: u32,
    pub physical: u32,
    pub source: u32,
    pub pc: u16,
    pub port: u16,
    pub vector: u16,
    pub value: u8,
    pub opcode: [u8; 3],
    pub len: u8,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct z180_trace {
    pub cycle: u64,
    pub physical_pc: u32,
    pub pc: u16,
    pub bytes: [u8; 4],
    pub len: u8,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct z180_instruction {
    pub address: u16,
    pub bytes: [u8; 4],
    pub len: u8,
}
