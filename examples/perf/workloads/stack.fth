\ Copyright (c) 2026 M.W.R. Hoiting

\ The stack words. LEAF leaves the bottom Cell alone, and counts in the top one.
: LEAF 1 2 3 DUP DROP SWAP OVER ROT DROP DROP DROP 1 PICK DROP 2 ROLL SWAP DROP SWAP 1 + ;
0 0
