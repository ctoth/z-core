; Assemble with z88dk 2.4:
; z88dk-z80asm -mz180 -b -o=fibonacci.bin fibonacci.asm

org 0

ld b, 0
ld c, 1
ld d, 10

fibonacci_loop:
ld a, b
add a, c
ld b, c
ld c, a
dec d
jr nz, fibonacci_loop

finished:
jr finished
