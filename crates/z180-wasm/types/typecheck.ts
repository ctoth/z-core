import { Machine, Reg, WatchKind } from "../pkg/z180_wasm.js";
import type {
    Event,
    InstructionTraceEntry,
    MachineConfig,
    RamRegion,
    RegionConfig,
} from "../pkg/z180_wasm.js";

export function exerciseMachineTypes(): void {
    const config: MachineConfig = {
        clockHz: 12_288_000,
        physAddrBits: 20,
        unmappedRead: 0xff,
        variant: "Z8S180",
        eventCapacity: 128,
        regions: [
            { base: 0x00000, size: 0x01000, kind: "rom", data: new Uint8Array(0x1000) },
            { base: 0x01000, size: 0x0f000, kind: "ram" },
            { base: 0x10000, size: 0x01000, kind: "external" },
        ],
    };
    const machine = new Machine(config, {
        memRead: (physicalAddress) => physicalAddress & 0xff,
        memWrite: (_physicalAddress, _value) => undefined,
        ioRead: (port) => port & 0xff,
        ioWrite: (_port, _value) => undefined,
    });

    machine.setReg(Reg.PC, 0);
    machine.addMemWatch(0, 0x1000, WatchKind.Both);

    const events: Event[] = machine.drainEvents();
    const trace: InstructionTraceEntry[] = machine.drainInsnTrace();
    const regions: RamRegion[] = machine.ramRegions();
    void [events, trace, regions];
}

export function eventLocation(event: Event): number {
    switch (event.kind) {
        case "io_read":
        case "io_write":
            return event.port;
        case "mem_read":
        case "mem_write":
        case "rom_write":
            return event.phys;
        case "irq_ack":
            return event.vector;
        case "trap":
            return event.pc + event.opcode.length + event.len;
        default: {
            const unreachable: never = event;
            return unreachable;
        }
    }
}

// @ts-expect-error ROM regions require their byte contents.
export const invalidRomRegion: RegionConfig = {
    base: 0,
    size: 0x1000,
    kind: "rom",
};
