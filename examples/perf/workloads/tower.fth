\ Copyright (c) 2026 M.W.R. Hoiting

\ Each word calls the one below it twice, so Tn runs LEAF 2^n times. This gives long runs
\ without loops, and the return stack only needs n levels. Most workloads add this after
\ their own setup, which defines LEAF.
: T0 LEAF ;
: T1 T0 T0 ;
: T2 T1 T1 ;
: T3 T2 T2 ;
: T4 T3 T3 ;
: T5 T4 T4 ;
: T6 T5 T5 ;
: T7 T6 T6 ;
: T8 T7 T7 ;
: T9 T8 T8 ;
: T10 T9 T9 ;
: T11 T10 T10 ;
: T12 T11 T11 ;
: T13 T12 T12 ;
: T14 T13 T13 ;
: T15 T14 T14 ;
: T16 T15 T15 ;
: T17 T16 T16 ;
: T18 T17 T17 ;
: T19 T18 T18 ;
: T20 T19 T19 ;
