#include "z180.h"
#include <inttypes.h>
#include <stdio.h>

int main(void) {
    uint8_t rom[4096] = {0}; /* NOP */
    z180_config config;
    z180_error error;
    z180_machine *machine = NULL;
    z180_region regions[] = {
        {0, 4096, Z180_REGION_ROM, rom, sizeof rom},
        {4096, 4096, Z180_REGION_RAM, NULL, 0}
    };
    uint64_t cycles;
    uint16_t pc;
    if (z180_config_init(&config, &error) != Z180_OK ||
        z180_create(&config, regions, 2, NULL, &machine, &error) != Z180_OK ||
        z180_step(machine, &cycles, &error) != Z180_OK ||
        z180_reg_get(machine, Z180_REG_PC, &pc, &error) != Z180_OK) {
        fprintf(stderr, "%s\n", error.message);
        z180_destroy(machine);
        return 1;
    }
    printf("PC=%u cycles=%" PRIu64 "\n", (unsigned)pc, cycles);
    z180_destroy(machine);
    return 0;
}
