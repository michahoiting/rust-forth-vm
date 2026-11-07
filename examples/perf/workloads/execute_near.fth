\ Copyright (c) 2026 M.W.R. Hoiting

\ EXECUTE of a word close to the head of the word list, so that its cost doesn't depend on
\ how many words the VM has.
VARIABLE V
: INC V @ 1 + V ! ;
: NOP ;
' INC CONSTANT XT-INC
' NOP CONSTANT XT-NOP
: LEAF XT-NOP EXECUTE XT-INC EXECUTE ;
