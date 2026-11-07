\ Copyright (c) 2026 M.W.R. Hoiting

\ The text interpreter with words close to the head of the word list and no numbers, so that
\ its cost doesn't depend on how many words the VM has.
VARIABLE N
: COUNT-LINE N @ 1 + N ! ;
: A ;
: B ;
: C ;
: D ;
\ MEASURE
A B C D A B C D A B C D A B C D COUNT-LINE
