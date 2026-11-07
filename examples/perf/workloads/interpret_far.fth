\ Copyright (c) 2026 M.W.R. Hoiting

\ The text interpreter with numbers and primitives. A number walks the whole word list before
\ it's parsed, so this scales with the number of words. It's reported but never fails.
VARIABLE N
\ MEASURE
1 2 + DROP 3 4 + DROP N @ 1 + N !
