\ Copyright (c) 2026 M.W.R. Hoiting

\ The code of defined words: DOCON, DOVAR and DODOES. Each call adds 10 to V.
7 CONSTANT K
VARIABLE V
: ADDER CREATE , DOES> @ + ;
3 ADDER P3
: LEAF V @ K + P3 V ! ;
