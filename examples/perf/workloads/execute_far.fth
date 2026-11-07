\ Copyright (c) 2026 M.W.R. Hoiting

\ EXECUTE of a primitive, at the far end of the word list. This scales with the number of
\ words, which is the cost in issue #39, so it's reported but never fails.
VARIABLE V
: INC V @ 1 + V ! ;
' DUP CONSTANT XT-DUP
: LEAF 1 XT-DUP EXECUTE DROP DROP INC ;
