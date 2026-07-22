export type Variant = "Z80180" | "Z8S180";

export interface RamRegionConfig {
    readonly base: number;
    readonly size: number;
    readonly kind: "ram";
}

export interface RomRegionConfig {
    readonly base: number;
    readonly size: number;
    readonly kind: "rom";
    readonly data: Uint8Array;
}

export interface ExternalRegionConfig {
    readonly base: number;
    readonly size: number;
    readonly kind: "external";
}

export type RegionConfig =
    | RamRegionConfig
    | RomRegionConfig
    | ExternalRegionConfig;

export interface MachineConfig {
    readonly clockHz?: number;
    readonly physAddrBits?: number;
    readonly unmappedRead?: number;
    readonly variant?: Variant;
    readonly regions?: readonly RegionConfig[];
    readonly eventCapacity?: number;
}

export type MemoryReadCallback = (physicalAddress: number) => number;
export type MemoryWriteCallback = (physicalAddress: number, value: number) => void;
export type IoReadCallback = (port: number) => number;
export type IoWriteCallback = (port: number, value: number) => void;

export interface MachineCallbacks {
    readonly memRead?: MemoryReadCallback;
    readonly memWrite?: MemoryWriteCallback;
    readonly ioRead?: IoReadCallback;
    readonly ioWrite?: IoWriteCallback;
}

export type ExternalAddressMapper = (physicalAddress: number) => number;
export type RegionKind = "ram" | "rom" | "external";
export type RamRegion = readonly [base: number, size: number];

export type IrqSource =
    | "nmi"
    | "int0"
    | "int1"
    | "int2"
    | "prt0"
    | "prt1"
    | "dma0"
    | "dma1"
    | "csio"
    | "asci0"
    | "asci1";

export interface IoReadEvent {
    readonly kind: "io_read";
    readonly cycle: bigint;
    readonly pc: number;
    readonly port: number;
    readonly value: number;
}

export interface IoWriteEvent {
    readonly kind: "io_write";
    readonly cycle: bigint;
    readonly pc: number;
    readonly port: number;
    readonly value: number;
}

export interface MemWriteEvent {
    readonly kind: "mem_write";
    readonly cycle: bigint;
    readonly pc: number;
    readonly phys: number;
    readonly value: number;
}

export interface MemReadEvent {
    readonly kind: "mem_read";
    readonly cycle: bigint;
    readonly pc: number;
    readonly phys: number;
    readonly value: number;
}

export interface IrqAckEvent {
    readonly kind: "irq_ack";
    readonly cycle: bigint;
    readonly source: IrqSource;
    readonly vector: number;
}

export interface TrapEvent {
    readonly kind: "trap";
    readonly cycle: bigint;
    readonly pc: number;
    readonly opcode: Uint8Array;
    readonly len: number;
}

export interface RomWriteEvent {
    readonly kind: "rom_write";
    readonly cycle: bigint;
    readonly pc: number;
    readonly phys: number;
    readonly value: number;
}

export type Event =
    | IoReadEvent
    | IoWriteEvent
    | MemWriteEvent
    | MemReadEvent
    | IrqAckEvent
    | TrapEvent
    | RomWriteEvent;

export interface InstructionTraceEntry {
    readonly cycle: bigint;
    readonly pc: number;
    readonly physPc: number;
    readonly bytes: Uint8Array;
    readonly len: number;
}

export class Machine {
    constructor(config?: MachineConfig | null, callbacks?: MachineCallbacks | null);
    free(): void;
    [Symbol.dispose](): void;
    reset(): void;
    step(): number;
    run(cycles: number): number;
    cycleCount(): bigint;
    halted(): boolean;
    sleeping(): boolean;
    reg(reg: Reg): number;
    setReg(reg: Reg, value: number): void;
    instructionPc(): number;
    iff1(): boolean;
    setIff1(enabled: boolean): void;
    iff2(): boolean;
    setIff2(enabled: boolean): void;
    interruptMode(): number;
    setInterruptMode(mode: number): void;
    setIrq(line: IrqLine, level: boolean): void;
    setNmi(level: boolean): void;
    setDreq(channel: number, level: boolean): void;
    ioRegPeek(internalAddress: number): number;
    mmuTranslate(logicalAddress: number): number;
    asciRxPush(channel: number, byte: number): boolean;
    asciTxPop(channel: number): number | undefined;
    csioRxPush(byte: number): boolean;
    csioTxPop(): number | undefined;
    setAsciCts(channel: number, level: boolean): void;
    setAsciDcd(channel: number, level: boolean): void;
    memPeek(physicalAddress: number): number;
    memPoke(physicalAddress: number, value: number): void;
    remap(
        base: number,
        size: number,
        kind: RegionKind,
        data?: Uint8Array | null,
    ): void;
    setExtMapper(mapper?: ExternalAddressMapper | null): void;
    ramRegions(): RamRegion[];
    ram(base: number): Uint8Array;
    loadRam(base: number, data: Uint8Array): void;
    addMemWatch(base: number, size: number, kind: WatchKind): WatchId;
    removeMemWatch(id: WatchId): void;
    setIoTrace(enabled: boolean): void;
    setIrqTrace(enabled: boolean): void;
    setPcWatch(address?: number | null): void;
    pcWatchHits(): bigint;
    drainEvents(): Event[];
    eventsLost(): boolean;
    clearEventsLost(): void;
    setInsnTrace(capacity?: number | null): void;
    drainInsnTrace(): InstructionTraceEntry[];
    saveState(): Uint8Array;
    loadState(data: Uint8Array): void;
    static isInstructionImplemented(opcodes: Uint8Array): boolean;
}
