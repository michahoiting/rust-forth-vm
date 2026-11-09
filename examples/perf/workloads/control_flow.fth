\ Copyright (c) 2026 M.W.R. Hoiting

\ Branching: a loop of four rounds with BEGIN and UNTIL, which takes 0BRANCH backward three
\ times, and an IF whose code runs once per call.
VARIABLE V
: LEAF 4 BEGIN 1- DUP 0= UNTIL 0= IF V @ 1 + V ! THEN ;
