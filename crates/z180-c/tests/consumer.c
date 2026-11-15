#include "z180.h"
#include "z180.h" /* Header must be safe to include twice. */
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define OK(call) assert((call) == Z180_OK)

struct board { unsigned reads, writes, io_reads, io_writes; uint8_t first_write; };

static int32_t memory_read(void *context, uint32_t address, uint8_t *value) {
    struct board *b = (struct board *)context;
    b->reads++;
    if (address == 1) return -42;
    *value = 0; /* NOP */
    return 0;
}
static int32_t memory_write(void *context, uint32_t address, uint8_t value) {
    struct board *b = (struct board *)context;
    (void)address;
    b->writes++;
    if (b->writes == 1) { b->first_write = value; return 0; }
    return -55;
}
static int32_t io_read(void *context, uint16_t port, uint8_t *value) {
    struct board *b = (struct board *)context;
    (void)port;
    b->io_reads++;
    *value = 0x7a;
    return 0;
}
static int32_t io_write(void *context, uint16_t port, uint8_t value) {
    struct board *b = (struct board *)context;
    (void)port; (void)value;
    b->io_writes++;
    return 0;
}

static void exercise_machine(void) {
    uint8_t rom[4096] = {0};
    z180_config config;
    z180_error error;
    z180_machine *cpu = NULL, *other = NULL;
    z180_region regions[2] = {
        {0, 4096, Z180_REGION_ROM, rom, sizeof rom},
        {4096, 4096, Z180_REGION_RAM, NULL, 0}
    };
    uint64_t cycles, watch;
    uint16_t reg;
    uint8_t value, present;
    size_t count, size, written;
    z180_status status;
    z180_event event;
    z180_trace trace;
    z180_ram_region ram;
    uint8_t sample[3] = {11, 22, 33}, copy[3] = {0};
    uint8_t *saved;
    uint32_t *mapping;
    uint32_t physical;
    OK(z180_config_init(&config, &error));
    assert(config.phys_addr_bits == 20 && config.unmapped_read == 255);
    assert(error.message[0] == 0);
    OK(z180_create(&config, regions, 2, NULL, &cpu, &error));
    OK(z180_create(&config, regions, 2, NULL, &other, &error));
    rom[0] = 0x76; /* Must not change either machine's owned ROM. */
    OK(z180_set_insn_trace(cpu, 1, 4, &error));
    OK(z180_mem_watch_add(cpu, 0, 4096, Z180_WATCH_READ, &watch, &error));
    assert(watch != 0);
    OK(z180_set_pc_watch(cpu, 1, 0, &error));
    OK(z180_step(cpu, &cycles, &error));
    assert(cycles > 0);
    OK(z180_get_status(cpu, &status, &error));
    assert(status.cycles == cycles && !status.halted);
    OK(z180_reg_get(cpu, Z180_REG_PC, &reg, &error)); assert(reg == 1);
    OK(z180_reg_get(other, Z180_REG_PC, &reg, &error)); assert(reg == 0);
    OK(z180_pc_watch_hits(cpu, &cycles, &error)); assert(cycles == 1);

    assert(z180_drain_events(cpu, NULL, 0, &count, &error) == Z180_BUFFER_TOO_SMALL);
    assert(count == 1);
    assert(z180_drain_events(cpu, NULL, 1, &count, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_drain_events(cpu, &event, 1, &count, &error));
    assert(count == 1 && event.kind == Z180_EVENT_MEM_READ && event.value == 0);
    assert(event.port == 0 && event.source == 0 && event.len == 0);
    assert(z180_drain_insn_trace(cpu, NULL, 0, &count, &error) == Z180_BUFFER_TOO_SMALL);
    OK(z180_drain_insn_trace(cpu, &trace, 1, &count, &error));
    assert(count == 1 && trace.pc == 0 && trace.bytes[0] == 0 && trace.len == 1);
    OK(z180_drain_insn_trace(cpu, NULL, 0, &count, &error)); assert(count == 0);
    OK(z180_events_lost(cpu, &value, &error)); assert(value == 0);
    OK(z180_clear_events_lost(cpu, &error));

    assert(z180_ram_regions(cpu, NULL, 0, &count, &error) == Z180_BUFFER_TOO_SMALL);
    OK(z180_ram_regions(cpu, &ram, 1, &count, &error));
    assert(ram.base == 4096 && ram.size == 4096);
    OK(z180_ram_write(cpu, 4096, 5, sample, 3, &error));
    OK(z180_ram_read(cpu, 4096, 5, copy, 3, &error)); assert(memcmp(copy, sample, 3) == 0);
    assert(z180_ram_write(cpu, 4096, 4095, sample, 3, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_mem_peek(cpu, 8191, &value, &error)); assert(value == 0);
    OK(z180_mem_poke(cpu, 0, 0xff, &error));
    OK(z180_mem_peek(cpu, 0, &value, &error)); assert(value == 0);
    OK(z180_mem_poke(cpu, 4096, 0x42, &error));
    OK(z180_mem_peek(cpu, 4096, &value, &error)); assert(value == 0x42);

    OK(z180_asci_tx_pop(cpu, 0, &present, &value, &error)); assert(!present && !value);
    OK(z180_csio_tx_pop(cpu, &present, &value, &error)); assert(!present);
    OK(z180_asci_rx_push(cpu, 0, 3, &present, &error)); assert(!present);
    OK(z180_csio_rx_push(cpu, 3, &present, &error)); assert(!present);
    assert(z180_asci_rx_push(cpu, 2, 3, &present, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_set_asci_cts(cpu, 1, 1, &error));
    OK(z180_set_asci_dcd(cpu, 1, &error));
    OK(z180_set_dreq(cpu, 0, 0, &error));
    OK(z180_set_irq(cpu, Z180_INT1, 0, &error));
    OK(z180_set_nmi(cpu, 0, &error));
    OK(z180_set_iff1(cpu, 1, &error));
    OK(z180_get_iff1(cpu, &value, &error)); assert(value == 1);
    OK(z180_set_iff2(cpu, 1, &error));
    OK(z180_get_iff2(cpu, &value, &error)); assert(value == 1);
    OK(z180_set_interrupt_mode(cpu, 2, &error));
    OK(z180_get_interrupt_mode(cpu, &value, &error)); assert(value == 2);
    assert(z180_set_interrupt_mode(cpu, 3, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_io_reg_peek(cpu, 0, &value, &error));
    assert(z180_io_reg_peek(cpu, 64, &value, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_mmu_translate(cpu, 123, &physical, &error)); assert(physical == 123);

    OK(z180_save_state_size(cpu, &size, &error)); assert(size > 4096);
    saved = (uint8_t *)malloc(size); assert(saved);
    written = 99;
    assert(z180_save_state(cpu, saved, 1, &written, &error) == Z180_BUFFER_TOO_SMALL);
    assert(written == 0);
    OK(z180_save_state(cpu, saved, size, &written, &error)); assert(written == size);
    OK(z180_mem_watch_remove(cpu, watch, &error));
    OK(z180_mem_poke(cpu, 4096, 0x99, &error));
    assert(z180_load_state(cpu, NULL, 0, &error) == Z180_INVALID_STATE);
    OK(z180_mem_peek(cpu, 4096, &value, &error)); assert(value == 0x99);
    OK(z180_load_state(cpu, saved, size, &error));
    free(saved);
    OK(z180_mem_peek(cpu, 4096, &value, &error)); assert(value == 0x42);
    OK(z180_step(cpu, &cycles, &error));
    OK(z180_drain_events(cpu, &event, 1, &count, &error)); assert(count == 1);
    OK(z180_mem_watch_remove(cpu, watch, &error));
    OK(z180_step(cpu, &cycles, &error));
    OK(z180_drain_events(cpu, NULL, 0, &count, &error)); assert(count == 0);
    assert(z180_mem_watch_remove(cpu, 0, &error) == Z180_INVALID_ARGUMENT);

    /* The mapping table is copied and survives load_state. */
    mapping = (uint32_t *)malloc((1u << 20) * sizeof *mapping); assert(mapping);
    for (size_t i = 0; i < (1u << 20); ++i) mapping[i] = (uint32_t)i;
    mapping[0] = 4096;
    OK(z180_mem_poke(cpu, 4096, 0x76, &error)); /* HALT via mapped fetch */
    OK(z180_save_state_size(cpu, &size, &error));
    saved = (uint8_t *)malloc(size); assert(saved);
    OK(z180_save_state(cpu, saved, size, &written, &error));
    OK(z180_set_ext_map_table(cpu, mapping, 1u << 20, &error));
    free(mapping);
    OK(z180_load_state(cpu, saved, size, &error));
    free(saved);
    OK(z180_reg_set(cpu, Z180_REG_PC, 0, &error));
    OK(z180_step(cpu, &cycles, &error));
    OK(z180_get_status(cpu, &status, &error)); assert(status.halted);
    cycles = status.cycles;
    OK(z180_reset(cpu, &error));
    OK(z180_get_status(cpu, &status, &error)); assert(status.cycles == cycles);
    OK(z180_set_ext_map_table(cpu, NULL, 0, &error));
    OK(z180_remap(cpu, &regions[1], &error));
    OK(z180_mem_peek(cpu, 4096, &value, &error)); assert(value == 0);
    z180_destroy(cpu); z180_destroy(other); z180_destroy(NULL);
}

static void exercise_callbacks(void) {
    z180_config config;
    z180_error error;
    z180_machine *cpu = NULL;
    struct board board = {0, 0, 0, 0, 0};
    z180_bus bus = {&board, memory_read, memory_write, io_read, io_write};
    z180_region external = {0, 4096, Z180_REGION_EXTERNAL, NULL, 0};
    uint64_t cycles;
    uint16_t reg;
    size_t size, written;
    uint8_t *saved;
    uint8_t program[4096] = {0xe5}; /* PUSH HL; second host write fails */
    z180_region rom = {0, 4096, Z180_REGION_ROM, program, sizeof program};
    z180_region stack = {0x3000, 4096, Z180_REGION_EXTERNAL, NULL, 0};
    OK(z180_config_init(&config, &error));
    OK(z180_create(&config, &external, 1, &bus, &cpu, &error));
    OK(z180_save_state_size(cpu, &size, &error));
    saved = (uint8_t *)malloc(size); assert(saved);
    OK(z180_save_state(cpu, saved, size, &written, &error));
    assert(z180_run(cpu, 100, &cycles, &error) == Z180_HOST_ERROR);
    assert(cycles > 0 && error.host_code == -42 && board.reads == 2);
    OK(z180_reg_get(cpu, Z180_REG_PC, &reg, &error)); assert(reg == 1);
    OK(z180_load_state(cpu, saved, size, &error));
    free(saved);
    assert(z180_run(cpu, 100, &cycles, &error) == Z180_HOST_ERROR);
    assert(error.host_code == -42 && board.reads == 4);
    OK(z180_remap(cpu, &rom, &error));
    OK(z180_remap(cpu, &stack, &error));
    OK(z180_reg_set(cpu, Z180_REG_PC, 0, &error));
    OK(z180_reg_set(cpu, Z180_REG_SP, 0x3002, &error));
    OK(z180_reg_set(cpu, Z180_REG_HL, 0x1234, &error));
    assert(z180_step(cpu, &cycles, &error) == Z180_HOST_ERROR);
    assert(error.host_code == -55 && board.writes == 2 && board.first_write == 0x12);
    assert(cycles == 0);
    OK(z180_reg_get(cpu, Z180_REG_SP, &reg, &error)); assert(reg == 0x3002);
    /* OUT0 (0),A and IN0 A,(0) still issue the duplicate host cycles. */
    program[0] = 0xed; program[1] = 0x39; program[2] = 0;
    program[3] = 0xed; program[4] = 0x38; program[5] = 0;
    OK(z180_remap(cpu, &rom, &error));
    OK(z180_set_io_trace(cpu, 1, &error));
    OK(z180_set_irq_trace(cpu, 1, &error));
    OK(z180_step(cpu, &cycles, &error));
    OK(z180_step(cpu, &cycles, &error));
    assert(board.io_writes == 1 && board.io_reads == 1);
    z180_destroy(cpu);
}

/* Drive internal I/O through real guest OUT0 instructions, using only the ABI. */
static void internal_write(z180_machine *cpu, uint8_t port, uint8_t value) {
    uint8_t instruction[] = {0xed, 0x39, port};
    uint64_t cycles;
    OK(z180_ram_write(cpu, 0, 0, instruction, sizeof instruction, NULL));
    OK(z180_reg_set(cpu, Z180_REG_AF, (uint16_t)((uint16_t)value << 8), NULL));
    OK(z180_reg_set(cpu, Z180_REG_PC, 0, NULL));
    OK(z180_step(cpu, &cycles, NULL));
}

static void exercise_serial_and_loss(void) {
    z180_config config;
    z180_region ram = {0, 4096, Z180_REGION_RAM, NULL, 0};
    z180_machine *cpu = NULL;
    uint8_t present, value, halt = 0x76;
    uint64_t cycles, watch;
    size_t count;
    z180_event event;
    OK(z180_config_init(&config, NULL));
    config.event_capacity = 1;
    OK(z180_create(&config, &ram, 1, NULL, &cpu, NULL));
    internal_write(cpu, 2, 0); /* CNTLB0: internal clock, /10 /16 */
    internal_write(cpu, 0, 0x64); /* CNTLA0: RX/TX enabled, 8N1 */
    internal_write(cpu, 6, 0xa5); /* TDR0 */
    OK(z180_asci_rx_push(cpu, 0, 0x5a, &present, NULL)); assert(present);
    OK(z180_asci_rx_push(cpu, 0, 0x33, &present, NULL)); assert(!present);
    OK(z180_mem_poke(cpu, 3, halt, NULL));
    OK(z180_run(cpu, 2000, &cycles, NULL));
    OK(z180_asci_tx_pop(cpu, 0, &present, &value, NULL)); assert(present && value == 0xa5);
    OK(z180_io_reg_peek(cpu, 8, &value, NULL)); assert(value == 0x5a);
    OK(z180_reset(cpu, NULL));
    internal_write(cpu, 11, 0x81); /* TRD */
    internal_write(cpu, 10, 0x10); /* CNTR transmit, fastest internal speed */
    OK(z180_mem_poke(cpu, 3, halt, NULL));
    OK(z180_run(cpu, 200, &cycles, NULL));
    OK(z180_csio_tx_pop(cpu, &present, &value, NULL)); assert(present && value == 0x81);
    OK(z180_reset(cpu, NULL));
    internal_write(cpu, 10, 0x20); /* CNTR receive */
    OK(z180_csio_rx_push(cpu, 0x92, &present, NULL)); assert(present);
    OK(z180_csio_rx_push(cpu, 0x43, &present, NULL)); assert(!present);
    OK(z180_run(cpu, 200, &cycles, NULL));
    OK(z180_io_reg_peek(cpu, 11, &value, NULL)); assert(value == 0x92);
    OK(z180_reset(cpu, NULL));
    OK(z180_mem_poke(cpu, 0, 0, NULL));
    OK(z180_mem_poke(cpu, 1, 0, NULL));
    OK(z180_mem_watch_add(cpu, 0, 2, Z180_WATCH_READ, &watch, NULL));
    OK(z180_step(cpu, &cycles, NULL));
    OK(z180_step(cpu, &cycles, NULL));
    OK(z180_events_lost(cpu, &value, NULL)); assert(value);
    OK(z180_drain_events(cpu, &event, 1, &count, NULL)); assert(count == 1 && event.physical == 1);
    OK(z180_events_lost(cpu, &value, NULL)); assert(value);
    OK(z180_clear_events_lost(cpu, NULL));
    OK(z180_events_lost(cpu, &value, NULL)); assert(!value);
    z180_destroy(cpu);
}

static void exercise_decode_and_creation_errors(void) {
    const uint8_t bytes[] = {0x3e, 0x42};
    const uint8_t truncated[] = {0xed};
    z180_instruction instruction;
    char text[64];
    size_t required;
    uint8_t implemented;
    z180_config config;
    z180_error error;
    z180_machine *cpu = NULL;
    OK(z180_config_init(&config, &error));
    config.variant = 500;
    assert(z180_create(&config, NULL, 0, NULL, &cpu, &error) == Z180_INVALID_ARGUMENT);
    assert(cpu == NULL);
    config.variant = Z180_Z80180; config.phys_addr_bits = 19;
    assert(z180_create(&config, NULL, 0, NULL, &cpu, &error) == Z180_INVALID_CONFIG);
    assert(cpu == NULL && error.message[0]);
    assert(z180_disassemble(bytes, 2, 0x1234, &instruction, NULL, 0, &required, &error) == Z180_BUFFER_TOO_SMALL);
    assert(required <= sizeof text);
    OK(z180_disassemble(bytes, 2, 0x1234, &instruction, text, sizeof text, &required, &error));
    assert(instruction.address == 0x1234 && instruction.len == 2 && text[required - 1] == 0);
    OK(z180_disassemble(truncated, 1, 0, &instruction, text, sizeof text, &required, &error));
    assert(instruction.len == 1);
    assert(z180_disassemble(NULL, 0, 0, &instruction, text, sizeof text, &required, &error) == Z180_INVALID_ARGUMENT);
    OK(z180_is_instruction_implemented(bytes, 1, &implemented, &error)); assert(implemented);
}

struct context_board { uint64_t cycles; uint16_t pc; unsigned reads; int32_t failure; };
static int32_t context_read(void *data, uint32_t address, const z180_bus_context *context, uint8_t *value) {
    struct context_board *b = (struct context_board *)data;
    assert(context->cycles == b->cycles && context->pc == b->pc);
    assert(address == b->pc);
    b->reads++; *value = 0; return b->failure;
}
static void exercise_context(void) {
    z180_config config; z180_machine *cpu = NULL; z180_error error; uint64_t elapsed;
    struct context_board board = {0, 0, 0, 0};
    z180_context_bus bus = {&board, context_read, NULL, NULL, NULL};
    z180_region region = {0, 4096, Z180_REGION_EXTERNAL, NULL, 0};
    OK(z180_config_init(&config, &error));
    OK(z180_create_with_context(&config, &region, 1, &bus, &cpu, &error));
    OK(z180_step(cpu, &elapsed, &error)); board.cycles += elapsed; board.pc = 1;
    OK(z180_step(cpu, &elapsed, &error)); board.cycles += elapsed; board.pc = 0;
    OK(z180_reset(cpu, &error));
    OK(z180_step(cpu, &elapsed, &error)); board.cycles += elapsed; board.pc = 1; board.failure = -73;
    assert(z180_step(cpu, &elapsed, &error) == Z180_HOST_ERROR);
    assert(error.host_code == -73 && board.reads == 4);
    z180_destroy(cpu);
}

struct fetch_board { uint32_t fetches[16], reads[32]; size_t nf, nr; int32_t failure; };
static void observe_fetch(void *data, uint32_t address) {
    struct fetch_board *b = (struct fetch_board *)data;
    assert(b->nf < 16); b->fetches[b->nf++] = address;
}
static int32_t fetch_read(void *data, uint32_t address, uint8_t *value) {
    const uint8_t program[] = {0x3e, 0x12, 0xdd, 0xcb, 1, 0x46, 0x76};
    struct fetch_board *b = (struct fetch_board *)data;
    assert(b->nr < 32); b->reads[b->nr++] = address;
    if (address == 0 || address == 2 || address == 3 || address == 5 || address == 6)
        assert(b->nf && b->fetches[b->nf - 1] == address);
    *value = address < sizeof program ? program[address] : 0;
    return b->failure;
}
static void exercise_fetch_observer(void) {
    struct fetch_board board = {{0}, {0}, 0, 0, 0};
    z180_bus bus = {&board, fetch_read, NULL, NULL, NULL};
    z180_fetch_observer observer = {&board, observe_fetch};
    z180_region region = {0, 65536, Z180_REGION_EXTERNAL, NULL, 0};
    z180_config config; z180_error error; z180_machine *cpu = NULL; uint64_t cycles;
    const uint32_t fetches[] = {0, 2, 3, 5, 6};
    const uint32_t reads[] = {0, 1, 2, 3, 4, 5, 0x101, 6, 7};
    size_t i;
    OK(z180_config_init(&config, &error));
    OK(z180_create(&config, &region, 1, &bus, &cpu, &error));
    OK(z180_set_fetch_observer(cpu, &observer, &error));
    OK(z180_reg_set(cpu, Z180_REG_IX, 0x100, &error));
    for (i = 0; i < 4; i++) OK(z180_step(cpu, &cycles, &error));
    assert(board.nf == sizeof fetches / sizeof *fetches && board.nr == sizeof reads / sizeof *reads);
    assert(memcmp(board.fetches, fetches, sizeof fetches) == 0);
    assert(memcmp(board.reads, reads, sizeof reads) == 0);
    OK(z180_reset(cpu, &error)); board.failure = -84;
    assert(z180_step(cpu, &cycles, &error) == Z180_HOST_ERROR);
    assert(error.host_code == -84 && board.nf == 6 && board.nr == 10);
    OK(z180_set_fetch_observer(cpu, NULL, &error));
    OK(z180_reset(cpu, &error)); board.failure = 0;
    /* Use unmapped PC so fetch_read's assertion does not require an observer. */
    OK(z180_reg_set(cpu, Z180_REG_PC, 0x200, &error));
    OK(z180_step(cpu, &cycles, &error)); assert(board.nf == 6);
    z180_destroy(cpu);
}

int main(void) {
    assert(z180_abi_version() == 1);
    exercise_machine();
    exercise_callbacks();
    exercise_serial_and_loss();
    exercise_decode_and_creation_errors();
    exercise_context();
    exercise_fetch_observer();
    puts("C ABI consumer passed");
    return 0;
}
