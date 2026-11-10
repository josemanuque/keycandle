// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
//
// rand()/srand() for the RGB effects (only Digital Rain uses rand()).
//
// newlib-nano's rand() keeps its state in the reentrancy structure, which drags in malloc, assert,
// fprintf and the stdio buffers: about 390 bytes of RAM and several KB of flash on a chip with 8 KB
// of RAM. Defining both functions here keeps the library versions out of the link. A 32-bit
// xorshift is plenty for lighting effects.
#include <stdlib.h>
#include <stdint.h>

static uint32_t state = 2463534242u;

void srand(unsigned seed) {
    state = seed ? seed : 2463534242u; /* xorshift must never be 0 */
}

int rand(void) {
    state ^= state << 13;
    state ^= state >> 17;
    state ^= state << 5;
    return (int)(state & RAND_MAX);
}
