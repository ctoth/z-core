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

mod bus;
use bus::*;
mod config;
use config::*;
mod events;
mod machine;
use events::*;

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
}
